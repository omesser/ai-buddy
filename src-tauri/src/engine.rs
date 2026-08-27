//! The Engine: `WorldSnapshot` in, `Frame` out, once per tick.
//!
//! Pure and synchronous. It reads no clock, holds no timers and performs no
//! I/O, so time reaches it only as elapsed milliseconds on a snapshot. That is
//! what lets every spatial property be tested by constructing snapshots and
//! asserting frames, with no windowing system, no model and no waiting.

/// A point in the one coordinate space the Engine works in: points, y growing
/// downward, spanning every visible display.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A display frame or a visible window rectangle, in the same space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn bottom(&self) -> f64 {
        self.y + self.height
    }

    fn spans_x(&self, x: f64) -> bool {
        x >= self.x && x <= self.x + self.width
    }
}

/// Where the sprite is anchored and which physics apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Grounded,
    Falling,
    Dragged,
    Perched,
    Climbing,
    Asleep,
}

/// An interaction verb the user performed since the previous tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verb {
    /// Present on every tick the sprite is held, not just on the press.
    Grab,
    /// A Grab released while moving. The Shell measures the cursor's velocity;
    /// the Engine only flies it.
    Throw { velocity: Point },
    /// A click on the sprite.
    Poke,
}

/// What the Director proposed since the previous tick.
///
/// The Engine only speaks the line for now. Playing the named Behavior needs
/// the Primitives and the Character's declarations, which arrive with #8; until
/// then an unknown name costs nothing, because nothing acts on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorProposal {
    pub behavior: String,
    pub dialogue: Option<String>,
}

/// Everything the Engine is told about the world for one tick.
#[derive(Clone, Debug, Default)]
pub struct WorldSnapshot {
    /// Visible display frames.
    pub displays: Vec<Rect>,
    /// Visible window rectangles in descending z-order.
    pub windows: Vec<Rect>,
    pub cursor: Point,
    /// Interaction verbs pending since the previous tick.
    pub verbs: Vec<Verb>,
    /// Milliseconds since the previous tick.
    pub elapsed_ms: u32,
    /// A Behavior proposal delivered since the previous tick, if the Director
    /// made one. Advisory: the Engine is free to refuse it.
    pub proposal: Option<BehaviorProposal>,
}

/// Everything the renderer is told after one tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The sprite's contact point — where its feet are, not its top-left. The
    /// renderer offsets by the art's size, which the Engine does not know.
    pub position: Point,
    pub velocity: Point,
    pub state: State,
    /// The Animation to play, by the name every Character Package must supply.
    pub animation: &'static str,
    /// How far into that Animation the sprite is. The renderer wraps it by the
    /// frame count, which it knows from the manifest and the Engine does not.
    pub frame_index: usize,
    /// A line to speak on this frame only. Dialogue is an event, not a state.
    pub dialogue: Option<String>,
}

/// How long one Animation frame is held.
///
/// ponytail: one cadence for every Animation, roughly 8fps. Per-Animation fps
/// and loop mode arrive on the Character manifest with #7, and replace this.
const FRAME_MS: u32 = 125;

/// How long a resting, untouched sprite waits before it goes to sleep. A tuning
/// knob: long enough not to nod off mid-conversation, short enough that a sprite
/// on an unattended desktop settles down.
const SLEEP_AFTER_MS: u32 = 60_000;

/// Points per second the sprite hauls itself up a screen edge. A tuning knob.
const CLIMB_SPEED: f64 = 200.0;

/// Points per second squared. A tuning knob: the number that makes a fall read
/// as heavy rather than floaty is found by watching it, not by deriving it.
const GRAVITY: f64 = 1800.0;

pub struct Engine {
    position: Point,
    velocity: Point,
    state: State,
    /// Milliseconds the sprite has rested untouched.
    idle_ms: u32,
    animation: &'static str,
    /// Milliseconds since the current Animation started.
    animation_ms: u32,
}

impl Engine {
    /// A sprite placed at `position`, falling until the world says otherwise.
    pub fn new(position: Point) -> Self {
        Self {
            position,
            velocity: Point::default(),
            state: State::Falling,
            idle_ms: 0,
            animation: animation_for(State::Falling),
            animation_ms: 0,
        }
    }

    pub fn tick(&mut self, snapshot: &WorldSnapshot) -> Frame {
        let dt = f64::from(snapshot.elapsed_ms) / 1000.0;

        // Idling is resting untouched. Time spent in the air or in someone's
        // hand does not count towards nodding off.
        if snapshot.verbs.is_empty() {
            self.idle_ms = match self.state {
                State::Grounded | State::Perched | State::Asleep => {
                    self.idle_ms.saturating_add(snapshot.elapsed_ms)
                }
                _ => 0,
            };
        } else {
            self.idle_ms = 0;
            if self.state == State::Asleep {
                // It gets up. Whether it is still standing on anything is
                // settled by falling, the same as any other loss of footing.
                self.state = State::Falling;
            }
        }

        // A Grab wins over whatever the sprite was doing: the user's hand is
        // the one input that outranks the world.
        if snapshot.verbs.contains(&Verb::Grab) {
            self.state = State::Dragged;
            self.position = snapshot.cursor;
            self.velocity = Point::default();
        } else if let Some(velocity) = thrown_velocity(snapshot) {
            self.velocity = velocity;
            self.state = State::Falling;
        } else if self.state == State::Dragged {
            // Let go without velocity: it simply drops.
            self.state = State::Falling;
        }

        match self.state {
            State::Falling => {
                self.velocity.y += GRAVITY * dt;
                self.position.x += self.velocity.x * dt;

                if let Some(wall) = wall_reached(self.position.x, self.velocity.x, snapshot) {
                    // Arriving at a screen edge sideways is a catch, not a stop.
                    // It also keeps the sprite inside the displays.
                    self.position.x = wall;
                    self.velocity = Point::default();
                    self.state = State::Climbing;
                } else {
                    let next_y = self.position.y + self.velocity.y * dt;

                    match support_below(self.position, snapshot) {
                        Some(support) if next_y >= support.y => {
                            self.position.y = support.y;
                            self.velocity = Point::default();
                            self.state = support.state;
                        }
                        _ => self.position.y = next_y,
                    }
                }
            }
            State::Climbing => {
                self.position.y -= CLIMB_SPEED * dt;

                // Off the top of the display there is nothing left to hold, so
                // it lets go. A sprite over no display at all is already
                // holding nothing, hence its own y as the fallback ceiling.
                let ceiling = ceiling_over(self.position.x, snapshot).unwrap_or(self.position.y);
                if self.position.y <= ceiling {
                    self.position.y = ceiling;
                    self.state = State::Falling;
                }
            }
            // Resting is only ever resting on something. When that something
            // moves, closes or resizes, the sprite is in the air again.
            State::Grounded | State::Perched | State::Asleep => {
                if support_below(self.position, snapshot).map(|s| s.y) != Some(self.position.y) {
                    self.state = State::Falling;
                } else if self.idle_ms >= SLEEP_AFTER_MS {
                    self.state = State::Asleep;
                }
            }
            State::Dragged => {}
        }

        let animation = animation_for(self.state);
        if animation == self.animation {
            self.animation_ms = self.animation_ms.saturating_add(snapshot.elapsed_ms);
        } else {
            self.animation = animation;
            self.animation_ms = 0;
        }

        Frame {
            position: self.position,
            velocity: self.velocity,
            state: self.state,
            animation: self.animation,
            frame_index: (self.animation_ms / FRAME_MS) as usize,
            dialogue: snapshot
                .proposal
                .as_ref()
                .and_then(|proposal| proposal.dialogue.clone()),
        }
    }
}

/// Which of the Required Animation Set a State plays.
///
/// A dragged sprite dangles from the cursor, which is what `fall` already
/// draws; the required set has no animation of its own for being held, and
/// eight required animations is already a tax on every Character.
fn animation_for(state: State) -> &'static str {
    match state {
        State::Grounded => "idle",
        State::Falling | State::Dragged => "fall",
        State::Perched => "sit",
        State::Climbing => "walk",
        State::Asleep => "sleep",
    }
}

fn thrown_velocity(snapshot: &WorldSnapshot) -> Option<Point> {
    snapshot.verbs.iter().find_map(|verb| match verb {
        Verb::Throw { velocity } => Some(*velocity),
        _ => None,
    })
}

/// A surface the sprite can come to rest on, and the State that resting on it
/// puts it in.
struct Support {
    y: f64,
    state: State,
}

/// The first surface at or below `position`: the nearest window top edge, or
/// the floor when no window is in the way.
///
/// Window sides and bottoms are not surfaces, so a rectangle only counts when
/// the sprite is above its top edge. That is what lets the sprite rise through
/// a window from underneath instead of being trapped below it.
fn support_below(position: Point, snapshot: &WorldSnapshot) -> Option<Support> {
    let floor = floor_under(position.x, snapshot).map(|y| Support {
        y,
        state: State::Grounded,
    });

    snapshot
        .windows
        .iter()
        .filter(|window| window.spans_x(position.x) && window.y >= position.y)
        .map(|window| Support {
            y: window.y,
            state: State::Perched,
        })
        .chain(floor)
        .min_by(|a, b| a.y.total_cmp(&b.y))
}

/// The bottom of the display the sprite is over, or nothing when it is over no
/// display at all — a sprite outside every display has nothing to land on.
fn floor_under(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    displays_spanning(x, snapshot)
        .map(Rect::bottom)
        .max_by(f64::total_cmp)
}

/// The top of the display the sprite is against, which is where a climb ends.
fn ceiling_over(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    displays_spanning(x, snapshot)
        .map(|display| display.y)
        .min_by(f64::total_cmp)
}

fn displays_spanning<'a>(
    x: f64,
    snapshot: &'a WorldSnapshot,
) -> impl Iterator<Item = &'a Rect> + 'a {
    snapshot
        .displays
        .iter()
        .filter(move |display| display.spans_x(x))
}

/// The screen edge the sprite has just arrived at while moving into it.
///
/// ponytail: the outermost edges of all displays, not the true union of their
/// frames. Gaps between non-aligned displays are #4's problem, and this is the
/// hook they clamp at.
fn wall_reached(x: f64, velocity_x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    let left = snapshot
        .displays
        .iter()
        .map(|display| display.x)
        .min_by(f64::total_cmp)?;
    let right = snapshot
        .displays
        .iter()
        .map(|display| display.x + display.width)
        .max_by(f64::total_cmp)?;

    match velocity_x {
        v if v > 0.0 && x >= right => Some(right),
        v if v < 0.0 && x <= left => Some(left),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One 1000x800 display with its top-left at the origin.
    fn one_display() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
        }
    }

    fn snapshot(elapsed_ms: u32) -> WorldSnapshot {
        WorldSnapshot {
            displays: vec![one_display()],
            elapsed_ms,
            ..WorldSnapshot::default()
        }
    }

    /// Ticks long enough for anything in flight over an 800-point display to
    /// come to rest, and returns the last frame.
    fn settle(engine: &mut Engine, snapshot: &WorldSnapshot) -> Frame {
        (0..40).map(|_| engine.tick(snapshot)).last().unwrap()
    }

    /// A day in the life, as snapshots: the sprite falls onto a window, the
    /// window closes, it lands on the floor, dozes off, is poked awake, is
    /// picked up and carried, is dropped, then is flung at the screen edge and
    /// climbs it.
    fn a_day_in_the_life() -> Vec<WorldSnapshot> {
        let on_a_window = WorldSnapshot {
            windows: vec![Rect {
                x: 50.0,
                y: 400.0,
                width: 300.0,
                height: 200.0,
            }],
            ..snapshot(100)
        };

        let mut script: Vec<WorldSnapshot> = (0..40).map(|_| on_a_window.clone()).collect();
        script.extend((0..40).map(|_| snapshot(100)));
        script.push(snapshot(60_000));
        script.push(WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });
        script.extend((0..2).map(|_| WorldSnapshot {
            cursor: Point { x: 500.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        }));
        script.extend((0..10).map(|_| snapshot(100)));
        script.push(WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point {
                    x: 2000.0,
                    y: -400.0,
                },
            }],
            ..snapshot(100)
        });
        script.extend((0..60).map(|_| snapshot(100)));
        script
    }

    fn play(script: &[WorldSnapshot]) -> Vec<Frame> {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        script.iter().map(|s| engine.tick(s)).collect()
    }

    #[test]
    fn every_state_is_reached_and_every_state_is_left_again() {
        let states: Vec<State> = play(&a_day_in_the_life())
            .iter()
            .map(|frame| frame.state)
            .collect();

        for state in [
            State::Grounded,
            State::Falling,
            State::Dragged,
            State::Perched,
            State::Climbing,
            State::Asleep,
        ] {
            assert!(states.contains(&state), "{state:?} is never reached");
            assert!(
                states
                    .windows(2)
                    .any(|pair| pair[0] == state && pair[1] != state),
                "{state:?} is a dead end"
            );
        }
    }

    #[test]
    fn the_same_snapshots_twice_produce_the_same_frames() {
        let script = a_day_in_the_life();

        assert_eq!(play(&script), play(&script));
    }

    #[test]
    fn a_proposals_dialogue_is_spoken_once_and_not_repeated() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let spoken = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "greet".to_string(),
                dialogue: Some("morning".to_string()),
            }),
            ..snapshot(100)
        });
        assert_eq!(spoken.dialogue.as_deref(), Some("morning"));

        let quiet = engine.tick(&snapshot(100));
        assert_eq!(quiet.dialogue, None, "a line is said once, not held");
    }

    #[test]
    fn the_frame_names_the_animation_and_walks_through_it_as_time_passes() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        // At 8 frames per second an animation holds each frame for 125ms.
        let first = engine.tick(&snapshot(100));
        assert_eq!(first.animation, "fall");
        assert_eq!(first.frame_index, 0, "100ms in, still the first frame");

        let second = engine.tick(&snapshot(100));
        assert_eq!(second.frame_index, 1, "200ms in, the second frame");

        let landed = settle(&mut engine, &snapshot(100));
        assert_eq!(landed.animation, "idle");

        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        engine.tick(&snapshot(1_000));
        let restarted = engine.tick(&snapshot(100));
        assert_eq!(restarted.animation, "idle", "it landed in that first tick");
        assert_eq!(
            restarted.frame_index, 0,
            "a new animation starts from its first frame"
        );
    }

    #[test]
    fn a_sprite_thrown_at_the_screen_edge_climbs_it_and_lets_go_at_the_top() {
        let mut engine = Engine::new(Point { x: 900.0, y: 400.0 });

        let grabbed_wall = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 2000.0, y: 0.0 },
            }],
            ..snapshot(100)
        });
        assert_eq!(grabbed_wall.state, State::Climbing);
        assert_eq!(
            grabbed_wall.position.x, 1000.0,
            "the display's right edge, not past it"
        );

        let ascending = engine.tick(&snapshot(100));
        assert_eq!(ascending.state, State::Climbing);
        assert!(
            ascending.position.y < grabbed_wall.position.y,
            "it goes up: {ascending:?}"
        );

        // Climbing ends: at the top of the display there is nothing left to
        // climb, so it lets go and drops to the floor.
        let landed = settle(&mut engine, &snapshot(100));
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0);
    }

    #[test]
    fn a_sprite_left_alone_falls_asleep_and_a_poke_wakes_it() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &snapshot(100)).state, State::Grounded);

        // A full minute of nobody touching it.
        let asleep = engine.tick(&snapshot(60_000));
        assert_eq!(asleep.state, State::Asleep);

        let woken = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });
        assert_eq!(woken.state, State::Grounded, "awake and back on its feet");
    }

    #[test]
    fn time_spent_in_the_air_is_not_time_spent_idling() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        // One enormous tick: a minute passes while the sprite is airborne.
        assert_eq!(engine.tick(&snapshot(60_000)).state, State::Grounded);

        assert_eq!(
            engine.tick(&snapshot(100)).state,
            State::Grounded,
            "it has only just landed, so it has not been idle a minute"
        );
    }

    #[test]
    fn the_sprite_perches_on_a_window_top_edge_and_falls_when_the_window_goes() {
        let window = WorldSnapshot {
            windows: vec![Rect {
                x: 50.0,
                y: 400.0,
                width: 300.0,
                height: 200.0,
            }],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let perched = settle(&mut engine, &window);
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0, "the window's top edge");

        // The window closes out from under it.
        let dropped = engine.tick(&snapshot(100));
        assert_eq!(dropped.state, State::Falling);

        let landed = settle(&mut engine, &snapshot(100));
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0);
    }

    #[test]
    fn a_grab_takes_the_sprite_over_and_letting_go_drops_it() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let held = engine.tick(&WorldSnapshot {
            cursor: Point { x: 400.0, y: 250.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        assert_eq!(held.state, State::Dragged);
        assert_eq!(
            held.position,
            Point { x: 400.0, y: 250.0 },
            "follows the cursor"
        );

        let dragged_on = engine.tick(&WorldSnapshot {
            cursor: Point { x: 420.0, y: 240.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        assert_eq!(dragged_on.position, Point { x: 420.0, y: 240.0 });

        // The Grab is gone from the snapshot: the user let go.
        let released = engine.tick(&snapshot(100));
        assert_eq!(released.state, State::Falling);
        assert!(released.position.y > 240.0, "it drops: {released:?}");
    }

    #[test]
    fn letting_go_with_velocity_throws_the_sprite_instead_of_dropping_it() {
        let mut engine = Engine::new(Point { x: 100.0, y: 100.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 200.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });

        // Flung up and to the right at 300 points/s across, 200 points/s up.
        let thrown = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point {
                    x: 300.0,
                    y: -200.0,
                },
            }],
            ..snapshot(100)
        });

        assert_eq!(thrown.state, State::Falling);
        assert_eq!(thrown.velocity.x, 300.0, "gravity does not slow the arc");
        assert!(thrown.position.x > 200.0, "it travels across: {thrown:?}");
        assert!(
            thrown.position.y < 100.0,
            "an upward throw rises before it falls: {thrown:?}"
        );
    }

    #[test]
    fn a_sprite_in_mid_air_falls_and_comes_to_rest_on_the_floor() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let first = engine.tick(&snapshot(100));
        assert_eq!(first.state, State::Falling);
        assert!(first.position.y > 0.0, "it descends: {first:?}");

        let landed = (0..40).map(|_| engine.tick(&snapshot(100))).last().unwrap();

        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0, "the display's bottom edge");
        assert_eq!(landed.velocity, Point::default(), "at rest");
    }
}
