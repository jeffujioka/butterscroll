//! Minimal hand-written bindings for the CoreGraphics, CoreFoundation and
//! ApplicationServices calls butterscroll uses. Constants mirror CGEventTypes.h.

#![allow(non_upper_case_globals, non_camel_case_types)]

use std::ffi::c_void;

pub type CFTypeRef = *const c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type CFRunLoopTimerRef = *mut c_void;
pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFAbsoluteTime = f64;
pub type CGEventRef = *mut c_void;
pub type CGEventSourceRef = *mut c_void;
pub type CGEventType = u32;
pub type CGEventField = u32;
pub type CGEventMask = u64;
pub type CGEventFlags = u64;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

pub type CGEventTapCallBack =
    extern "C" fn(proxy: *mut c_void, ty: CGEventType, event: CGEventRef, user_info: *mut c_void) -> CGEventRef;
pub type CFRunLoopTimerCallBack = extern "C" fn(timer: CFRunLoopTimerRef, info: *mut c_void);

#[repr(C)]
pub struct CFRunLoopTimerContext {
    pub version: isize,
    pub info: *mut c_void,
    pub retain: *const c_void,
    pub release: *const c_void,
    pub copy_description: *const c_void,
}

// Event types
pub const kCGEventOtherMouseDown: CGEventType = 25;
pub const kCGEventOtherMouseUp: CGEventType = 26;
pub const kCGEventOtherMouseDragged: CGEventType = 27;
pub const kCGEventScrollWheel: CGEventType = 22;
pub const kCGEventTapDisabledByTimeout: CGEventType = 0xFFFF_FFFE;
pub const kCGEventTapDisabledByUserInput: CGEventType = 0xFFFF_FFFF;

// Tap placement
pub const kCGHIDEventTap: u32 = 0;
pub const kCGSessionEventTap: u32 = 1;
pub const kCGHeadInsertEventTap: u32 = 0;
pub const kCGEventTapOptionDefault: u32 = 0;

// Fields
pub const kCGMouseEventButtonNumber: CGEventField = 3;
pub const kCGMouseEventDeltaX: CGEventField = 4;
pub const kCGMouseEventDeltaY: CGEventField = 5;
pub const kCGScrollWheelEventDeltaAxis1: CGEventField = 11;
pub const kCGScrollWheelEventDeltaAxis2: CGEventField = 12;
pub const kCGEventSourceUserData: CGEventField = 42;
pub const kCGScrollWheelEventIsContinuous: CGEventField = 88;
pub const kCGScrollWheelEventFixedPtDeltaAxis1: CGEventField = 93;
pub const kCGScrollWheelEventFixedPtDeltaAxis2: CGEventField = 94;
pub const kCGScrollWheelEventPointDeltaAxis1: CGEventField = 96;
pub const kCGScrollWheelEventPointDeltaAxis2: CGEventField = 97;
pub const kCGScrollWheelEventScrollPhase: CGEventField = 99;

// Scroll phases (CGScrollPhase)
pub const kCGScrollPhaseBegan: i64 = 1;
pub const kCGScrollPhaseChanged: i64 = 2;
pub const kCGScrollPhaseEnded: i64 = 4;

pub const kCGScrollEventUnitPixel: u32 = 0;

pub const fn event_mask_bit(ty: CGEventType) -> CGEventMask {
    1 << ty
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    pub fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    pub fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    pub fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
    pub fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: CGEventField, value: i64);
    pub fn CGEventGetDoubleValueField(event: CGEventRef, field: CGEventField) -> f64;
    pub fn CGEventSetDoubleValueField(event: CGEventRef, field: CGEventField, value: f64);
    pub fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, keycode: u16, key_down: bool) -> CGEventRef;
    pub fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        ty: CGEventType,
        location: CGPoint,
        button: u32,
    ) -> CGEventRef;
    pub fn CGEventCreateScrollWheelEvent2(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGAssociateMouseAndMouseCursorPosition(connected: bool) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFBooleanTrue: CFTypeRef;
    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFMachPortInvalidate(port: CFMachPortRef);
    pub fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    pub fn CFRunLoopGetMain() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFStringRef);
    pub fn CFAbsoluteTimeGetCurrent() -> CFAbsoluteTime;
    pub fn CFRunLoopTimerCreate(
        allocator: CFAllocatorRef,
        fire_date: CFAbsoluteTime,
        interval: f64,
        flags: u64,
        order: isize,
        callout: CFRunLoopTimerCallBack,
        context: *mut CFRunLoopTimerContext,
    ) -> CFRunLoopTimerRef;
    pub fn CFRunLoopTimerSetNextFireDate(timer: CFRunLoopTimerRef, fire_date: CFAbsoluteTime);
    pub fn CFRunLoopTimerInvalidate(timer: CFRunLoopTimerRef);
    pub fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

extern "C" {
    pub fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    pub fn _exit(status: i32) -> !;
}

pub const SIGINT: i32 = 2;
pub const SIGTERM: i32 = 15;
pub const SIGHUP: i32 = 1;
