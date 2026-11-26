//! CGEventTap wiring: translate CoreGraphics events into engine inputs and
//! engine effects into posted events. Everything here runs on the main thread.

// CoreGraphics constants keep their C names, including in match patterns.
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::ffi::*;
use crate::config::Config;
use crate::engine::{Effect, Engine, Input, ScrollPhase, Verdict};
use crate::keys::Shortcut;
use crate::scroll::FRAME;

/// Marks events butterscroll posted so its own tap lets them through.
const BUTTERSCROLL_TAG: i64 = 0x6273_6372_6f6c_6c21; // "bscroll!"

/// Far-future fire date that parks the animation timer.
const PARKED: CFAbsoluteTime = 1.0e12;

static CURSOR_FROZEN: AtomicBool = AtomicBool::new(false);

struct Tap {
    engine: Engine,
    cfg: Config,
    port: CFMachPortRef,
    source: CFRunLoopSourceRef,
    timer: CFRunLoopTimerRef,
    enabled: bool,
    debug: bool,
}

thread_local! {
    static TAP: RefCell<Option<Tap>> = const { RefCell::new(None) };
}

/// Runs `f` on the installed tap; `None` if not installed or already borrowed.
fn with_tap<R>(f: impl FnOnce(&mut Tap) -> R) -> Option<R> {
    TAP.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Asks macOS to show its permission dialog if butterscroll has no access yet. On macOS
/// 27.3 and later this lists butterscroll under System & App Protection Override.
pub fn request_access() {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt as CFTypeRef];
        let values = [kCFBooleanTrue];
        let opts = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),
            ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast(),
        );
        AXIsProcessTrustedWithOptions(opts);
        CFRelease(opts);
    }
}

/// Creates the event tap; null while macOS denies it or leaves it disabled. Whether the tap can be
/// created is the only reliable permission test: `AXIsProcessTrusted` ignores
/// the System & App Protection Override grant of macOS 27.3.
fn create_port() -> CFMachPortRef {
    let mask = [kCGEventOtherMouseDown, kCGEventOtherMouseUp, kCGEventOtherMouseDragged, kCGEventScrollWheel]
        .into_iter()
        .fold(0, |m, t| m | event_mask_bit(t));
    unsafe {
        let port = CGEventTapCreate(
            kCGSessionEventTap,
            kCGHeadInsertEventTap,
            kCGEventTapOptionDefault,
            mask,
            tap_callback,
            ptr::null_mut(),
        );
        if !port.is_null() && !CGEventTapIsEnabled(port) {
            CFMachPortInvalidate(port);
            CFRelease(port);
            return ptr::null_mut();
        }
        port
    }
}

/// Whether macOS lets this process create the event tap.
pub fn can_create() -> bool {
    let port = create_port();
    if port.is_null() {
        return false;
    }
    unsafe {
        CFMachPortInvalidate(port);
        CFRelease(port);
    }
    true
}

/// Installs the tap and animation timer on the main run loop. Events flow once
/// the caller runs that loop (the menu-bar app does). Returns false, without
/// calling `load_cfg`, while macOS denies the tap.
pub fn install(load_cfg: impl FnOnce() -> Config, debug: bool) -> bool {
    let port = create_port();
    if port.is_null() {
        return false;
    }
    install_signal_handlers();
    let cfg = load_cfg();

    unsafe {
        let run_loop = CFRunLoopGetMain();
        let source = CFMachPortCreateRunLoopSource(ptr::null(), port, 0);
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

        let mut ctx = CFRunLoopTimerContext {
            version: 0,
            info: ptr::null_mut(),
            retain: ptr::null(),
            release: ptr::null(),
            copy_description: ptr::null(),
        };
        let timer = CFRunLoopTimerCreate(ptr::null(), PARKED, FRAME.as_secs_f64(), 0, 0, timer_callback, &mut ctx);
        CFRunLoopAddTimer(run_loop, timer, kCFRunLoopCommonModes);

        TAP.with(|cell| {
            *cell.borrow_mut() =
                Some(Tap { engine: Engine::new(cfg.clone()), cfg, port, source, timer, enabled: true, debug })
        });
        CGEventTapEnable(port, true);
    }
    true
}

/// Whether a tap that should be `wanted` is still delivering events. A tap macOS
/// disabled gets one re-enable; one that stays disabled is dead. A paused tap is
/// disabled on purpose and counts as alive.
fn tap_alive(wanted: bool, is_enabled: impl Fn() -> bool, enable: impl FnOnce()) -> bool {
    if !wanted || is_enabled() {
        return true;
    }
    enable();
    is_enabled()
}

/// Checks the installed tap, re-enabling it if macOS disabled it. False once it
/// cannot be revived: the grant no longer matches the binary, or macOS revoked it.
pub fn is_alive() -> bool {
    with_tap(|tap| {
        let port = tap.port;
        tap_alive(tap.enabled, || unsafe { CGEventTapIsEnabled(port) }, || unsafe { CGEventTapEnable(port, true) })
    })
    .unwrap_or(true)
}

/// Removes the tap and animation timer, so `install` can create fresh ones.
pub fn uninstall() {
    let Some(mut tap) = TAP.with(|cell| cell.try_borrow_mut().ok()?.take()) else { return };
    unsafe {
        post_all(&tap.engine.stop_scrolling());
        CFRunLoopTimerInvalidate(tap.timer);
        CFRelease(tap.timer);
        CFMachPortInvalidate(tap.port);
        CFRelease(tap.source);
        CFRelease(tap.port);
    }
    set_cursor_frozen(false);
}

/// Swaps in a new config; any drag or scroll in progress is dropped.
pub fn reload(cfg: Config) {
    with_tap(|tap| {
        unsafe { post_all(&tap.engine.stop_scrolling()) };
        tap.engine = Engine::new(cfg.clone());
        tap.cfg = cfg;
    });
    set_cursor_frozen(false);
}

/// Pauses or resumes all interception.
pub fn set_enabled(enabled: bool) {
    with_tap(|tap| {
        tap.enabled = enabled;
        unsafe { post_all(&tap.engine.stop_scrolling()) };
        tap.engine = Engine::new(tap.cfg.clone());
        unsafe { CGEventTapEnable(tap.port, enabled) };
    });
    set_cursor_frozen(false);
}

extern "C" fn tap_callback(_proxy: *mut c_void, ty: CGEventType, event: CGEventRef, _info: *mut c_void) -> CGEventRef {
    // A panic must not unwind into CoreGraphics; on failure the event passes unchanged.
    std::panic::catch_unwind(|| with_tap(|tap| unsafe { handle_event(tap, ty, event) }).unwrap_or(event))
        .unwrap_or(event)
}

unsafe fn handle_event(tap: &mut Tap, ty: CGEventType, event: CGEventRef) -> CGEventRef {
    if ty == kCGEventTapDisabledByTimeout || ty == kCGEventTapDisabledByUserInput {
        if tap.enabled {
            eprintln!("butterscroll: event tap was disabled by macOS (type {ty:#x}); re-enabling");
            CGEventTapEnable(tap.port, true);
        }
        return event;
    }
    if CGEventGetIntegerValueField(event, kCGEventSourceUserData) == BUTTERSCROLL_TAG {
        return event;
    }

    let button = || CGEventGetIntegerValueField(event, kCGMouseEventButtonNumber) as u32;
    let input = match ty {
        kCGEventOtherMouseDown => Input::ButtonDown { button: button() },
        kCGEventOtherMouseUp => Input::ButtonUp { button: button() },
        kCGEventOtherMouseDragged => Input::Dragged {
            button: button(),
            dx: CGEventGetDoubleValueField(event, kCGMouseEventDeltaX),
            dy: CGEventGetDoubleValueField(event, kCGMouseEventDeltaY),
        },
        kCGEventScrollWheel => Input::Wheel {
            lines: [
                CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis2),
                CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis1),
            ],
            continuous: CGEventGetIntegerValueField(event, kCGScrollWheelEventIsContinuous) != 0,
            flags: CGEventGetFlags(event),
        },
        _ => return event,
    };

    let was_animating = tap.engine.is_animating();
    let outcome = tap.engine.handle(input, Instant::now());
    if tap.debug && (outcome.verdict != Verdict::Pass || !outcome.effects.is_empty()) {
        eprintln!("butterscroll: {input:?} -> {:?} {:?}", outcome.verdict, outcome.effects);
    }
    for effect in &outcome.effects {
        post(effect);
    }
    if !was_animating && tap.engine.is_animating() {
        CFRunLoopTimerSetNextFireDate(tap.timer, CFAbsoluteTimeGetCurrent());
    }

    match outcome.verdict {
        Verdict::Pass => event,
        Verdict::Swallow => ptr::null_mut(),
        Verdict::InvertScroll => {
            invert_scroll(event);
            event
        }
    }
}

extern "C" fn timer_callback(timer: CFRunLoopTimerRef, _info: *mut c_void) {
    let _ = std::panic::catch_unwind(|| {
        with_tap(|tap| unsafe {
            post_all(&tap.engine.frame(Instant::now()));
            if !tap.engine.is_animating() {
                CFRunLoopTimerSetNextFireDate(timer, PARKED);
            }
        })
    });
}

unsafe fn invert_scroll(event: CGEventRef) {
    for field in [kCGScrollWheelEventDeltaAxis1, kCGScrollWheelEventDeltaAxis2, kCGScrollWheelEventPointDeltaAxis1, kCGScrollWheelEventPointDeltaAxis2] {
        CGEventSetIntegerValueField(event, field, -CGEventGetIntegerValueField(event, field));
    }
    for field in [kCGScrollWheelEventFixedPtDeltaAxis1, kCGScrollWheelEventFixedPtDeltaAxis2] {
        CGEventSetDoubleValueField(event, field, -CGEventGetDoubleValueField(event, field));
    }
}

unsafe fn post_all(effects: &[Effect]) {
    for effect in effects {
        post(effect);
    }
}

unsafe fn post(effect: &Effect) {
    match *effect {
        Effect::Key(s) => post_shortcut(s),
        Effect::Click { button } => {
            let here = CGEventCreate(ptr::null_mut());
            let at = CGEventGetLocation(here);
            CFRelease(here);
            for ty in [kCGEventOtherMouseDown, kCGEventOtherMouseUp] {
                let ev = CGEventCreateMouseEvent(ptr::null_mut(), ty, at, button);
                CGEventSetIntegerValueField(ev, kCGMouseEventButtonNumber, button as i64);
                post_tagged(ev);
            }
        }
        Effect::Scroll { px, phase, flags } => {
            let ev = CGEventCreateScrollWheelEvent2(ptr::null_mut(), kCGScrollEventUnitPixel, 2, px[1], px[0], 0);
            CGEventSetIntegerValueField(ev, kCGScrollWheelEventIsContinuous, 1);
            let phase = match phase {
                ScrollPhase::None => 0,
                ScrollPhase::Began => kCGScrollPhaseBegan,
                ScrollPhase::Changed => kCGScrollPhaseChanged,
                ScrollPhase::Ended => kCGScrollPhaseEnded,
            };
            CGEventSetIntegerValueField(ev, kCGScrollWheelEventScrollPhase, phase);
            CGEventSetFlags(ev, flags);
            post_tagged(ev);
        }
        Effect::FreezeCursor(freeze) => set_cursor_frozen(freeze),
    }
}

unsafe fn post_shortcut(s: Shortcut) {
    for down in [true, false] {
        let ev = CGEventCreateKeyboardEvent(ptr::null_mut(), s.keycode, down);
        CGEventSetFlags(ev, s.flags);
        post_tagged(ev);
    }
}

unsafe fn post_tagged(ev: CGEventRef) {
    if ev.is_null() {
        return;
    }
    CGEventSetIntegerValueField(ev, kCGEventSourceUserData, BUTTERSCROLL_TAG);
    CGEventPost(kCGHIDEventTap, ev);
    CFRelease(ev);
}

fn set_cursor_frozen(freeze: bool) {
    CURSOR_FROZEN.store(freeze, Ordering::SeqCst);
    unsafe {
        CGAssociateMouseAndMouseCursorPosition(!freeze);
    }
}

extern "C" fn on_signal(_sig: i32) {
    // Never leave the cursor stuck if killed mid-drag.
    if CURSOR_FROZEN.load(Ordering::SeqCst) {
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(true);
        }
    }
    unsafe { _exit(0) }
}

fn install_signal_handlers() {
    for sig in [SIGINT, SIGTERM, SIGHUP] {
        unsafe {
            signal(sig, on_signal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn disabled_tap_that_enable_cannot_revive_is_dead() {
        let enable_calls = Cell::new(0);
        let alive = tap_alive(true, || false, || enable_calls.set(enable_calls.get() + 1));
        assert!(!alive);
        assert_eq!(enable_calls.get(), 1);
    }

    #[test]
    fn disabled_tap_that_enable_revives_is_alive() {
        let enabled = Cell::new(false);
        assert!(tap_alive(true, || enabled.get(), || enabled.set(true)));
    }

    #[test]
    fn enabled_tap_is_left_alone() {
        assert!(tap_alive(true, || true, || panic!("must not re-enable a healthy tap")));
    }

    #[test]
    fn paused_tap_is_alive_even_though_disabled() {
        assert!(tap_alive(false, || false, || panic!("a paused tap stays disabled")));
    }
}
