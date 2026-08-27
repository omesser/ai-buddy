//! Where the windows are.
//!
//! The Spatial Layer needs window geometry before the user has granted
//! anything, so this layer is deliberately built out of what the operating
//! system hands over for free. Everything a permission prompt would buy —
//! window titles above all — is absent by construction rather than by policy.

use std::time::Duration;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacosWindowSource;

/// How often the Shell should read the source: approximately 10Hz.
///
/// Windows move at human speed, and the Engine interpolates between reads, so
/// smoothness is the renderer's problem rather than a reason to poll faster.
/// Ten times a second is what keeps a full window-list enumeration off the
/// frame loop and out of the battery budget.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// What a platform can actually tell us about the desktop.
///
/// Declared, never assumed. Under Wayland both of these are unavailable and the
/// Spatial Layer degrades to screen-edge physics, which is a supported mode
/// rather than an error state.
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

/// One visible window: where it is, who owns it, and how high it stacks.
///
/// No title. Titles need Screen Recording consent, and v1 asks for nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowRect {
    pub bounds: Rect,
    /// The owning application's name, as the window server reports it.
    pub owner: String,
    /// The window server's level: 0 for ordinary application windows, higher
    /// for menus, docks and other overlays. The Engine decides what is a Perch.
    pub layer: i32,
}

/// The geometry half of a `WorldSnapshot`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldGeometry {
    /// Every active display's frame, in the same coordinate space as `windows`.
    pub display_frames: Vec<Rect>,
    /// Visible windows in descending z-order: frontmost first.
    pub windows: Vec<WindowRect>,
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

    fn window(owner: &str, bounds: Rect) -> WindowRect {
        WindowRect {
            bounds,
            owner: owner.to_string(),
            layer: 0,
        }
    }

    /// The Wayland case: displays are known, windows are not.
    #[test]
    fn a_platform_without_window_geometry_yields_display_frames_and_no_windows() {
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: false,
                absolute_positioning: false,
            },
            geometry: WorldGeometry {
                display_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![window("Terminal", rect(10.0, 20.0, 800.0, 600.0))],
            },
        };

        let snapshot = source.snapshot();

        assert_eq!(
            snapshot.display_frames,
            vec![rect(0.0, 0.0, 1920.0, 1080.0)]
        );
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
                display_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![
                    window("Terminal", rect(10.0, 20.0, 800.0, 600.0)),
                    window("Finder", rect(30.0, 40.0, 500.0, 400.0)),
                ],
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
