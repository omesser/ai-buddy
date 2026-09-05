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

use crate::engine::{Point, Verb, Window, WorldSnapshot};
use crate::window_source::{
    Rect,
    WindowSource, WorldGeometry, DOCK_PERCH_ID, POLL_INTERVAL, RIDE_POLL_INTERVAL,
};

/// The longest step of the world the Engine is ever told about, whatever the
/// wall clock says.
///
/// A slept machine, a suspended process or a stalled read hands the loop
/// minutes of elapsed time at once, and the Engine integrates whatever it is
/// given: gravity over five minutes is a velocity no window can catch and the
/// sprite is flung off the desktop. One poll interval is the cap because that
/// is how old the geometry may already be — the world in a snapshot is only
/// accurate to a poll, so integrating a longer step against it is fiction. The
/// sprite resumes from where it was rather than from where the missing minutes
/// would have thrown it.
const MAX_ELAPSED_MS: u32 = POLL_INTERVAL.as_millis() as u32;

/// Reads the platform at its own cadence and assembles a snapshot per tick.
///
/// The two cadences are the point of this type. Geometry is read at the idle
/// poll, or at the ride poll when the sprite is holding a moving Perch, and
/// reused on any tick that arrives sooner. #98.
pub struct SnapshotAssembler<S> {
    source: S,
    /// The last geometry read, reused until the next read replaces it.
    geometry: WorldGeometry,
    since_poll: Duration,
    /// How many times the window list has actually been read. Carried on the
    /// snapshot so the Engine can tell a reused rectangle from a new sample
    /// and coast a ride between polls. #98.
    poll_generation: u64,
    /// A ride needs the frame rate; sitting and sleeping do not. The Shell
    /// flips this from the last Frame. #98.
    fast: bool,
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
            poll_generation: 0,
            fast: false,
        }
    }

    /// Read at the ride cadence for as long as the sprite is holding on.
    /// Idle is the default so a sleeping buddy does not enumerate the
    /// desktop sixty times a second. #98.
    pub fn poll_fast(&mut self, ride: bool) {
        if ride && !self.fast {
            // One immediate read, not a burst of whatever the idle clock
            // had left — that remainder is many ride intervals at once.
            self.since_poll = RIDE_POLL_INTERVAL;
        }
        self.fast = ride;
    }

    fn interval(&self) -> Duration {
        if self.fast {
            RIDE_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        }
    }

    /// One tick's snapshot: `elapsed_ms` since the previous tick, and the
    /// cursor in the Engine's coordinate space.
    pub fn assemble(&mut self, elapsed_ms: u32, cursor: Point, verbs: Vec<Verb>) -> WorldSnapshot {
        let elapsed_ms = elapsed_ms.min(MAX_ELAPSED_MS);
        let interval = self.interval();

        // The due check comes before the tick's own time is added, and a read
        // takes one interval off the clock rather than zeroing it. Zeroing
        // throws away the overshoot, which on a faster tick than the poll
        // would make every read late by that remainder.
        if self.since_poll >= interval {
            self.since_poll = self.since_poll.saturating_sub(interval);
            self.geometry = self.source.snapshot();
            self.poll_generation = self.poll_generation.saturating_add(1);
        }
        self.since_poll += Duration::from_millis(u64::from(elapsed_ms));

        world_snapshot(
            &self.geometry,
            cursor,
            elapsed_ms,
            verbs,
            self.poll_generation,
        )
    }

    /// What the sprite's feet are on, for the Director. Titles need Screen
    /// Recording; the owner name is what the window server gives for free.
    pub fn standing_on(&self, feet: Point) -> String {
        describe_standing(feet, &self.geometry)
    }
}

/// Name the surface under `feet`. A Perch is a window (owner, not title);
/// the usable floor is the Dock's top — or, when the Dock's true bounds are
/// known, the Dock is its own surface and the floor runs beside it; a
/// display's left or right is an edge.
pub fn describe_standing(feet: Point, geometry: &WorldGeometry) -> String {
    if let Some(dock) = &geometry.dock {
        if feet.y == dock.y && feet.x >= dock.x && feet.x <= dock.x + dock.width {
            return "the top of the Dock".to_string();
        }
    }
    for window in geometry
        .windows
        .iter()
        .filter(|window| perch_eligible(window.layer))
    {
        let bounds = &window.bounds;
        if feet.y == bounds.y && feet.x >= bounds.x && feet.x <= bounds.x + bounds.width {
            let owner = window.owner.trim();
            return if owner.is_empty() {
                "a window".to_string()
            } else {
                format!("a {owner} window")
            };
        }
    }
    for display in &geometry.usable_frames {
        let right = display.x + display.width;
        let bottom = display.y + display.height;
        if feet.y == bottom && feet.x >= display.x && feet.x <= right {
            // With the Dock's bounds known the floor reaches the display's
            // own bottom edge, so "above the Dock" would name the wrong
            // side — and only on the display that actually holds the Dock.
            return match &geometry.dock {
                Some(dock) if crate::window_source::centered_in(dock, *display) => {
                    "the display floor, beside the Dock".to_string()
                }
                Some(_) => "the display floor".to_string(),
                None => "the display floor, above the Dock".to_string(),
            };
        }
        if feet.y >= display.y && feet.y <= bottom && (feet.x == display.x || feet.x == right) {
            return "the screen edge".to_string();
        }
    }
    "nothing".to_string()
}

/// The Engine's view of one moment, from what the platform reported.
///
/// Both sides already speak points with y growing downward across every
/// display, so this changes type without changing space. Window rectangles keep
/// the order they arrived in: descending z-order is carried by the order alone.
///
/// Only Perch-eligible windows are carried over. Window level is a platform
/// concept — a macOS `kCGWindowLayer`, meaningless to a pure Engine — so which
/// levels count as somewhere to stand is decided here, where the platform is
/// still in view, and the Engine is handed a world in which every rectangle is
/// a Perch.
fn world_snapshot(
    geometry: &WorldGeometry,
    cursor: Point,
    elapsed_ms: u32,
    verbs: Vec<Verb>,
    poll_generation: u64,
) -> WorldSnapshot {
    let mut windows: Vec<Window> = geometry
        .windows
        .iter()
        .filter(|w| perch_eligible(w.layer))
        .map(|w| Window {
            id: w.id,
            rect: w.bounds,
        })
        .collect();
    if let Some(dock) = &geometry.dock {
        // Frontmost, because the Dock draws above every application window:
        // a fall over the Dock lands on the Dock, never on a window hiding
        // underneath it.
        windows.insert(
            0,
            Window {
                id: DOCK_PERCH_ID,
                rect: *dock,
            },
        );
    }
    WorldSnapshot {
        displays: geometry.usable_frames.clone(),
        windows,
        cursor,
        elapsed_ms,
        verbs,
        poll_generation,
        // The frame loop writes `proposal` after assemble(). None means no
        // proposal this tick.
        ..WorldSnapshot::default()
    }
}

/// Whether a window at this level is somewhere the sprite may stand.
///
/// Only the ordinary application level is. Everything above it is the desktop's
/// furniture — the menu bar at 24, the status items at 25, the Dock at 20, and
/// on macOS the Dock reports a rectangle covering the whole display — and a
/// sprite that treats furniture as a Perch lands on the menu bar the moment it
/// is let go and never falls again. Everything below it is the desktop picture
/// and its notifications, which are behind the sprite rather than under it.
/// The one piece of furniture worth standing on — the Dock — arrives with its
/// true bounds as `WorldGeometry::dock` when the platform can see them, and
/// becomes a Perch above, not through this filter.
fn perch_eligible(layer: i32) -> bool {
    layer == 0
}


/// Where the sprite comes into the world: the middle of the first display the
/// platform reported.
///
/// A tuning knob, and the middle rather than the top edge because the art hangs
/// above the sprite's feet: dropped from the very top it would fall for its own
/// height before any of it was on screen. Half a display is far enough that the
/// fall reads as a fall and near enough that all of it is watched.
pub fn starting_position(geometry: &WorldGeometry) -> Point {
    geometry
        .usable_frames
        .first()
        .map_or(Point::default(), |display| Point {
            x: display.x + display.width / 2.0,
            y: display.y + display.height / 2.0,
        })
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use crate::engine::{Engine, State, Window};
    use crate::window_source::{Capabilities, FakeWindowSource, WindowId, WindowRect};

    fn rect(x: f64, y: f64, width: f64, height: f64) -> crate::window_source::Rect {
        crate::window_source::Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn window(id: WindowId, owner: &str, bounds: crate::window_source::Rect) -> WindowRect {
        WindowRect {
            id,
            bounds,
            owner: owner.to_string(),
            layer: 0,
        }
    }

    /// A window above the ordinary application level. The layers here are the
    /// ones a real macOS desktop reports: 20 for the Dock, 24 for the menu bar,
    /// 25 for the status items, and a large negative one for Notification
    /// Centre.
    fn elevated(
        id: WindowId,
        owner: &str,
        bounds: crate::window_source::Rect,
        layer: i32,
    ) -> WindowRect {
        WindowRect {
            id,
            bounds,
            owner: owner.to_string(),
            layer,
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
                        usable_frames: vec![rect(0.0, 0.0, width, 800.0)],
                        windows: Vec::new(),
                        dock: None,
                    })
                    .collect(),
            ))
        }
    }

    /// A desktop that reports how often it has been read: its one display is
    /// as wide as the number of reads so far, so a test counts reads by looking
    /// at the snapshot rather than at the fake.
    #[derive(Default)]
    struct CountingDesktop(Cell<f64>);

    impl WindowSource for CountingDesktop {
        fn capabilities(&self) -> Capabilities {
            seeing_everything()
        }

        fn read(&self) -> WorldGeometry {
            self.0.set(self.0.get() + 1.0);
            WorldGeometry {
                usable_frames: vec![rect(0.0, 0.0, self.0.get(), 800.0)],
                windows: Vec::new(),
                dock: None,
            }
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
                usable_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![
                    window(1, "Terminal", rect(10.0, 20.0, 800.0, 600.0)),
                    window(2, "Finder", rect(30.0, 40.0, 500.0, 400.0)),
                ],
                dock: None,
            },
        };

        let snapshot =
            SnapshotAssembler::new(source).assemble(16, Point { x: 7.0, y: 9.0 }, Vec::new());

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
                Window {
                    id: 1,
                    rect: Rect {
                        x: 10.0,
                        y: 20.0,
                        width: 800.0,
                        height: 600.0
                    }
                },
                Window {
                    id: 2,
                    rect: Rect {
                        x: 30.0,
                        y: 40.0,
                        width: 500.0,
                        height: 400.0
                    }
                },
            ],
            "frontmost first, with the window server's own ids, in the order \
             the platform reported"
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

        // 20ms ticks against a 100ms idle poll: five ticks per read.
        let widths: Vec<f64> = (0..15)
            .map(|_| {
                assembler
                    .assemble(20, Point::default(), Vec::new())
                    .displays[0]
                    .width
            })
            .collect();

        assert_eq!(
            widths,
            vec![
                1000.0, 1000.0, 1000.0, 1000.0, 1000.0, 2000.0, 2000.0, 2000.0, 2000.0, 2000.0,
                3000.0, 3000.0, 3000.0, 3000.0, 3000.0
            ],
            "the Engine ticks faster than the desktop is read"
        );

        let mut generations = SnapshotAssembler::new(ChangingDesktop::of_display_widths(&[
            1000.0, 2000.0, 3000.0,
        ]));
        let gens: Vec<u64> = (0..15)
            .map(|_| {
                generations
                    .assemble(20, Point::default(), Vec::new())
                    .poll_generation
            })
            .collect();
        assert_eq!(
            gens,
            vec![1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3],
            "a reused generation is a tick between polls"
        );
    }

    /// #98: a ride is the only time the window list is worth reading at
    /// the frame rate. Sitting and sleeping stay on the idle poll.
    #[test]
    fn a_ride_reads_the_desktop_at_the_frame_rate() {
        let mut assembler = SnapshotAssembler::new(ChangingDesktop::of_display_widths(&[
            1000.0, 2000.0, 3000.0,
        ]));
        assembler.poll_fast(true);

        // 8ms ticks against a 16ms ride poll: two ticks per read.
        let widths: Vec<f64> = (0..6)
            .map(|_| assembler.assemble(8, Point::default(), Vec::new()).displays[0].width)
            .collect();
        assert_eq!(
            widths,
            vec![1000.0, 1000.0, 2000.0, 2000.0, 3000.0, 3000.0],
            "a ride polls as often as the Engine ticks"
        );
    }

    /// The desktop furniture is not somewhere to stand. Layers, bounds and
    /// owners here are copied from what a real macOS desktop reports.
    #[test]
    fn only_ordinary_application_windows_reach_the_engine() {
        let source = FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![
                    elevated(1, "Control Center", rect(1264.0, 0.0, 39.0, 30.0), 25),
                    elevated(2, "Window Server", rect(0.0, 0.0, 1920.0, 30.0), 24),
                    elevated(3, "Dock", rect(0.0, 0.0, 1920.0, 1080.0), 20),
                    window(4, "Terminal", rect(0.0, 30.0, 1920.0, 952.0)),
                    elevated(
                        5,
                        "Notification Center",
                        rect(8.0, 38.0, 180.0, 180.0),
                        -2_147_483_601,
                    ),
                ],
                dock: None,
            },
        };

        let snapshot = SnapshotAssembler::new(source).assemble(16, Point::default(), Vec::new());

        assert_eq!(
            snapshot.windows,
            vec![Window {
                id: 4,
                rect: Rect {
                    x: 0.0,
                    y: 30.0,
                    width: 1920.0,
                    height: 952.0
                }
            }],
            "the menu bar, the Dock and the status items are not Perches"
        );
    }

    /// What the layer filter is for, at the seam that shows it: a sprite let go
    /// at the very top of a display falls the whole way instead of coming to
    /// rest on the menu bar.
    #[test]
    fn a_sprite_dropped_at_the_top_of_a_display_falls_past_the_menu_bar() {
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 0.0, 1920.0, 1080.0)],
                windows: vec![elevated(
                    2,
                    "Window Server",
                    rect(0.0, 0.0, 1920.0, 30.0),
                    24,
                )],
                dock: None,
            },
        });
        let mut engine = Engine::new(Point { x: 960.0, y: 0.0 });

        let landed = (0..100)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("a hundred ticks produce a hundred frames");

        assert_eq!(landed.state, State::Grounded);
        assert_eq!(
            landed.position.y, 1080.0,
            "the bottom of the display, not the menu bar it started on"
        );
    }

    /// #39: the sprite used to come to rest at the bottom of the display, which
    /// is behind the Dock — the Dock draws above the overlay, so three quarters
    /// of the art disappeared under it. Nothing in the window list can say where
    /// the Dock's top edge is, because macOS reports the Dock as a window
    /// covering the whole display. The fix is upstream of the Engine: the
    /// rectangles it is handed are the usable part of each display, so the floor
    /// it already derives is the Dock's top edge.
    #[test]
    fn a_sprite_comes_to_rest_on_the_usable_floor_rather_than_behind_the_dock() {
        // A 1920x1080 display reserving 30 points for the menu bar and 98 for
        // the Dock, read off a running app while #39 was being written.
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 30.0, 1920.0, 952.0)],
                windows: Vec::new(),
                dock: None,
            },
        });
        let mut engine = Engine::new(Point { x: 960.0, y: 40.0 });

        let landed = (0..100)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("a hundred ticks produce a hundred frames");

        assert_eq!(landed.state, State::Grounded);
        assert_eq!(
            landed.position.y, 982.0,
            "the Dock's top edge, not the display's bottom edge at 1080"
        );
    }

    /// The world the Shell hands over once the Dock's true bounds are known
    /// (Accessibility already granted): the floor runs to the display's own
    /// bottom edge and the Dock is a Perch. Values read off the desktop the
    /// bug was reported on — a 1920x1080 display, the Dock's island 234
    /// points in from either side.
    fn dock_aware_desktop() -> WorldGeometry {
        WorldGeometry {
            usable_frames: vec![rect(0.0, 30.0, 1920.0, 1050.0)],
            windows: Vec::new(),
            dock: Some(rect(234.0, 978.0, 1452.0, 92.0)),
        }
    }

    /// Over the Dock, the Dock is still what the sprite rests on — #39's
    /// guarantee, now carried by a Perch instead of a full-width floor.
    #[test]
    fn a_sprite_over_the_dock_rests_on_the_dock() {
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: dock_aware_desktop(),
        });
        let mut engine = Engine::new(Point { x: 960.0, y: 40.0 });

        let landed = (0..100)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("a hundred ticks produce a hundred frames");

        assert_eq!(landed.state, State::Perched, "the Dock is a Perch now");
        assert_eq!(
            landed.position.y, 978.0,
            "the Dock's own top edge, where the island actually is"
        );
    }

    /// The bug this world model fixes: the Dock does not stretch to the sides
    /// of the display, and a sprite beyond its real end used to stand on the
    /// full-width strip — walking on air. Beside the Dock the floor is the
    /// display's own bottom edge.
    #[test]
    fn a_sprite_beside_the_dock_falls_to_the_display_bottom() {
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: dock_aware_desktop(),
        });
        let mut engine = Engine::new(Point { x: 100.0, y: 40.0 });

        let landed = (0..100)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("a hundred ticks produce a hundred frames");

        assert_eq!(landed.state, State::Grounded);
        assert_eq!(
            landed.position.y, 1080.0,
            "the display's bottom edge, not the Dock-top strip hanging in air"
        );
    }

    /// The Director hears the difference too — and only on the display that
    /// actually holds the Dock. A second display's floor is nowhere near it.
    #[test]
    fn standing_names_the_dock_and_the_floor_beside_it() {
        let mut desktop = dock_aware_desktop();
        desktop
            .usable_frames
            .push(rect(1920.0, 33.0, 1728.0, 1084.0));

        assert_eq!(
            describe_standing(Point { x: 960.0, y: 978.0 }, &desktop),
            "the top of the Dock"
        );
        assert_eq!(
            describe_standing(
                Point {
                    x: 100.0,
                    y: 1080.0
                },
                &desktop
            ),
            "the display floor, beside the Dock"
        );
        assert_eq!(
            describe_standing(
                Point {
                    x: 2500.0,
                    y: 1117.0
                },
                &desktop
            ),
            "the display floor"
        );
    }

    /// #39 asked for this to be asserted rather than assumed: a Dock that hides
    /// gives its strip back, and the sprite resting on it is standing on
    /// nothing. Nothing new was needed to make it work — resting is only ever
    /// resting on something, and the Engine re-derives that every tick — but a
    /// sprite left hanging in the air is the failure nobody would notice until
    /// they saw it.
    #[test]
    fn a_reservation_that_disappears_drops_the_sprite_that_was_resting_on_it() {
        let dock = || WorldGeometry {
            usable_frames: vec![rect(0.0, 30.0, 1920.0, 952.0)],
            windows: Vec::new(),
            dock: None,
        };
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: dock(),
        });
        let mut engine = Engine::new(Point { x: 960.0, y: 40.0 });

        let resting = (0..100)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("a hundred ticks produce a hundred frames");
        assert_eq!(resting.position.y, 982.0, "on the Dock");
        assert_eq!(resting.state, State::Grounded);

        // The Dock hides, so the display is usable to its bottom edge.
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 30.0, 1920.0, 1050.0)],
                windows: Vec::new(),
                dock: None,
            },
        });

        let first = engine.tick(&assembler.assemble(20, Point::default(), Vec::new()));
        assert_eq!(
            first.state,
            State::Falling,
            "the strip it was standing on is gone, so it is in the air"
        );

        let landed = (0..100)
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("a hundred ticks produce a hundred frames");
        assert_eq!(
            landed.position.y, 1080.0,
            "and falls the rest of the way to the bottom of the display"
        );
    }

    /// A slept machine or a suspended process hands the loop minutes of wall
    /// clock at once.
    #[test]
    fn a_tick_after_a_long_gap_cannot_teleport_the_sprite() {
        let mut assembler = SnapshotAssembler::new(FakeWindowSource {
            capabilities: seeing_everything(),
            geometry: WorldGeometry {
                usable_frames: vec![rect(0.0, 0.0, 1000.0, 800.0)],
                windows: Vec::new(),
                dock: None,
            },
        });
        let mut engine = Engine::new(Point { x: 500.0, y: 400.0 });

        let snapshot = assembler.assemble(300_000, Point::default(), Vec::new());
        assert_eq!(
            snapshot.elapsed_ms, 100,
            "no more than one poll interval of world is ever integrated at once"
        );

        let frame = engine.tick(&snapshot);

        assert_eq!(frame.state, State::Falling, "still in the air");
        assert!(
            frame.position.y < 500.0,
            "five minutes of gravity in one tick would put it on the floor at \
             800; it is at {}",
            frame.position.y
        );
    }

    /// The cadence itself: the time a read consumes has to leave the remainder
    /// behind, or every read is late by whatever the tick overshot by and the
    /// desktop is read more slowly than `POLL_INTERVAL` says.
    #[test]
    fn the_platform_is_read_once_per_poll_interval_however_the_ticks_divide_it() {
        let mut assembler = SnapshotAssembler::new(CountingDesktop::default());

        // 16ms ticks divide 100ms unevenly, which is the case that drifts.
        let reads = (0..400)
            .map(|_| {
                assembler
                    .assemble(16, Point::default(), Vec::new())
                    .displays[0]
                    .width
            })
            .last()
            .expect("four hundred ticks produce four hundred snapshots");

        assert_eq!(
            reads, 64.0,
            "6400ms of ticks is sixty-four 100ms poll intervals"
        );
    }

    /// The platform reports the primary display first, and that is the one the
    /// sprite comes into the world on.
    #[test]
    fn a_sprite_starts_on_the_first_display_the_platform_reported() {
        let two_displays = WorldGeometry {
            usable_frames: vec![
                rect(0.0, 30.0, 1920.0, 952.0),
                rect(1920.0, 33.0, 1728.0, 1084.0),
            ],
            windows: Vec::new(),
            dock: None,
        };

        assert_eq!(
            starting_position(&two_displays),
            Point { x: 960.0, y: 506.0 },
            "the middle of the first display, not the second"
        );
    }

    /// The circuit this module closes: geometry from the platform becomes a
    /// world the sprite falls through and lands in.
    #[test]
    fn a_sprite_ticked_from_a_platforms_geometry_lands_on_that_desktops_window() {
        let desktop = WorldGeometry {
            usable_frames: vec![rect(0.0, 0.0, 1000.0, 800.0)],
            // Spans the middle of the display, so the sprite starts above it.
            windows: vec![window(1, "Terminal", rect(400.0, 500.0, 300.0, 200.0))],
            dock: None,
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
            .map(|_| engine.tick(&assembler.assemble(20, Point::default(), Vec::new())))
            .last()
            .expect("fifty ticks produce fifty frames");

        assert_eq!(landed.state, State::Perched);
        assert_eq!(landed.position.y, 500.0, "the window's top edge");
    }

    #[test]
    fn standing_names_the_window_owner_not_a_title() {
        let desktop = WorldGeometry {
            usable_frames: vec![rect(0.0, 30.0, 1920.0, 1050.0)],
            windows: vec![window(1, "Cursor", rect(100.0, 200.0, 800.0, 600.0))],
            dock: None,
        };
        assert_eq!(
            describe_standing(Point { x: 140.0, y: 200.0 }, &desktop),
            "a Cursor window"
        );
        assert_eq!(
            describe_standing(
                Point {
                    x: 960.0,
                    y: 1080.0
                },
                &desktop
            ),
            "the display floor, above the Dock"
        );
        assert_eq!(
            describe_standing(Point { x: 0.0, y: 400.0 }, &desktop),
            "the screen edge"
        );
    }

    #[test]
    fn furniture_is_not_named_as_a_perch() {
        let desktop = WorldGeometry {
            usable_frames: vec![rect(0.0, 30.0, 1920.0, 1050.0)],
            windows: vec![elevated(1, "Dock", rect(0.0, 1050.0, 1920.0, 80.0), 20)],
            dock: None,
        };
        assert_eq!(
            describe_standing(
                Point {
                    x: 960.0,
                    y: 1080.0
                },
                &desktop
            ),
            "the display floor, above the Dock"
        );
    }
}
