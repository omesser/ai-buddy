//! Window geometry from the macOS window server, without consent.
//!
//! `CGWindowListCopyWindowInfo` is the one macOS API that hands over the shape
//! of the desktop for free. It reports bounds, owning application and window
//! level to any process; only `kCGWindowName` — the title — is withheld until
//! Screen Recording is granted. Geometry never reads that key. The MCP titles
//! resource does, from the same dictionary, so a grant reaches a Harness
//! without putting a title on `WindowRect`.

use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFDictionary, CGRect};
use objc2_core_graphics::{
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::{ns_string, NSArray, NSDictionary, NSNumber, NSString};

use ai_buddy_core::window_source::{Capabilities, Rect, WindowRect, WindowSource, WorldGeometry};

use crate::mcp_resources::WindowTitle;

/// The macOS window server's view of the desktop.
pub struct MacosWindowSource {
    /// Where the usable part of each display comes from, and the Dock's true
    /// bounds when Accessibility lets the Shell read them. Supplied rather
    /// than read here: reserved strips are the window manager's answer.
    read_displays: Box<dyn Fn() -> (Vec<Rect>, Option<Rect>) + Send + Sync>,
    can_read_titles: Box<dyn Fn() -> bool + Send + Sync>,
}

impl MacosWindowSource {
    pub fn new(
        read_displays: impl Fn() -> (Vec<Rect>, Option<Rect>) + Send + Sync + 'static,
        can_read_titles: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            read_displays: Box::new(read_displays),
            can_read_titles: Box::new(can_read_titles),
        }
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
        let (usable_frames, dock) = (self.read_displays)();
        let can_read_titles = (self.can_read_titles)();
        WorldGeometry {
            usable_frames,
            windows: visible_windows(can_read_titles),
            dock,
        }
    }
}

/// Visible windows, frontmost first — ours among them.
/// The overlay is a layer-3 panel `snapshot` already drops. Excluding our
/// process took Chat (#362) and Settings with it.
fn visible_windows(can_read_titles: bool) -> Vec<WindowRect> {
    walk_visible(|e| window(e, can_read_titles))
}

/// Owner plus title, same walk and order as `visible_windows`. Empty title
/// when Screen Recording is off: macOS leaves `kCGWindowName` blank.
pub fn visible_window_titles() -> Vec<WindowTitle> {
    walk_visible(window_title)
}

fn walk_visible<T>(map: impl Fn(&NSDictionary<NSString, AnyObject>) -> Option<T>) -> Vec<T> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, 0) else {
        return Vec::new();
    };

    // SAFETY: CFArray is toll-free bridged to NSArray; every element is a
    // CFDictionary keyed by CFString, bridged to NSDictionary and NSString.
    // The bridge buys safe iteration and downcasts.
    let entries: &NSArray<NSDictionary<NSString, AnyObject>> =
        unsafe { &*std::ptr::from_ref(&*list).cast() };

    entries.iter().filter_map(|e| map(&e)).collect()
}

/// One window-list entry, or `None` for entries we cannot or should not use.
/// Keys are literals because `kCGWindow*` constants are exactly these strings,
/// and a bridged dictionary compares string keys by value.
fn window(entry: &NSDictionary<NSString, AnyObject>, can_read_titles: bool) -> Option<WindowRect> {
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

    let title = if can_read_titles {
        entry
            .objectForKey(ns_string!("kCGWindowName"))
            .and_then(|obj| obj.downcast::<NSString>().ok())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    Some(WindowRect {
        // The window server's id, same call, no permission. `CGWindowID` is
        // 32-bit and `WindowId` is 64-bit, so `from` rather than `as` cannot
        // truncate. #85.
        id: u64::from(number(entry, ns_string!("kCGWindowNumber"))?.as_u32()),
        bounds: rect(cg_rect),
        owner: entry
            .objectForKey(ns_string!("kCGWindowOwnerName"))?
            .downcast::<NSString>()
            .ok()?
            .to_string(),
        title,
        layer: number(entry, ns_string!("kCGWindowLayer"))?.as_i32(),
    })
}

fn window_title(entry: &NSDictionary<NSString, AnyObject>) -> Option<WindowTitle> {
    let geometry = window(entry, false)?;
    let title = entry
        .objectForKey(ns_string!("kCGWindowName"))
        .and_then(|value| value.downcast::<NSString>().ok())
        .map(|name| name.to_string())
        .unwrap_or_default();
    Some(WindowTitle {
        owner: geometry.owner,
        title,
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
    /// ```text
    /// cargo test \
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
        // Displays are the window manager's answer and arrive from the Shell,
        // so this stands one in. Windows are what this test watches.
        let source = MacosWindowSource::new(|| {
            (
                vec![Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                }],
                None,
            )
        });
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
                !geometry.usable_frames.is_empty(),
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
                    geometry.usable_frames
                );
                for w in &geometry.windows {
                    println!(
                        "  #{:<6} layer {:>3}  {:>7.0},{:<7.0} {:>6.0}x{:<6.0}  {}",
                        w.id,
                        w.layer,
                        w.bounds.x,
                        w.bounds.y,
                        w.bounds.width,
                        w.bounds.height,
                        w.owner
                    );
                }
                previous = Some(geometry);
            }

            std::thread::sleep(ai_buddy_core::window_source::POLL_INTERVAL);
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
