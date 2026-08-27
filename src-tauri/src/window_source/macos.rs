//! Window geometry from the macOS window server, without consent.
//!
//! `CGWindowListCopyWindowInfo` is the one macOS API that hands over the shape
//! of the desktop for free. It reports bounds, owning application and window
//! level to any process; only `kCGWindowName` — the title — is withheld until
//! Screen Recording is granted, and this module never asks for it. That is what
//! makes the Spatial Layer work on first run, before the user has agreed to
//! anything.

use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFDictionary, CGRect};
use objc2_core_graphics::{
    CGDisplayBounds, CGError, CGGetActiveDisplayList, CGRectMakeWithDictionaryRepresentation,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::{ns_string, NSArray, NSDictionary, NSNumber, NSString};

use super::{Capabilities, Rect, WindowRect, WindowSource, WorldGeometry};

/// Enough for any desk. `CGGetActiveDisplayList` truncates rather than failing,
/// so the seventeenth display would simply be missing from the physics.
const MAX_DISPLAYS: u32 = 16;

/// The macOS window server's view of the desktop.
pub struct MacosWindowSource {
    /// Our own process. Its windows are excluded from every read: the overlay
    /// spans every display, so a sprite allowed to see it would find a Perch
    /// stretching across the whole desktop and never fall again.
    own_pid: i32,
}

impl MacosWindowSource {
    pub fn new() -> Self {
        Self {
            own_pid: std::process::id() as i32,
        }
    }
}

impl Default for MacosWindowSource {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowSource for MacosWindowSource {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            window_geometry: true,
            absolute_positioning: true,
        }
    }

    fn read(&self) -> WorldGeometry {
        WorldGeometry {
            display_frames: display_frames(),
            windows: visible_windows(self.own_pid),
        }
    }
}

/// Every active display's frame, in the global display coordinate space.
///
/// `CGDisplayBounds` is in points with the origin at the top-left of the main
/// display, which is the same space `kCGWindowBounds` uses. Asking AppKit
/// instead would mean converting out of its bottom-left, per-screen space for
/// no gain.
fn display_frames() -> Vec<Rect> {
    let mut ids = [0u32; MAX_DISPLAYS as usize];
    let mut count = 0u32;

    // SAFETY: both pointers address local storage, and `MAX_DISPLAYS` is the
    // true capacity of `ids`.
    let status = unsafe { CGGetActiveDisplayList(MAX_DISPLAYS, ids.as_mut_ptr(), &mut count) };
    if status != CGError::Success {
        return Vec::new();
    }

    ids[..count as usize]
        .iter()
        .map(|&id| rect(CGDisplayBounds(id)))
        .collect()
}

/// Visible windows, frontmost first.
///
/// `OnScreenOnly` already returns the list in front-to-back order, which is the
/// descending z-order the Engine wants, and `ExcludeDesktopElements` drops the
/// desktop image and its icons — a sprite should stand on the desktop, not on a
/// window covering it.
fn visible_windows(own_pid: i32) -> Vec<WindowRect> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, 0) else {
        return Vec::new();
    };

    // SAFETY: CFArray is toll-free bridged to NSArray, and this call's contract
    // is that every element is a CFDictionary keyed by CFString — bridged to
    // NSDictionary and NSString. Reading through the bridge costs nothing and
    // buys safe iteration and safe downcasts.
    let entries: &NSArray<NSDictionary<NSString, AnyObject>> =
        unsafe { &*std::ptr::from_ref(&*list).cast() };

    entries.iter().filter_map(|e| window(&e, own_pid)).collect()
}

/// One window-list entry, or `None` for entries we cannot or should not use.
///
/// The keys are spelled as literals because their `kCGWindow*` constants are
/// defined as exactly these strings, and a bridged dictionary compares string
/// keys by value.
fn window(entry: &NSDictionary<NSString, AnyObject>, own_pid: i32) -> Option<WindowRect> {
    if number(entry, ns_string!("kCGWindowOwnerPID"))?.as_i32() == own_pid {
        return None;
    }

    let bounds = entry.objectForKey(ns_string!("kCGWindowBounds"))?;
    let mut cg_rect = CGRect::ZERO;
    // SAFETY: `kCGWindowBounds` is documented to be a rectangle in the
    // dictionary representation this function decodes, and NSDictionary is
    // toll-free bridged to the CFDictionary it expects.
    let decoded = unsafe {
        CGRectMakeWithDictionaryRepresentation(
            Some(&*std::ptr::from_ref(&*bounds).cast::<CFDictionary>()),
            &mut cg_rect,
        )
    };
    if !decoded {
        return None;
    }

    Some(WindowRect {
        bounds: rect(cg_rect),
        owner: entry
            .objectForKey(ns_string!("kCGWindowOwnerName"))?
            .downcast::<NSString>()
            .ok()?
            .to_string(),
        layer: number(entry, ns_string!("kCGWindowLayer"))?.as_i32(),
    })
}

fn number(
    entry: &NSDictionary<NSString, AnyObject>,
    key: &NSString,
) -> Option<objc2::rc::Retained<NSNumber>> {
    entry.objectForKey(key)?.downcast::<NSNumber>().ok()
}

fn rect(cg_rect: CGRect) -> Rect {
    Rect {
        x: cg_rect.origin.x,
        y: cg_rect.origin.y,
        width: cg_rect.size.width,
        height: cg_rect.size.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand verification, deliberately not part of the suite: it needs a real
    /// window server and it reads a clock, both of which `docs/SPEC.md` rules
    /// out for `cargo test`. `#[ignore]` keeps the suite pure and fast.
    ///
    /// Run it with:
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml \
    ///     window_source -- --ignored --nocapture
    /// ```
    ///
    /// It prints the desktop as this module sees it, once per poll, whenever
    /// the geometry changes — so move, resize, open and close windows while it
    /// runs and watch the numbers follow. What no assertion can check is the
    /// thing the issue cares about most: that no permission dialog appeared.
    #[test]
    #[ignore = "needs a real desktop; run by hand"]
    fn live_desktop_geometry_follows_the_real_windows() {
        let source = MacosWindowSource::new();
        let start = std::time::Instant::now();
        let deadline = start + std::time::Duration::from_secs(5);
        let mut previous = None;
        let mut polls = 0u32;
        let mut spent = std::time::Duration::ZERO;

        while std::time::Instant::now() < deadline {
            let started = std::time::Instant::now();
            let geometry = source.snapshot();
            spent += started.elapsed();
            polls += 1;

            assert!(
                !geometry.display_frames.is_empty(),
                "a real desktop has at least one display"
            );
            assert!(
                !geometry.windows.is_empty(),
                "a real desktop has at least one window"
            );

            if previous.as_ref() != Some(&geometry) {
                println!(
                    "\nt+{:.1}s  displays: {:?}",
                    start.elapsed().as_secs_f64(),
                    geometry.display_frames
                );
                for w in &geometry.windows {
                    println!(
                        "  layer {:>3}  {:>7.0},{:<7.0} {:>6.0}x{:<6.0}  {}",
                        w.layer, w.bounds.x, w.bounds.y, w.bounds.width, w.bounds.height, w.owner
                    );
                }
                previous = Some(geometry);
            }

            std::thread::sleep(crate::window_source::POLL_INTERVAL);
        }

        // Wall time, most of it spent blocked on the window server rather than
        // on a CPU. Measure the process, not this number, when the question is
        // whether polling costs anything.
        println!(
            "\n{polls} polls, {:?} waiting on reads, {:?} each",
            spent,
            spent / polls
        );
    }
}
