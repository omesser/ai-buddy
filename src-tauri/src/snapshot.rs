//! Assembling the `WorldSnapshot` the Engine ticks on.
//!
//! The Engine is pure and cannot drive itself, and `WindowSource` deliberately
//! produces only the geometry half of a snapshot. Something has to read the
//! platform, carry its readings into the Engine's terms and hand them over once
//! per tick. That is this module.
//!
//! It is separate from the loop in `main.rs` because everything here is
//! testable against a fake desktop: the loop owns the wall clock, the renderer
//! and the window server, and this owns the two cadences and the conversion.

use std::time::Duration;

use crate::engine::{Point, Rect, WorldSnapshot};
use crate::window_source::{WindowSource, WorldGeometry, POLL_INTERVAL};

/// Reads the platform at its own cadence and assembles a snapshot per tick.
///
/// The two cadences are the point of this type. Enumerating every window on the
/// desktop ten times a second is cheap; sixty times a second is not, and the
/// windows have not moved in between. So geometry is read at `POLL_INTERVAL`
/// and reused on the ticks between reads, which is what lets the Engine tick as
/// fast as the renderer wants.
pub struct SnapshotAssembler<S> {
    source: S,
    /// The last geometry read, handed to the Engine again on every tick until
    /// the next read replaces it.
    geometry: WorldGeometry,
    since_poll: Duration,
}

impl<S: WindowSource> SnapshotAssembler<S> {
    /// Starts already due for a read, so the first snapshot ever assembled
    /// describes a real desktop rather than an empty one, which is a world with
    /// no floor and nothing to land on.
    pub fn new(source: S) -> Self {
        Self {
            source,
            geometry: WorldGeometry::default(),
            since_poll: POLL_INTERVAL,
        }
    }

    /// One tick's snapshot: `elapsed_ms` since the previous tick, and the
    /// cursor in the Engine's coordinate space.
    pub fn assemble(&mut self, elapsed_ms: u32, cursor: Point) -> WorldSnapshot {
        self.since_poll += Duration::from_millis(u64::from(elapsed_ms));
        if self.since_poll >= POLL_INTERVAL {
            self.since_poll = Duration::ZERO;
            self.geometry = self.source.snapshot();
        }

        world_snapshot(&self.geometry, cursor, elapsed_ms)
    }
}

/// The Engine's view of one moment, from what the platform reported.
///
/// Both sides already speak points with y growing downward across every
/// display, so this changes type without changing space. Window rectangles keep
/// the order they arrived in: descending z-order is carried by the order alone.
fn world_snapshot(geometry: &WorldGeometry, cursor: Point, elapsed_ms: u32) -> WorldSnapshot {
    WorldSnapshot {
        displays: geometry.display_frames.iter().copied().map(rect).collect(),
        windows: geometry.windows.iter().map(|w| rect(w.bounds)).collect(),
        cursor,
        elapsed_ms,
        // Verbs arrive with Grab (#6) and proposals with the Director (#12).
        // Nothing produces either yet, so an empty snapshot is not a stub: it
        // is the truth about a desktop nobody has touched the sprite on.
        ..WorldSnapshot::default()
    }
}

fn rect(rect: crate::window_source::Rect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// Where the sprite comes into the world: the middle of the first display the
/// platform reported.
///
/// The middle rather than the top edge, because the top edge is where the menu
/// bar is, and a sprite dropped onto it would come to rest before it had
/// visibly fallen at all. A tuning knob: high enough that the fall reads as a
/// fall, low enough that the art is on screen for the whole of it.
pub fn starting_position(geometry: &WorldGeometry) -> Point {
    geometry
        .display_frames
        .first()
        .map_or(Point::default(), |display| Point {
            x: display.x + display.width / 2.0,
            y: display.y + display.height / 2.0,
        })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::engine::{Engine, State};
    use crate::window_source::{Capabilities, FakeWindowSource, WindowRect};

    fn rect(x: f64, y: f64, width: f64, height: f64) -> crate::window_source::Rect {
        crate::window_source::Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn window(owner: &str, bounds: crate::window_source::Rect) -> WindowRect {
        WindowRect {
            bounds,
            owner: owner.to_string(),
            layer: 0,
        }
    }

    fn seeing_everything() -> Capabilities {
        Capabilities {
            window_geometry: true,
            absolute_positioning: true,
        }
    }

    /// A desktop that changes between reads: each read hands back the next
    /// geometry in the list, and the last one repeats for ever. A test can then
    /// tell one read from the next by what came back, without watching the
    /// fake.
    struct ChangingDesktop(RefCell<Vec<WorldGeometry>>);

    impl ChangingDesktop {
        fn of_display_widths(widths: &[f64]) -> Self {
            Self(RefCell::new(
                widths
                    .iter()
                    .map(|&width| WorldGeometry {
                        display_frames: vec![rect(0.0, 0.0, width, 800.0)],
                        windows: Vec::new(),
                    })
                    .collect(),
            ))
        }
    }

    impl WindowSource for ChangingDesktop {
        fn capabilities(&self) -> Capabilities {
            seeing_everything()
        }

        fn read(&self) -> WorldGeometry {
            let mut remaining = self.0.borrow_mut();
            let geometry = remaining.first().cloned().unwrap_or_default();
            if remaining.len() > 1 {
                remaining.remove(0);
            }
            geometry
        }
    }

    #[test]
    fn a_snapshot_carries_the_platforms_displays_windows_and_cursor_to_the_engine() {
        let source = FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: WorldGeometry {
                display_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![
                    window("Terminal", rect(10.0, 20.0, 800.0, 600.0)),
                    window("Finder", rect(30.0, 40.0, 500.0, 400.0)),
                ],
            },
        };

        let snapshot = SnapshotAssembler::new(source).assemble(16, Point { x: 7.0, y: 9.0 });

        assert_eq!(
            snapshot.displays,
            vec![Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0
            }]
        );
        assert_eq!(
            snapshot.windows,
            vec![
                Rect {
                    x: 10.0,
                    y: 20.0,
                    width: 800.0,
                    height: 600.0
                },
                Rect {
                    x: 30.0,
                    y: 40.0,
                    width: 500.0,
                    height: 400.0
                },
            ],
            "frontmost first, in the order the platform reported"
        );
        assert_eq!(snapshot.cursor, Point { x: 7.0, y: 9.0 });
        assert_eq!(snapshot.elapsed_ms, 16);
    }

    #[test]
    fn the_platform_is_read_at_the_poll_interval_and_reused_on_the_ticks_between() {
        // Three desktops, told apart by the width of their one display.
        let mut assembler = SnapshotAssembler::new(ChangingDesktop::of_display_widths(&[
            1000.0, 2000.0, 3000.0,
        ]));

        // 20ms ticks against a 100ms poll interval: five ticks per read.
        let widths: Vec<f64> = (0..15)
            .map(|_| assembler.assemble(20, Point::default()).displays[0].width)
            .collect();

        assert_eq!(
            widths,
            vec![
                1000.0, 1000.0, 1000.0, 1000.0, 1000.0, 2000.0, 2000.0, 2000.0, 2000.0, 2000.0,
                3000.0, 3000.0, 3000.0, 3000.0, 3000.0
            ],
            "the Engine ticks faster than the desktop is read"
        );
    }

    /// The circuit this module closes: geometry from the platform becomes a
    /// world the sprite falls through and lands in.
    #[test]
    fn a_sprite_ticked_from_a_platforms_geometry_lands_on_that_desktops_window() {
        let desktop = WorldGeometry {
            display_frames: vec![rect(0.0, 0.0, 1000.0, 800.0)],
            // Spans the middle of the display, so the sprite starts above it.
            windows: vec![window("Terminal", rect(400.0, 500.0, 300.0, 200.0))],
        };
        let start = starting_position(&desktop);
        assert_eq!(
            start,
            Point { x: 500.0, y: 400.0 },
            "the middle of the display it was told about"
        );

        let mut engine = Engine::new(start);
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: desktop,
        });

        let landed = (0..50)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default())))
            .last()
            .expect("fifty ticks produce fifty frames");

        assert_eq!(landed.state, State::Perched);
        assert_eq!(landed.position.y, 500.0, "the window's top edge");
    }
}
