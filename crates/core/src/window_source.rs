//! Where the windows are.
//!
//! The Spatial Layer needs window geometry before the user has granted
//! anything, so this layer is deliberately built out of what the operating
//! system hands over for free. Everything a permission prompt would buy —
//! window titles above all — is absent by construction rather than by policy.

use std::time::Duration;

/// How often the Shell should read the source while the sprite is still.
///
/// Windows move at human speed, and a sitting buddy does not need the list
/// sixty times a second. A ride switches to `RIDE_POLL_INTERVAL`. #98.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How often the Shell should read the source while the sprite is riding.
///
/// The OS paints the window every frame; 10 Hz left a hitch the derivatives
/// could not hide. Only a ride pays this rate, so the list stays off the
/// battery budget the rest of the day. #98.
pub const RIDE_POLL_INTERVAL: Duration = Duration::from_millis(16);

/// What a platform can actually tell us about the desktop.
///
/// Declared, never assumed. The Wayland arm declares neither and the Spatial
/// Layer degrades to screen-edge physics, which is a supported mode rather
/// than an error state. DESIGN.md decision 3 says which limit is permanent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Whether the platform reports the rectangles of other applications'
    /// windows. Without it there are no Perches.
    pub window_geometry: bool,
    /// Whether a window can be placed at chosen coordinates. Without it the
    /// overlay goes wherever the compositor puts it.
    pub absolute_positioning: bool,
}

/// A rectangle in the global display coordinate space, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub(crate) fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub(crate) fn spans_x(&self, x: f64) -> bool {
        x >= self.x && x <= self.x + self.width
    }
}

/// The window server's handle for one window.
///
/// An opaque token. Nothing above this layer interprets one, orders them or
/// reads meaning into the number: they are only ever compared for equality,
/// which is all the Engine needs to say two snapshots describe the same
/// window. #85.
///
/// `u64` because it has to be wide enough for any platform's window handle
/// without a lossy conversion, and pointer width is the widest one on offer: a
/// macOS `CGWindowID` is 32-bit, a Windows `HWND` is a handle and so pointer
/// width, and an X11 `Window` is an `XID`, an `unsigned long`. Each of those
/// widens into this. A platform converts at its own boundary; the core never
/// learns which one it came from.
pub type WindowId = u64;

/// The Perch id the Dock stands behind when its true bounds are known.
///
/// The Dock is not an application window, so the window server never hands it
/// a place in the id space this could collide with: every macOS `CGWindowID`
/// is 32-bit and widens into the low half of `WindowId`. A constant id keeps
/// the Dock the same Perch across polls, which is what lets a sprite ride a
/// Dock that resizes (#109). It lives here beside `WindowId` rather than with
/// the snapshot assembler that stamps it, because the Engine reads it too and
/// nothing below it should have to depend on the layer above.
pub const DOCK_PERCH_ID: WindowId = WindowId::MAX;

/// One visible window: which one it is, where it is, who owns it, and how high
/// it stacks.
///
/// No title. Titles need Screen Recording consent, and v1 asks for nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowRect {
    /// The window server's own id, carried all the way to the Engine. Geometry
    /// alone cannot say that the window under the sprite this tick is the one
    /// it stood on last tick, and guessing that from size and displacement is
    /// identity by another name. #85.
    pub id: WindowId,
    pub bounds: Rect,
    /// The owning application's name, as the window server reports it.
    pub owner: String,
    /// The window server's level: 0 for ordinary application windows, higher
    /// for menus, docks and other overlays, lower for the desktop picture and
    /// its notifications. Reported rather than acted on here: the Shell reads
    /// it when it assembles a `WorldSnapshot` and keeps only the levels a
    /// sprite may stand on, so the Engine never sees a level at all.
    pub layer: i32,
}

/// A rectangle the window server measured in physical pixels, in points.
///
/// The scale passed is always the scale of the display the rectangle was
/// measured on, never the primary's: each display reports its geometry against
/// its own factor, and two of the four bugs `docs/SPEC.md` lists were one
/// factor used across two of them.
///
/// A scale of zero or less would divide every coordinate into infinity, so it
/// counts as 1. A display describing itself as nonsense should still be
/// somewhere the sprite can stand.
pub fn in_points(rect_physical: Rect, scale: f64) -> Rect {
    let scale = if scale > 0.0 { scale } else { 1.0 };

    Rect {
        x: rect_physical.x / scale,
        y: rect_physical.y / scale,
        width: rect_physical.width / scale,
        height: rect_physical.height / scale,
    }
}

/// The part of a display a sprite may occupy, in logical points.
///
/// Screens reserve strips of themselves for furniture the sprite must not
/// disappear behind: the Dock and the menu bar on macOS, the taskbar on
/// Windows. That reservation cannot be read from the window list — macOS
/// reports the Dock as a window covering the whole display, so its top edge
/// looks like the top of the screen — but every desktop platform already
/// computes it for its own window manager, and reports it as a work area.
///
/// Both rectangles arrive in physical pixels, because that is how a window
/// server measures a screen, and the Engine works in points. `in_points` does
/// the conversion and says which scale factor is the right one.
///
/// A platform that does not report a work area reports an empty one, and gets
/// the whole frame back. That is the correct answer rather than a degraded
/// one: a desktop reserving nothing is a desktop the sprite may cross entirely.
pub fn usable_frame(frame_physical: Rect, work_area_physical: Rect, scale: f64) -> Rect {
    let frame = in_points(frame_physical, scale);
    let work = in_points(work_area_physical, scale);

    // Clamped into the frame edge by edge, rather than refused whole if any
    // edge escapes. Refusing hands back the entire display, which is bug #39
    // returning silently, and it would take almost nothing to trigger: a
    // fractional scale factor — macOS offers several — divides these numbers
    // into values that need not land back exactly on the frame's own edges, and
    // being over by one unit in the last place is enough. Clamping cannot
    // invert the rectangle and cannot put the sprite outside its display, which
    // are the two properties physics needs.
    let left = work.x.max(frame.x);
    let top = work.y.max(frame.y);
    let right = (work.x + work.width).min(frame.x + frame.width);
    let bottom = (work.y + work.height).min(frame.y + frame.height);

    // Nothing left to stand on means no work area at all, or one that misses
    // its own display.
    if right <= left || bottom <= top {
        return frame;
    }

    Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// The geometry half of a `WorldSnapshot`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldGeometry {
    /// The part of each active display a sprite may occupy, in the same
    /// coordinate space as `windows`.
    ///
    /// Usable area rather than the whole frame; see `usable_frame`. A platform
    /// that reserves nothing reports whole frames, which is the same thing said
    /// about an emptier desktop.
    pub usable_frames: Vec<Rect>,
    /// Visible windows in descending z-order: frontmost first.
    pub windows: Vec<WindowRect>,
    /// The Dock's true bounds, when the platform can see them.
    ///
    /// The work area only says the Dock's edge and thickness — a full-width
    /// strip — while the Dock itself does not stretch to the sides of its
    /// display. A platform that can read the real rectangle reports it here,
    /// the Dock becomes a Perch (`snapshot`), and `floor_under_dock` gives the
    /// floor beside it back to the sprite. `None` means unknown, and the
    /// full-width strip stands in, as it always has.
    pub dock: Option<Rect>,
}

/// Whether this rectangle's center lies inside `frame` — which display, if
/// any, a claimed Dock belongs to. The center rather than containment,
/// because a Dock legitimately touches its display's edges.
pub(crate) fn centered_in(dock: &Rect, frame: Rect) -> bool {
    let center = (dock.x + dock.width / 2.0, dock.y + dock.height / 2.0);
    center.0 >= frame.x
        && center.0 <= frame.x + frame.width
        && center.1 >= frame.y
        && center.1 <= frame.y + frame.height
}

/// Whether a rectangle claiming to be the Dock can be believed against one
/// display.
///
/// The claim comes from outside the type system — a private SPI or an
/// Accessibility read — so it is trusted only when it is shaped like a
/// bottom Dock: a thin horizontal strip, standing in the margin the work
/// area reserved (its center below the usable floor, inside the frame). A
/// full-display rectangle, an empty one, a bar floating mid-screen, or any
/// rect on a display whose work area reserved nothing all fail, and the
/// caller keeps the full-width strip instead of building a floor from a lie.
pub fn plausible_dock(dock: &Rect, frame: Rect, usable: Rect) -> bool {
    let thin = dock.height > 0.0 && dock.height <= frame.height * 0.3;
    let horizontal = dock.width > dock.height;
    let in_reserved_margin = dock.y + dock.height / 2.0 > usable.y + usable.height;
    thin && horizontal && centered_in(dock, frame) && in_reserved_margin
}

/// The usable frame of the display that holds the Dock, once the Dock's true
/// bounds are known.
///
/// The work area reserves a full-width strip because that is all the window
/// manager will say. With the Dock's real rectangle in hand the reservation is
/// the Dock itself — a Perch — and the floor drops to the display's own bottom
/// edge, so a sprite walking off the Dock's end falls instead of standing on
/// air. A display the Dock is not on keeps its work area unchanged, and so do
/// the menu bar's strip and the side edges.
pub fn floor_under_dock(usable: Rect, frame: Rect, dock: &Rect) -> Rect {
    if !centered_in(dock, frame) {
        return usable;
    }

    Rect {
        x: usable.x,
        y: usable.y,
        width: usable.width,
        height: (frame.y + frame.height) - usable.y,
    }
}

/// A platform's view of the desktop.
pub trait WindowSource {
    fn capabilities(&self) -> Capabilities;

    /// Read whatever the platform will report right now. An implementation
    /// reports what it can see; honouring the declared capabilities is
    /// `snapshot`'s job, not each platform's.
    fn read(&self) -> WorldGeometry;

    /// The seam the Engine reads from.
    ///
    /// Window rectangles are dropped unless the platform declares
    /// `window_geometry`, so a degraded platform yields display frames and
    /// nothing else. Without this, a platform that can see some windows some of
    /// the time would feed the physics half a desktop, and the sprite would
    /// perch on Perches that are not there.
    fn snapshot(&self) -> WorldGeometry {
        let mut geometry = self.read();
        if !self.capabilities().window_geometry {
            geometry.windows.clear();
        }
        geometry
    }
}

/// Every other platform for now. Windows is stubbed deliberately: `docs/SPEC.md`
/// puts it out of scope for v1, and the interface exists so that the Spatial
/// Layer meets a platform that declares nothing and degrades, rather than a
/// platform that is missing.
#[cfg(not(target_os = "macos"))]
pub struct StubWindowSource;

#[cfg(not(target_os = "macos"))]
impl WindowSource for StubWindowSource {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn read(&self) -> WorldGeometry {
        WorldGeometry::default()
    }
}

/// Hand-written fake for Engine tests: it reports exactly what it was built
/// with, so a test can describe a desktop as a literal.
#[cfg(test)]
pub struct FakeWindowSource {
    pub capabilities: Capabilities,
    pub geometry: WorldGeometry,
}

#[cfg(test)]
impl WindowSource for FakeWindowSource {
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn read(&self) -> WorldGeometry {
        self.geometry.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// A 1x display beside a 2x one. Literal values, so this runs the same
    /// anywhere, and they are what a window server reports for that pair.
    ///
    /// A window server gives each display's geometry against that display's own
    /// scale, so the 2x display's origin arrives already doubled. Converting
    /// both with one factor puts the overlay on a display that is not there.
    #[test]
    fn each_display_converts_with_its_own_scale() {
        assert_eq!(
            in_points(rect(0.0, 0.0, 1920.0, 1080.0), 1.0),
            rect(0.0, 0.0, 1920.0, 1080.0)
        );
        assert_eq!(
            in_points(rect(3840.0, 0.0, 3456.0, 2234.0), 2.0),
            rect(1920.0, 0.0, 1728.0, 1117.0),
            "the 2x display sits at 1920 in points, not at 3840"
        );
    }

    /// Literal values read off a running app while #39 was being written: a
    /// 1920x1080 display at scale 1 with a 30-point menu bar and a 98-point
    /// Dock.
    #[test]
    fn a_reserved_strip_is_taken_off_the_frame_the_sprite_may_occupy() {
        let usable = usable_frame(
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(0.0, 30.0, 1920.0, 952.0),
            1.0,
        );

        assert_eq!(usable, rect(0.0, 30.0, 1920.0, 952.0));
        assert_eq!(
            usable.y + usable.height,
            982.0,
            "the Dock's top edge, which is where the sprite comes to rest"
        );
    }

    /// The Retina display beside it, so every number it reports is twice the
    /// points the Engine works in.
    #[test]
    fn a_displays_own_scale_is_what_converts_it() {
        let usable = usable_frame(
            rect(3840.0, 0.0, 3456.0, 2234.0),
            rect(3840.0, 66.0, 3456.0, 2168.0),
            2.0,
        );

        assert_eq!(usable, rect(1920.0, 33.0, 1728.0, 1084.0));
        assert_eq!(
            usable.y + usable.height,
            1117.0,
            "no Dock on this one, so the floor is the bottom of the display"
        );
    }

    /// A Dock on the left or the right is a setting, not an edge case. The
    /// sprite should meet its inner edge as a wall.
    #[test]
    fn a_reserved_strip_at_the_side_narrows_the_frame() {
        let usable = usable_frame(
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(80.0, 30.0, 1840.0, 1050.0),
            1.0,
        );

        assert_eq!(usable.x, 80.0, "the sprite cannot walk left of the Dock");
        assert_eq!(usable.x + usable.width, 1920.0, "and the right edge stands");
    }

    #[test]
    fn a_platform_that_reserves_nothing_yields_the_whole_frame() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(usable_frame(frame, frame, 1.0), frame);
    }

    /// A platform with no work area of its own — the stub, or a compositor that
    /// will not say — must leave the sprite a display to stand on.
    #[test]
    fn an_unreported_work_area_yields_the_whole_frame() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(usable_frame(frame, rect(0.0, 0.0, 0.0, 0.0), 1.0), frame);
    }

    /// A work area larger than its display, or hanging off it, is a platform
    /// contradicting itself. Clamped into the display rather than refused: the
    /// sprite must never be handed a rectangle it can leave, and refusing would
    /// quietly give back the whole display, which is the bug this exists to fix.
    #[test]
    fn a_work_area_that_escapes_its_display_is_clamped_into_it() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);

        assert_eq!(
            usable_frame(frame, rect(0.0, 0.0, 4000.0, 4000.0), 1.0),
            frame,
            "a work area swallowing the display reserves nothing"
        );
        assert_eq!(
            usable_frame(frame, rect(-100.0, 30.0, 1920.0, 952.0), 1.0),
            rect(0.0, 30.0, 1820.0, 952.0),
            "the part off the left is cut, and the Dock inset survives"
        );
        assert_eq!(
            usable_frame(frame, rect(0.0, 30.0, 1920.0, 1080.0), 1.0),
            rect(0.0, 30.0, 1920.0, 1050.0),
            "the part off the bottom is cut, and the menu bar inset survives"
        );
    }

    /// The reason clamping beats refusing. A fractional scale divides these
    /// numbers into values that need not land back on the frame's own edges,
    /// and a rectangle over by one unit in the last place must not cost the
    /// sprite its Dock inset.
    #[test]
    fn a_fractional_scale_does_not_give_the_whole_display_back() {
        // 1.5x, which macOS offers: a 2880x1620 panel drawn as 1920x1080 points
        // with the same 30-point menu bar and 98-point Dock.
        let usable = usable_frame(
            rect(0.0, 0.0, 2880.0, 1620.0),
            rect(0.0, 45.0, 2880.0, 1428.0),
            1.5,
        );

        assert_eq!(
            usable.y + usable.height,
            982.0,
            "still the Dock's top edge, not the display's bottom at 1080"
        );
        assert!(
            usable.height < 1080.0,
            "the reservation was not thrown away"
        );
    }

    /// A scale of zero would divide every coordinate into infinity.
    #[test]
    fn a_nonsensical_scale_is_treated_as_one() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let usable = usable_frame(frame, rect(0.0, 30.0, 1920.0, 952.0), 0.0);
        assert_eq!(usable, rect(0.0, 30.0, 1920.0, 952.0));
    }

    fn window(id: WindowId, owner: &str, bounds: Rect) -> WindowRect {
        WindowRect {
            id,
            bounds,
            owner: owner.to_string(),
            layer: 0,
        }
    }

    /// Literal values read off a running macOS 26 desktop: a 1920x1080 display,
    /// a 30-point menu bar, the Dock's strip from 982 down, and the Dock's real
    /// rectangle — 234 points in from either side — read over Accessibility.
    #[test]
    fn a_known_dock_gives_the_floor_beside_it_back() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let usable = rect(0.0, 30.0, 1920.0, 952.0);
        let dock = rect(234.0, 978.0, 1452.0, 92.0);

        let floor = floor_under_dock(usable, frame, &dock);

        assert_eq!(
            floor.y + floor.height,
            1080.0,
            "the floor drops to the display's own bottom edge"
        );
        assert_eq!(floor.y, 30.0, "the menu bar's strip is not touched");
        assert_eq!((floor.x, floor.width), (0.0, 1920.0), "nor the sides");
    }

    /// The display beside it has no Dock, and must keep its own work area.
    #[test]
    fn a_display_without_the_dock_keeps_its_work_area() {
        let frame = rect(1920.0, 0.0, 1728.0, 1117.0);
        let usable = rect(1920.0, 33.0, 1728.0, 1084.0);
        let dock = rect(234.0, 978.0, 1452.0, 92.0);

        assert_eq!(floor_under_dock(usable, frame, &dock), usable);
    }

    /// A work area with no bottom strip — the Dock hidden, or on another
    /// edge — has nothing to give back.
    #[test]
    fn a_floor_already_at_the_bottom_stays_where_it_is() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let usable = rect(0.0, 30.0, 1920.0, 1050.0);
        let dock = rect(234.0, 978.0, 1452.0, 92.0);

        assert_eq!(floor_under_dock(usable, frame, &dock), usable);
    }

    /// The gate between an unversioned source and the physics. The believed
    /// rectangles are the two shapes measured on a real desktop: the SPI's
    /// reserved strip and the Accessibility island.
    #[test]
    fn a_dock_claim_is_believed_only_when_it_is_shaped_like_one() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let usable = rect(0.0, 30.0, 1920.0, 952.0);

        for measured in [
            rect(233.0, 982.0, 1453.0, 98.0),
            rect(234.0, 978.0, 1452.0, 92.0),
        ] {
            assert!(plausible_dock(&measured, frame, usable), "{measured:?}");
        }

        for (why, lie) in [
            ("the full display", frame),
            ("an empty rect", rect(234.0, 978.0, 0.0, 0.0)),
            (
                "a bar floating mid-screen",
                rect(234.0, 500.0, 1452.0, 92.0),
            ),
            ("a side Dock's shape", rect(0.0, 100.0, 92.0, 900.0)),
            (
                "a rect off every display",
                rect(5000.0, 978.0, 1452.0, 92.0),
            ),
        ] {
            assert!(!plausible_dock(&lie, frame, usable), "{why}");
        }

        // A display whose work area reserved nothing has no margin for a
        // Dock to stand in, so no claim on it is believable.
        let flush = rect(0.0, 30.0, 1920.0, 1050.0);
        assert!(!plausible_dock(
            &rect(233.0, 982.0, 1453.0, 98.0),
            frame,
            flush
        ));
    }

    /// The Wayland case: displays are known, windows are not.
    #[test]
    fn a_platform_without_window_geometry_yields_usable_frames_and_no_windows() {
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: false,
                absolute_positioning: false,
            },
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![window(1, "Terminal", rect(10.0, 20.0, 800.0, 600.0))],
                dock: None,
            },
        };

        let snapshot = source.snapshot();

        assert_eq!(snapshot.usable_frames, vec![rect(0.0, 0.0, 1920.0, 1080.0)]);
        assert!(
            snapshot.windows.is_empty(),
            "no window_geometry capability means no rectangles: {:?}",
            snapshot.windows
        );
    }

    /// The macOS case: the guard above must not swallow the windows of a
    /// platform that can see them, and must not reshuffle them — the Engine
    /// reads z-order out of the order alone.
    #[test]
    fn a_platform_with_window_geometry_yields_its_windows_frontmost_first() {
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![
                    window(1, "Terminal", rect(10.0, 20.0, 800.0, 600.0)),
                    window(2, "Finder", rect(30.0, 40.0, 500.0, 400.0)),
                ],
                dock: None,
            },
        };

        let owners: Vec<_> = source
            .snapshot()
            .windows
            .iter()
            .map(|w| w.owner.clone())
            .collect();

        assert_eq!(owners, vec!["Terminal", "Finder"]);
    }
}
