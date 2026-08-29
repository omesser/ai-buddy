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
    /// the Engine only flies it. Nothing else is a Throw, so one that arrives
    /// while the sprite is not being held is ignored.
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
    /// The part of each visible display a sprite may occupy.
    ///
    /// Not the whole display: a screen reserves strips of itself for furniture
    /// the sprite must not go behind, and the Shell takes those off before the
    /// Engine sees them. That is why the floor, the ceiling and both walls can
    /// all be derived from these rectangles without the Engine knowing a Dock
    /// exists.
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
    /// How long the current Animation has been playing, in milliseconds.
    ///
    /// Not a frame index: which frame that is depends on the fps and loop mode
    /// the Character Manifest declares, which the Engine has no business
    /// knowing. `character::Animation::frame_at` does that arithmetic.
    pub animation_ms: u32,
    /// A line to speak on this frame only. Dialogue is an event, not a state.
    pub dialogue: Option<String>,
}

/// How long a Poke keeps the sprite reacting.
///
/// ponytail: one duration for every Character's reaction. A tuning knob rather
/// than a frame count: how long the sprite stays startled is
/// behaviour, and how many frames that fills is the Character Manifest's
/// business. A `react` declared `loop = once` holds its last frame for the
/// remainder, which is what being briefly startled looks like.
///
/// #8 replaces this when Behaviors can play an Animation and say when it ends.
const REACT_MS: u32 = 600;

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
    /// Milliseconds of reacting left, after a Poke.
    ///
    /// Not a State: reacting is something the sprite does while standing,
    /// falling or perched, and giving it a State would mean deciding what it
    /// resumes as.
    reacting_ms: u32,
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
            reacting_ms: 0,
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

        // A Poke is answered, whatever else is going on. Being prodded is the
        // one thing a companion must never ignore, and it reads as alive
        // exactly because it interrupts.
        // Prodded again mid-reaction, it reacts again from the beginning. The
        // Animation's own clock has to be restarted for that to show: the name
        // has not changed, so the bookkeeping below would otherwise carry on
        // counting and a second Poke would extend a held last frame.
        let poked = snapshot.verbs.contains(&Verb::Poke);
        if poked {
            self.reacting_ms = REACT_MS;
        } else {
            self.reacting_ms = self.reacting_ms.saturating_sub(snapshot.elapsed_ms);
        }

        // A Grab wins over whatever the sprite was doing: the user's hand is
        // the one input that outranks the world.
        //
        // Including the usable floor. The cursor may go over the Dock, and a
        // held sprite goes where the cursor goes rather than stopping short of
        // a strip the user can plainly see it over. Letting go settles it
        // somewhere legal, because falling ends on the usable floor like any
        // other fall — so the reserved strip is somewhere the sprite can be
        // put and not somewhere it can come to rest. #39.
        if snapshot.verbs.contains(&Verb::Grab) {
            self.state = State::Dragged;
            self.position = snapshot.cursor;
            self.velocity = Point::default();
        } else if self.state == State::Dragged {
            // Let go. With velocity that is a Throw; without, it simply drops.
            self.velocity = thrown_velocity(snapshot).unwrap_or_default();
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

        // Reacting is drawn over whatever the sprite is doing, so a Poke shows
        // even mid-fall. It changes nothing about where the sprite is.
        let animation = if self.reacting_ms > 0 {
            "react"
        } else {
            animation_for(self.state)
        };
        if animation == self.animation && !poked {
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
            animation_ms: self.animation_ms,
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

/// The screen edge the sprite has just arrived at while moving into it, or the
/// nearest one when it is already outside every display. A sprite out there has
/// no floor under it and would otherwise fall for ever, so grabbing an edge is
/// how it gets back over a display at all.
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

    if x > right || (velocity_x > 0.0 && x >= right) {
        Some(right)
    } else if x < left || (velocity_x < 0.0 && x <= left) {
        Some(left)
    } else {
        None
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
    /// picked up and carried, is dropped, then is picked up again and flung at
    /// the screen edge, which it climbs.
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
            cursor: Point { x: 500.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
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
    fn the_frame_names_the_animation_and_how_long_it_has_been_playing() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let first = engine.tick(&snapshot(100));
        assert_eq!(first.animation, "fall");
        assert_eq!(first.animation_ms, 100);

        let second = engine.tick(&snapshot(100));
        assert_eq!(second.animation_ms, 200, "still falling, still accruing");

        let landed = settle(&mut engine, &snapshot(100));
        assert_eq!(landed.animation, "idle");

        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        engine.tick(&snapshot(1_000));
        let restarted = engine.tick(&snapshot(100));
        assert_eq!(restarted.animation, "idle", "it landed in that first tick");
        assert_eq!(
            restarted.animation_ms, 100,
            "a new animation starts its own clock rather than inheriting one"
        );
    }

    /// #39, at the end of the screen no falling test could reach: furniture at
    /// the top is only ever met by a climb, so a sprite that let go behind the
    /// menu bar was a defect nothing would have caught.
    ///
    /// The Engine learns nothing about menu bars here. It is handed the usable
    /// part of the display instead of the whole of it, and the ceiling it
    /// already derives moves with it.
    #[test]
    fn a_climb_ends_at_the_usable_top_rather_than_behind_the_menu_bar() {
        // A display reserving 30 points at the top, as a menu bar does.
        let usable = || WorldSnapshot {
            displays: vec![Rect {
                x: 0.0,
                y: 30.0,
                width: 1000.0,
                height: 770.0,
            }],
            elapsed_ms: 100,
            ..WorldSnapshot::default()
        };

        let mut engine = Engine::new(Point { x: 900.0, y: 400.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..usable()
        });
        let caught = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 2000.0, y: 0.0 },
            }],
            ..usable()
        });
        assert_eq!(caught.state, State::Climbing);

        let highest = (0..200)
            .map(|_| engine.tick(&usable()).position.y)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(
            highest, 30.0,
            "it lets go under the menu bar, not at the display's own top of 0"
        );

        let landed = settle(&mut engine, &usable());
        assert_eq!(
            landed.position.y, 800.0,
            "and falls to the usable floor, which this display does not inset"
        );
    }

    /// A second Poke restarts the reaction rather than extending a held frame.
    /// `react` is declared `loop = once`, so an Animation left running past its
    /// last frame simply holds it: without restarting the clock, prodding twice
    /// would look exactly like prodding once.
    #[test]
    fn poking_again_mid_reaction_starts_the_reaction_over() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        settle(&mut engine, &snapshot(100));

        let poke = || WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        };
        engine.tick(&poke());
        let mid = engine.tick(&snapshot(100));
        assert_eq!(mid.animation, "react");
        assert!(mid.animation_ms > 0, "the reaction has been running");

        let again = engine.tick(&poke());
        assert_eq!(again.animation, "react");
        assert_eq!(
            again.animation_ms, 0,
            "and the second Poke plays it from its first frame"
        );
    }

    /// #6: verbs arriving in the same tick resolve deterministically. A Grab
    /// and a Poke together is the ordinary case — a press that has just become
    /// a drag can share a tick with the click that preceded it — and the hand
    /// has to win, because a sprite that reacts instead of being picked up is a
    /// sprite that ignored you.
    #[test]
    fn a_grab_and_a_poke_in_one_tick_resolve_the_same_way_every_time() {
        let together = || WorldSnapshot {
            cursor: Point { x: 400.0, y: 200.0 },
            verbs: vec![Verb::Poke, Verb::Grab],
            ..snapshot(100)
        };
        let reversed = || WorldSnapshot {
            verbs: vec![Verb::Grab, Verb::Poke],
            ..together()
        };

        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        let first = engine.tick(&together());

        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        let second = engine.tick(&reversed());

        assert_eq!(
            first.state,
            State::Dragged,
            "the hand wins over the reaction"
        );
        assert_eq!(first.position, Point { x: 400.0, y: 200.0 });
        assert_eq!(
            first, second,
            "and the order the verbs arrive in changes nothing"
        );
    }

    /// #6: a Director proposal arriving during a Grab is deferred or dropped,
    /// never yanking the sprite. Being held is the one moment the sprite is the
    /// user's rather than the Director's.
    #[test]
    fn a_proposal_during_a_grab_never_moves_the_sprite() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        let held = WorldSnapshot {
            cursor: Point { x: 300.0, y: 300.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        };
        engine.tick(&held);

        let proposed = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "wander".to_string(),
                dialogue: Some("off we go".to_string()),
            }),
            ..held.clone()
        });

        assert_eq!(proposed.state, State::Dragged, "still in the hand");
        assert_eq!(
            proposed.position,
            Point { x: 300.0, y: 300.0 },
            "and exactly where the cursor left it"
        );
        assert_eq!(
            proposed.dialogue.as_deref(),
            Some("off we go"),
            "it may still speak, which moves nothing"
        );
    }

    /// #39 left this decision to #6 and asked for it to be asserted: a held
    /// sprite may be taken below the usable floor, over the Dock, because the
    /// cursor may go there. Letting go puts it back somewhere it can stand.
    #[test]
    fn a_sprite_dropped_below_the_usable_floor_settles_back_onto_it() {
        // A display whose usable part stops short of its bottom edge, as one
        // with a Dock does.
        let usable = || WorldSnapshot {
            displays: vec![Rect {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 700.0,
            }],
            elapsed_ms: 100,
            ..WorldSnapshot::default()
        };

        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        let held = engine.tick(&WorldSnapshot {
            cursor: Point { x: 500.0, y: 900.0 },
            verbs: vec![Verb::Grab],
            ..usable()
        });
        assert_eq!(held.position.y, 900.0, "the hand may take it over the Dock");

        let landed = settle(&mut engine, &usable());
        assert_eq!(
            landed.position.y, 700.0,
            "and letting go settles it on the usable floor"
        );
        assert_eq!(landed.state, State::Grounded);
    }

    /// #6: a Poke is the one interaction that has to be visible, and the
    /// Required Animation Set carries `react` for it. Before this the verb only
    /// woke a sleeping sprite and reset the idle timer, so prodding a wide-awake
    /// one did nothing at all.
    #[test]
    fn a_poke_plays_the_reaction_and_then_goes_back_to_what_it_was_doing() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        let resting = settle(&mut engine, &snapshot(100));
        assert_eq!(resting.animation, "idle");

        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });
        assert_eq!(poked.animation, "react");
        assert_eq!(
            poked.state,
            State::Grounded,
            "reacting is not a State: it is still standing where it stood"
        );

        assert_eq!(
            engine.tick(&snapshot(100)).animation,
            "react",
            "and it lasts longer than the tick it started on"
        );

        let after = (0..20)
            .map(|_| engine.tick(&snapshot(100)))
            .last()
            .expect("twenty ticks produce twenty frames");
        assert_eq!(after.animation, "idle", "then back to idling");
    }

    /// Being prodded mid-air is answered too, and does not change the fall.
    #[test]
    fn a_poke_while_falling_reacts_without_interrupting_the_fall() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        let falling = engine.tick(&snapshot(100));
        assert_eq!(falling.state, State::Falling);

        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });
        assert_eq!(poked.animation, "react");
        assert_eq!(poked.state, State::Falling);
        assert!(
            poked.position.y > falling.position.y,
            "still going down: {poked:?}"
        );
    }

    #[test]
    fn a_sprite_thrown_at_the_screen_edge_climbs_it_and_lets_go_at_the_top() {
        let mut engine = Engine::new(Point { x: 900.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
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

    #[test]
    fn a_window_is_passed_through_from_below_and_landed_on_from_above() {
        let window = WorldSnapshot {
            windows: vec![Rect {
                x: 50.0,
                y: 400.0,
                width: 300.0,
                height: 200.0,
            }],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 550.0 });

        // Held inside the window, below its top edge, and flung straight up.
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 100.0, y: 550.0 },
            verbs: vec![Verb::Grab],
            ..window.clone()
        });
        let thrown = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 0.0, y: -1000.0 },
            }],
            ..window.clone()
        });
        assert_eq!(
            thrown.state,
            State::Falling,
            "the edge above it is not a surface from underneath: {thrown:?}"
        );

        let risen = (0..2).map(|_| engine.tick(&window)).last().unwrap();
        assert!(
            risen.position.y < 400.0,
            "it rises through the top edge: {risen:?}"
        );
        assert_eq!(risen.state, State::Falling);

        // The same edge, approached from above, catches it.
        let perched = settle(&mut engine, &window);
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0, "the window's top edge");
    }

    #[test]
    fn a_sprite_over_no_display_is_recovered_onto_the_nearest_one() {
        // A display was unplugged out from under it: nothing spans its x, so
        // there is no floor beneath it and nothing to fall towards.
        let mut engine = Engine::new(Point { x: 2000.0, y: 0.0 });

        let caught = engine.tick(&snapshot(100));
        assert_eq!(caught.position.x, 1000.0, "hauled back to the nearest edge");

        let landed = settle(&mut engine, &snapshot(100));
        assert_eq!(
            landed.state,
            State::Grounded,
            "it stops falling: {landed:?}"
        );
        assert_eq!(landed.position.y, 800.0, "the display's bottom edge");
    }

    #[test]
    fn a_throw_is_the_release_of_a_grab_so_a_resting_sprite_ignores_one() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        let resting = settle(&mut engine, &snapshot(100));
        assert_eq!(resting.state, State::Grounded);

        // A Throw with nothing holding the sprite is not a Throw at all.
        let unmoved = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point {
                    x: 2000.0,
                    y: -400.0,
                },
            }],
            ..snapshot(100)
        });
        assert_eq!(unmoved.state, State::Grounded, "not flung: {unmoved:?}");
        assert_eq!(unmoved.position, resting.position);
        assert_eq!(unmoved.velocity, Point::default());
    }
}
