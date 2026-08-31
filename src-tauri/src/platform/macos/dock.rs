//! The Dock's true bounds, from the best source the running system offers.
//!
//! `CGWindowListCopyWindowInfo` reports the Dock as a window covering the
//! whole display, and the work area only says which edge it reserves and how
//! thick the strip is — the Dock itself does not stretch to the sides of the
//! screen. Two APIs report the real rectangle, and this module tries them in
//! order, falling through silently:
//!
//! 1. **`CoreDockGetRect`** — private CoreDock SPI in ApplicationServices,
//!    adopted knowingly: it is exact for every user with zero grants, which
//!    is decision 9's no-permissions spirit carried further than the public
//!    API allows. Private means unversioned and removable, so it is resolved
//!    by name at runtime, never linked, and its answer is believed only after
//!    `plausible_dock` agrees it is shaped like a Dock. The release that
//!    removes or breaks it costs nothing but this rung.
//! 2. **The Accessibility API** — public and trust-gated. `DESIGN.md`
//!    decision 9 refuses to prompt, so this rung pays out only where trust
//!    was already granted, and `AXIsProcessTrusted` is asked before anything
//!    that could prompt.
//! 3. **Nothing** — the caller keeps the full-width work-area strip, which is
//!    the behavior this module exists to improve and the one it degrades back
//!    to, never a crash.
//!
//! The symbols are declared by hand rather than through a binding crate: a
//! handful of functions is fewer lines than a dependency, which is the trade
//! the Cargo manifest already names for CoreGraphics.
//!
//! Every other route was measured before settling on this order, so the next
//! hunt can start from its results instead of repeating them (macOS 26,
//! against the AX rect as ground truth):
//!
//! - **Every Dock-owned window in `CGWindowListCopyWindowInfo`**: three, all
//!   options included — the layer-20 window at exactly the display's frame
//!   and two wallpaper backstops. No window carries the island's rectangle;
//!   the magnification lens exists only mid-hover.
//! - **Estimating from consent-free inputs** (`com.apple.dock` defaults,
//!   running applications, Trash): 44–100 points short on the very desktop
//!   the spacing constants were tuned against, before recents tiles,
//!   minimized windows, spacers or magnification move them again. An
//!   estimate that is wrong by an icon or two puts the fall line mid-icon.
//! - **`AXUIElementCopyElementAtPosition`, screen capture**: the same trust
//!   gate as rung 2, or the Screen Recording consent the SPEC refuses.
//!
//! The two live rungs frame the Dock differently: CoreDock reports the
//! reserved strip (top at the work area's floor, down to the display's
//! bottom edge), AX reports the drawn island, a few points shorter at both
//! ends. Physics stands on the rectangle's top either way, and the strip's
//! top is the line #39 chose by hand, so the preferred rung is also the one
//! that keeps that line.

use std::ffi::c_void;

use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{CFString, CGPoint, CGSize};
use objc2_foundation::ns_string;

use ai_buddy_core::window_source::Rect;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const CFString,
        value: *mut *const c_void,
    ) -> i32;
    fn AXValueGetValue(value: *const c_void, value_type: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
}

/// `kAXValueCGPointType` and `kAXValueCGSizeType`, fixed since macOS 10.0.
const AX_VALUE_CG_POINT: u32 = 1;
const AX_VALUE_CG_SIZE: u32 = 2;

/// `kAXErrorSuccess`.
const AX_SUCCESS: i32 = 0;

use crate::platform::DockSource;

/// The Dock's real rectangle in points and where it came from, or `None`
/// when no source can say.
///
/// `None` when the SPI is gone and Accessibility is not granted (this never
/// prompts), when the Dock is not running, or when it sits on a side edge —
/// a side Dock is a wall the work area already places, and only a bottom
/// Dock leaves floor beside itself to give back. Coordinates arrive in the
/// same top-left global point space `CGWindowList` uses. The caller still
/// owes the answer a `plausible_dock` check against its displays: this
/// module can say "shaped like a bottom Dock", not "on your screen".
///
/// Call it on the main thread beside the display read: the calls are IPC to
/// the Dock, cheap at that cadence and not worth a poll of their own.
pub fn dock_bounds() -> Option<(Rect, DockSource)> {
    let horizontal = |bounds: Rect| {
        // A Dock on the left or the right is taller than wide. Its wall is
        // already where the work area puts it; nothing to report.
        (bounds.width > bounds.height && bounds.height > 0.0).then_some(bounds)
    };

    if let Some(bounds) = core_dock_rect().and_then(horizontal) {
        return Some((bounds, DockSource::CoreDock));
    }
    ax_bounds()
        .and_then(horizontal)
        .map(|bounds| (bounds, DockSource::Accessibility))
}

/// `CoreDockGetRect`, if this macOS still ships it.
///
/// Resolved by name on every call rather than linked: a link-time reference
/// to a private symbol turns its removal into a launch failure, and a lookup
/// turns it into `None`. `dlsym` costs nothing at a two-a-second cadence.
fn core_dock_rect() -> Option<Rect> {
    // The SPI takes a pointer to a CGRect and fills it; reverse-engineered,
    // stable in this shape for two decades, and trusted no further than the
    // caller's plausibility check.
    type CoreDockGetRect = unsafe extern "C" fn(*mut objc2_core_foundation::CGRect);

    // SAFETY: RTLD_DEFAULT (-2 on macOS) searches images already loaded, and
    // ApplicationServices is linked above for the AX symbols.
    let symbol = unsafe { dlsym(-2isize as *mut c_void, c"CoreDockGetRect".as_ptr()) };
    if symbol.is_null() {
        return None;
    }
    let mut rect = objc2_core_foundation::CGRect::ZERO;
    // SAFETY: dlsym proves the name resolves, never the arity or the ABI.
    // The signature is the SPI's long-known shape — out-pointer in, nothing
    // back — and a release that reshaped it would smash here, during the
    // call: the plausibility gate downstream can reject wrong values, never
    // a wrong ABI. That residual risk is the one the module doc records as
    // accepted knowingly. The out-pointer lives for the call.
    unsafe {
        let get_rect: CoreDockGetRect = std::mem::transmute(symbol);
        get_rect(&raw mut rect);
    }
    (rect.size.width > 0.0 && rect.size.height > 0.0).then_some(Rect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    })
}

unsafe extern "C" {
    /// libSystem's, linked into everything; no `#[link]` needed.
    fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
}

/// The Dock's rectangle over the Accessibility API, where trust already
/// exists. Never prompts.
fn ax_bounds() -> Option<Rect> {
    // SAFETY: takes nothing, returns a BOOL, documented callable anywhere.
    if !unsafe { AXIsProcessTrusted() } {
        return None;
    }

    let dock = NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .find(|app| {
            app.bundleIdentifier()
                .is_some_and(|id| id.isEqualToString(ns_string!("com.apple.dock")))
        })?;

    // SAFETY: a plain constructor over a pid; the element is released below.
    let element = unsafe { AXUIElementCreateApplication(dock.processIdentifier()) };
    if element.is_null() {
        return None;
    }
    let bounds = list_bounds(element);
    // SAFETY: releasing what AXUIElementCreateApplication handed over.
    unsafe { CFRelease(element) };
    bounds
}

/// The frame of the Dock application's list — the bar of icons itself, and
/// the only child the Dock exposes with a position and a size.
fn list_bounds(application: *const c_void) -> Option<Rect> {
    let children = copy_attribute(application, "AXChildren")?;
    // SAFETY: AXChildren is a CFArray of AXUIElements; count and index reads
    // stay inside it, and the array is released when `children` drops.
    unsafe {
        (0..CFArrayGetCount(children.0)).find_map(|index| {
            let child = CFArrayGetValueAtIndex(children.0, index);
            let mut origin = CGPoint::ZERO;
            let mut size = CGSize::ZERO;
            if !(ax_value(
                child,
                "AXPosition",
                AX_VALUE_CG_POINT,
                (&raw mut origin).cast(),
            ) && ax_value(child, "AXSize", AX_VALUE_CG_SIZE, (&raw mut size).cast()))
            {
                return None;
            }
            (size.width > 0.0 && size.height > 0.0).then_some(Rect {
                x: origin.x,
                y: origin.y,
                width: size.width,
                height: size.height,
            })
        })
    }
}

/// One AX attribute of `child`, decoded into `out` through `AXValueGetValue`.
///
/// SAFETY: the caller passes an `out` matching `value_type`, and the copied
/// attribute is released before returning.
unsafe fn ax_value(child: *const c_void, name: &str, value_type: u32, out: *mut c_void) -> bool {
    let Some(value) = copy_attribute(child, name) else {
        return false;
    };
    // SAFETY: `value` is the AXValue the attribute copy handed over, and the
    // caller's contract pairs `out` with `value_type`.
    unsafe { AXValueGetValue(value.0, value_type, out) }
}

/// A copied AX attribute that releases itself, so no exit path leaks one.
struct Copied(*const c_void);

impl Drop for Copied {
    fn drop(&mut self) {
        // SAFETY: the pointer came out of AXUIElementCopyAttributeValue,
        // which hands the caller ownership.
        unsafe { CFRelease(self.0) };
    }
}

fn copy_attribute(element: *const c_void, name: &str) -> Option<Copied> {
    let attribute = CFString::from_str(name);
    let mut value: *const c_void = std::ptr::null();
    // SAFETY: element and attribute are live for the call, and value receives
    // either null or an object this now owns.
    let status =
        unsafe { AXUIElementCopyAttributeValue(element, &raw const *attribute, &raw mut value) };
    (status == AX_SUCCESS && !value.is_null()).then_some(Copied(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand verification, like `window_source`'s live test: it needs a real
    /// Dock, and which rung answers depends on the OS and on an
    /// Accessibility decision, none of which `cargo test` may depend on. It
    /// prints the rectangle and the rung that produced it — `CoreDock` on
    /// any macOS still shipping the SPI, `Accessibility` from a trusted
    /// process after that, `None` last — and, critically, no permission
    /// dialog appears in any case.
    ///
    /// ```text
    /// cargo test dock -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a real Dock; run by hand"]
    fn live_dock_bounds_report_the_bar_and_their_source() {
        println!("dock_bounds: {:?}", dock_bounds());
    }
}
