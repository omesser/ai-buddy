//! The Dock's true bounds, over the one API that reports them.
//!
//! `CGWindowListCopyWindowInfo` reports the Dock as a window covering the
//! whole display, and the work area only says which edge it reserves and how
//! thick the strip is — the Dock itself does not stretch to the sides of the
//! screen. The real rectangle is only readable over the Accessibility API,
//! which is trust-gated. `DESIGN.md` decision 9 refuses to prompt, so this
//! module reads the rectangle when trust is already granted and answers
//! `None` otherwise — and the full-width strip stands in, as it always has.
//!
//! The AX symbols are declared by hand rather than through a binding crate:
//! four functions is fewer lines than a dependency, which is the trade the
//! Cargo manifest already names for CoreGraphics.

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

/// The Dock's real rectangle in points, or `None` when it cannot be read.
///
/// `None` when Accessibility is not granted (this never prompts), when the
/// Dock is not running, or when it sits on a side edge — a side Dock is a
/// wall the work area already places, and only a bottom Dock leaves floor
/// beside itself to give back. Coordinates arrive in the same top-left global
/// point space `CGWindowList` uses.
///
/// Call it on the main thread beside the display read: the AX calls are IPC
/// to the Dock, cheap at that cadence and not worth a poll of their own.
pub fn dock_bounds() -> Option<Rect> {
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
    let bounds = unsafe {
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
    }?;

    // A Dock on the left or the right is taller than wide. Its wall is
    // already where the work area puts it, so there is nothing to report.
    (bounds.width > bounds.height).then_some(bounds)
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
    /// Dock and an Accessibility decision, neither of which `cargo test` may
    /// depend on. From a trusted terminal it prints the Dock's rectangle;
    /// from an untrusted one it prints `None` and, critically, no permission
    /// dialog appears either way.
    ///
    /// ```text
    /// cargo test dock -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a real Dock and an Accessibility decision; run by hand"]
    fn live_dock_bounds_report_the_bar_not_the_display() {
        println!("dock_bounds: {:?}", dock_bounds());
    }
}
