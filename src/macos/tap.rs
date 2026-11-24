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

/// Marks events mfx posted so its own tap lets them through.
const MFX_TAG: i64 = 0x6d66_7821; // "mfx!"

/// Far-future fire date that parks the animation timer.
const PARKED: CFAbsoluteTime = 1.0e12;

static CURSOR_FROZEN: AtomicBool = AtomicBool::new(false);

struct Tap {
    engine: Engine,
    cfg: Config,
    port: CFMachPortRef,
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

/// Whether this process may observe and post input events. With `prompt`, macOS
/// shows its Accessibility dialog if not yet granted.
pub fn accessibility_trusted(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            return AXIsProcessTrusted();
        }
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
        let trusted = AXIsProcessTrustedWithOptions(opts);
        CFRelease(opts);
        trusted
    }
}

/// Installs the tap and animation timer on the main run loop. Events flow once
/// the caller runs that loop (the menu-bar app does).
pub fn install(cfg: Config, debug: bool) -> Result<(), String> {
    install_signal_handlers();

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
        if port.is_null() {
            return Err("could not create the event tap: Accessibility access is missing".into());
        }
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
                Some(Tap { engine: Engine::new(cfg.clone()), cfg, port, timer, enabled: true, debug })
        });
        CGEventTapEnable(port, true);
    }
    Ok(())
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
            eprintln!("mfx: event tap was disabled by macOS (type {ty:#x}); re-enabling");
            CGEventTapEnable(tap.port, true);
        }
        return event;
    }
    if CGEventGetIntegerValueField(event, kCGEventSourceUserData) == MFX_TAG {
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
        eprintln!("mfx: {input:?} -> {:?} {:?}", outcome.verdict, outcome.effects);
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
    CGEventSetIntegerValueField(ev, kCGEventSourceUserData, MFX_TAG);
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
