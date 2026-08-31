//! The Engine: `WorldSnapshot` in, `Frame` out, once per tick.
//!
//! Pure and synchronous. It reads no clock, holds no timers and performs no
//! I/O, so time reaches it only as elapsed milliseconds on a snapshot. That is
//! what lets every spatial property be tested by constructing snapshots and
//! asserting frames, with no windowing system, no model and no waiting.

use crate::character::{Behavior, Primitive};
pub use crate::window_source::WindowId;
use std::collections::{BTreeMap, BTreeSet};

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

/// One visible window: which one it is, and where.
///
/// The id is opaque here and only ever compared for equality. That comparison
/// is the whole point: it is what says the window under the sprite this tick is
/// the one it was standing on last tick, which geometry cannot say. #85.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub id: WindowId,
    pub rect: Rect,
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
    /// A right-click on the sprite. Opens the same menu the tray icon opens,
    /// which #18 owns; until it exists the verb is accepted and shows nothing.
    Menu,
    /// A double-click on the sprite. Opens the chat surface, which is #17;
    /// until it exists the verb is accepted and shows nothing.
    ///
    /// Accepted now rather than added later because the verb set is fixed at
    /// five: every verb is a tax on every Character that will ever exist, and a
    /// sixth would mean a ninth Required Animation.
    Summon,
}

/// What the Director proposed since the previous tick.
///
/// Advisory: the Engine plays the named Behavior if the Character declares one
/// by that name and the sprite's State permits its Primitives, and refuses it
/// otherwise. The line is spoken either way, since speaking moves nothing.
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
    /// Visible windows in descending z-order.
    pub windows: Vec<Window>,
    pub cursor: Point,
    /// Interaction verbs pending since the previous tick.
    pub verbs: Vec<Verb>,
    /// Milliseconds since the previous tick.
    pub elapsed_ms: u32,
    /// A Behavior proposal delivered since the previous tick, if the Director
    /// made one. Advisory: the Engine is free to refuse it.
    pub proposal: Option<BehaviorProposal>,
    /// Bumps when the assembler actually re-read the window list. Zero means
    /// the caller did not say, and the Engine treats every tick as a fresh
    /// sample — which is what the tests that construct snapshots by hand are.
    /// A reused generation is a tick between polls: the rectangles have not
    /// changed, and a riding sprite has to coast on the last Perch velocity
    /// or it hitch-steps while the window slides. #98.
    pub poll_generation: u64,
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
    /// The Behavior that started playing on this frame, if a proposal was
    /// taken. An event like `dialogue`, and for the Shell rather than the
    /// renderer: a proposal is advisory, so what the Director suggested and
    /// what the user actually saw are different lists, and repetition is
    /// suppressed on the second.
    pub behavior: Option<String>,
    /// Whether this tick carried the sprite with a moving Perch. The Shell
    /// polls the window list at the frame rate only then. #98.
    pub riding: bool,
    /// Which way the sprite is pointed, as -1.0 (left) or 1.0 (right). Only
    /// horizontal travel turns it, so a stop keeps the last heading and the
    /// renderer can mirror the art by it without flicker at rest.
    pub facing: f64,
}

/// How long one Primitive holds the screen.
///
/// ponytail: one duration for every Primitive of every Character. The Engine
/// deliberately does not know an Animation's length — fps and loop mode are the
/// Character Manifest's — so it cannot play a Primitive until the art runs out.
/// Art shorter than the turn costs nothing: `loop = once` holds its last frame
/// for the remainder, which is what a brief startle looks like. Longer art is
/// the ceiling, and Blip already sits on it — sleep's `fps = 1` over
/// two frames is a 2000ms strip the sprite leaves at frame 0. Give a Primitive
/// its own duration when a Character's art needs to outlast this one.
const PRIMITIVE_MS: u32 = 600;

/// How long a resting, untouched sprite waits before it goes to sleep. A tuning
/// knob: long enough not to nod off mid-conversation, short enough that a sprite
/// on an unattended desktop settles down.
const SLEEP_AFTER_MS: u32 = 60_000;

/// Points per second the sprite hauls itself up a screen edge. A tuning knob.
const CLIMB_SPEED: f64 = 200.0;

/// Points per second the sprite walks along whatever it is standing on. A
/// tuning knob, slower than a climb: hauling yourself up an edge is urgent and
/// strolling along a title bar is not.
const WALK_SPEED: f64 = 120.0;

/// Points per second squared. A tuning knob: the number that makes a fall read
/// as heavy rather than floaty is found by watching it, not by deriving it.
/// 3600 keeps a Throw on the display long enough to see, instead of spending
/// the flight above the usable frame. #100.
const GRAVITY: f64 = 3600.0;

/// How far below the usable top the feet must stay so the art hanging above
/// them stays on screen. The Engine does not know the art's size; 128 is
/// 32px at 4×, and a perch closer than that is the title bar under the
/// menu bar that used to show only legs. #100.
const CEILING_CLEARANCE: f64 = 128.0;

/// Points per second squared. The yank gate is this times `YANK_WINDOW_S`:
/// a change in Perch speed larger than that, measured against the speed
/// from about one idle poll ago, drops the sprite. The last 16 ms slope
/// treats WindowServer jitter as a yank. #98.
pub const RIDE_ACCELERATION: f64 = 10_000.0;

/// How far back the yank gate looks. Fast poll still tracks the window;
/// only the fall decision stays on this cadence. #98.
const YANK_WINDOW_S: f64 = 0.1;

pub struct Engine {
    position: Point,
    velocity: Point,
    state: State,
    /// Milliseconds the sprite has spent on its feet, untouched.
    idle_ms: u32,
    animation: &'static str,
    /// Milliseconds since the current Animation started.
    animation_ms: u32,
    /// The Behaviors the Character declares, which a proposal names.
    behaviors: BTreeMap<String, Behavior>,
    /// The windows of the previous tick, to tell a window that has come to
    /// contain the sprite from one that contained it all along. See
    /// `swallowed_by`.
    previous_windows: Vec<Window>,
    /// Where the sprite stood at the end of the previous tick — the other half
    /// of what says whether a window has come to contain it.
    previous_position: Point,
    /// The Primitives of the Behavior being played, last first, so the one on
    /// screen is on top.
    ///
    /// Not a State: playing a Behavior is something the sprite does while
    /// standing, falling or perched, and giving it a State would mean deciding
    /// what it resumes as.
    playing: Vec<Primitive>,
    /// Milliseconds left of the Primitive being played.
    primitive_ms: u32,
    /// Which way the sprite is pointed, as -1 or 1. A walk needs a direction
    /// and the Primitive carries none, so it goes the way it was last heading.
    facing: f64,
    /// Whether this tick translated the sprite with a moving Perch. Decides
    /// the Hold animation; not a State — the sprite is still Perched. #98.
    riding: bool,
    /// Last observed velocity of the ridden Perch, for the acceleration gate
    /// and for coasting between polls. #98.
    perch_velocity: Point,
    /// Last observed acceleration of the ridden Perch. Constant-velocity
    /// coasting hitch-steps when a drag speeds up or slows; keeping the
    /// derivative lets the in-between ticks follow the curve. #98.
    perch_acceleration: Point,
    /// Seconds since the last fresh window sample, so a coast integrates
    /// from the sample rather than compounding Euler error each tick.
    coast_s: f64,
    /// The assembler's last `poll_generation`. Equal generations mean the
    /// window list was reused and a ride has to coast rather than wait. #98.
    last_poll_generation: u64,
    /// The Perch the sprite last stood on, remembered so a coast that has
    /// left the stale rectangle can still match the window when it updates.
    /// The id is what matches it in the next sample; the rectangle is the
    /// origin a coast integrates from. #85.
    last_perch: Option<Window>,
    /// How far along that Perch the sprite stands, so a snap back onto a
    /// fresh sample keeps the place it was holding.
    hold_offset_x: f64,
    /// Seconds since the last fresh window sample. Velocity and acceleration
    /// are that interval, not a constant poll, because idle and ride differ. #98.
    since_sample_s: f64,
    /// Perch velocity from about `YANK_WINDOW_S` ago. The fall decision
    /// compares against this, not the last 16 ms sample. #98.
    yank_reference: Point,
    since_yank_ref_s: f64,
    /// #84: quiet but not gone. Director proposals are refused and unprompted
    /// dialogue is not spoken, while Poke/Grab/Throw still work and the
    /// Character stays visible.
    do_not_disturb: bool,
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
            behaviors: BTreeMap::new(),
            previous_windows: Vec::new(),
            previous_position: position,
            playing: Vec::new(),
            primitive_ms: 0,
            facing: 1.0,
            riding: false,
            perch_velocity: Point::default(),
            perch_acceleration: Point::default(),
            coast_s: 0.0,
            last_poll_generation: 0,
            last_perch: None,
            hold_offset_x: 0.0,
            since_sample_s: 0.0,
            yank_reference: Point::default(),
            since_yank_ref_s: 0.0,
            do_not_disturb: false,
        }
    }

    /// The Behaviors this Character declares. Nothing else reaches the Engine
    /// from a Character Package: art is the renderer's, and a Behavior is
    /// Primitives the Engine already owns.
    pub fn with_behaviors(mut self, behaviors: BTreeMap<String, Behavior>) -> Self {
        self.behaviors = behaviors;
        self
    }

    /// #84: toggle Do Not Disturb. The Character stays visible but stops
    /// starting things: no Director proposals are applied and no unprompted
    /// dialogue is spoken. Poke, Grab, and Throw still work.
    pub fn set_do_not_disturb(&mut self, enabled: bool) {
        self.do_not_disturb = enabled;
    }

    pub fn do_not_disturb(&self) -> bool {
        self.do_not_disturb
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
        }

        // Being addressed is not being left alone, so a proposal holds off the
        // sleep timer without waking a sprite that has already nodded off —
        // only a verb does that. The timer is otherwise still running when the
        // Behavior is played at the end of the tick, and a sprite that nods off
        // first is asleep when the gate reads its State. #5.
        //
        // #84: Do Not Disturb means proposals do not count as being addressed,
        // so the idle timer keeps running and the sprite settles to sleep.
        if snapshot.proposal.is_some() && !self.do_not_disturb {
            self.idle_ms = 0;
        }

        // What was already playing ages before anything new starts, so a
        // Primitive begun this tick gets its whole turn rather than losing this
        // tick's milliseconds to the one it replaced.
        let mut started = self.advance(snapshot.elapsed_ms);

        // `woke` marks a sprite a verb roused, so the footing it is put back
        // on is not mistaken for one it arrived at. See the landing below.
        let (state, woke) = transition::on_verbs(self.state, &snapshot.verbs);

        // A Grab wins over whatever the sprite was doing: the user's hand is
        // the one input that outranks the world.
        //
        // Including the usable floor. The cursor may go over the Dock, and a
        // held sprite goes where the cursor goes rather than stopping short of
        // a strip the user can plainly see it over. Letting go settles it
        // somewhere legal, because falling ends on the usable floor like any
        // other fall — so the reserved strip is somewhere the sprite can be
        // put and not somewhere it can come to rest. #39.
        if state == State::Dragged {
            self.position = snapshot.cursor;
            self.velocity = Point::default();
        } else if self.state == State::Dragged {
            // Let go. With velocity that is a Throw; without, it simply drops.
            self.velocity = thrown_velocity(snapshot).unwrap_or_default();
        }

        // Walking is the Engine's, deciding to walk is not: nothing else here
        // moves the sprite of its own accord. A walk needs no ending — it lasts
        // until the sprite runs out of Perch, which is the whole point of it —
        // so the velocity holds when the Behavior that started it is over, and
        // through a Poke's reaction, which interrupts what the sprite is doing
        // rather than where it is going. What does stop it is a Primitive that
        // is the sprite standing still: `walk sit` would otherwise slide along
        // the edge it sat down on.
        if matches!(state, State::Grounded | State::Perched) {
            match self.on_screen() {
                Some(Primitive::Walk) => self.velocity.x = self.facing * WALK_SPEED,
                Some(Primitive::Idle | Primitive::Sit | Primitive::Sleep | Primitive::Hold) => {
                    self.velocity.x = 0.0
                }
                _ => {}
            }
        }

        let contact = self.integrate(state, dt, snapshot);

        // Still moving is still awake. A walk proposed just before the timer
        // comes due would otherwise leave the sprite gliding along the edge
        // playing `sleep`.
        let rested = self.idle_ms >= SLEEP_AFTER_MS && self.velocity.x == 0.0;
        self.state = transition::on_contact(state, contact, rested);

        if self.velocity.x != 0.0 {
            self.facing = self.velocity.x.signum();
        }

        // Losing its footing abandons the rest of a Behavior: what the sprite
        // was in the middle of doing was only ever a thing to do standing up.
        if !self.permitted(&self.playing) {
            self.playing.clear();
            self.primitive_ms = 0;
            started = true;
        }

        // Arriving is an event and not a State: by the time the sprite has
        // landed it is already standing, and `land` is the animation of the
        // moment in between. The Engine plays it itself because no Director
        // could propose it in time.
        // Not a sprite woken onto the same footing it fell asleep on: settling
        // that by falling is how the Engine asks what is underneath, and a
        // sprite that answers in the tick it was asked never left the ground.
        // A wake with nothing under it still falls, and still lands, later.
        if matches!(contact, Some(transition::Contact::Landed(_))) && !woke {
            started |= self.play(&[Primitive::Land]);
        }

        // Riding is an event the Director cannot propose in time, the same
        // as landing. Holding on is not resting, so an Asleep sprite that
        // has to ride wakes rather than sleeping through the move. #98.
        if self.riding {
            self.idle_ms = 0;
            self.state = State::Perched;
            if self.on_screen() != Some(Primitive::Hold) {
                started |= self.play(&[Primitive::Hold]);
            }
        } else if self.on_screen() == Some(Primitive::Hold) {
            self.playing.clear();
            self.primitive_ms = 0;
            started = true;
        }

        // A proposal is advisory, so a Behavior this Character does not declare
        // is refused rather than reported, and refusing it interrupts nothing.
        //
        // After the sprite has been moved, so the State the gate reads is the
        // one the tick ends in. A walk therefore takes its first step on the
        // tick after the proposal, which is what SPEC.md asks for.
        //
        // #84: Do Not Disturb refuses proposals before they reach the State
        // gate, so the Character stops starting things while staying visible.
        //
        // #119: dialogue with an empty behavior plays `talk`. Duration is
        // PRIMITIVE_MS, independent of bubble reading time.
        let mut behavior = None;
        if let Some(proposal) = &snapshot.proposal {
            if !self.do_not_disturb {
                if let Some(primitives) = self.chain(&proposal.behavior) {
                    if self.play(&primitives) {
                        started = true;
                        behavior = Some(proposal.behavior.clone());
                    }
                } else if proposal.behavior.is_empty() && proposal.dialogue.is_some() {
                    if self.play(&[Primitive::Talk]) {
                        started = true;
                    }
                }
            }
        }

        // A Poke is answered, whatever else is going on. Being prodded is the
        // one thing a companion must never ignore, and it reads as alive
        // exactly because it interrupts — including a Behavior, and including
        // its own reaction: prodded again, it reacts again from the beginning.
        if snapshot.verbs.contains(&Verb::Poke) {
            started |= self.play(&[Primitive::React]);
        }

        // A Behavior is drawn over whatever the sprite is doing, so a Poke shows
        // even mid-fall. It changes nothing about where the sprite is.
        let animation = match self.on_screen() {
            Some(primitive) => animation_of(primitive),
            // A walk outlasts the Primitive that starts it: the velocity holds
            // until the sprite runs out of Perch, so the Animation has to hold
            // with it rather than dropping back to standing mid-stride.
            None if self.is_walking() => "walk",
            None => animation_for(self.state),
        };
        // A Primitive that starts restarts the Animation's clock even when the
        // name has not changed, or a second Poke would extend a held last frame
        // instead of playing the reaction again.
        if animation == self.animation && !started {
            self.animation_ms = self.animation_ms.saturating_add(snapshot.elapsed_ms);
        } else {
            self.animation = animation;
            self.animation_ms = 0;
        }

        self.previous_windows.clone_from(&snapshot.windows);
        self.previous_position = self.position;
        self.last_poll_generation = snapshot.poll_generation;
        if matches!(self.state, State::Perched | State::Asleep) {
            if let Some(perch) = perch_at(self.position, &snapshot.windows) {
                self.last_perch = Some(perch);
                self.hold_offset_x = self.position.x - perch.rect.x;
            }
        } else {
            self.rest_perch();
            self.last_perch = None;
            self.hold_offset_x = 0.0;
        }

        Frame {
            position: self.position,
            velocity: self.velocity,
            state: self.state,
            animation: self.animation,
            animation_ms: self.animation_ms,
            dialogue: if self.do_not_disturb {
                None
            } else {
                snapshot
                    .proposal
                    .as_ref()
                    .and_then(|proposal| proposal.dialogue.clone())
            },
            behavior,
            riding: self.riding,
            facing: self.facing,
        }
    }

    /// Move the sprite through one tick's worth of `state`'s physics, and
    /// report what its body met. Position and velocity are settled here; what
    /// the sprite becomes as a result is `transition`'s to say, so no State is
    /// read or written past the one this is handed.
    fn integrate(
        &mut self,
        state: State,
        dt: f64,
        snapshot: &WorldSnapshot,
    ) -> Option<transition::Contact> {
        use transition::Contact;

        self.riding = false;
        match state {
            State::Falling => {
                self.velocity.y += GRAVITY * dt;
                self.position.x += self.velocity.x * dt;

                if let Some(wall) = wall_reached(self.position.x, self.velocity.x, snapshot) {
                    // Arriving at a screen edge sideways is a catch, not a stop.
                    // It also keeps the sprite inside the displays.
                    self.position.x = wall;
                    self.velocity = Point::default();
                    Some(Contact::Wall)
                } else {
                    let next_y = self.position.y + self.velocity.y * dt;

                    // Rising only: a Grab can put the feet above the ceiling,
                    // and snapping them down would teleport a drop. #100.
                    if self.velocity.y < 0.0 {
                        if let Some(ceiling) = ceiling_over(self.position.x, snapshot) {
                            let stop_at = self.position.y.min(ceiling);
                            if next_y < stop_at {
                                self.position.y = stop_at;
                                self.velocity.y = 0.0;
                                return Some(Contact::Ceiling);
                            }
                        }
                    }

                    match support_below(self.position, snapshot) {
                        Some(support) if next_y >= support.y => {
                            self.position.y = support.y;
                            self.velocity = Point::default();
                            Some(Contact::Landed(support.surface))
                        }
                        _ => {
                            self.position.y = next_y;
                            Some(Contact::Airborne)
                        }
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
                    Some(Contact::Ceiling)
                } else {
                    None
                }
            }
            // Resting is only ever resting on something. When that something
            // moves slowly the sprite Holds and rides it (#98). A resize is
            // a move of the top edge and rides the same way (#85). A yank, a
            // close, or walking off the end leaves it in the air, carrying
            // whatever speed it walked off with.
            State::Grounded | State::Perched | State::Asleep => {
                self.position.x += self.velocity.x * dt;
                if self.last_perch.is_some() {
                    self.hold_offset_x += self.velocity.x * dt;
                }

                let fresh = snapshot.poll_generation == 0
                    || snapshot.poll_generation != self.last_poll_generation;
                self.since_sample_s += dt;

                if fresh {
                    let sample_s = if snapshot.poll_generation == 0 {
                        dt
                    } else {
                        self.since_sample_s
                    };
                    self.since_sample_s = 0.0;
                    match self.perch_carry(snapshot, sample_s) {
                        PerchCarry::Ride(window) => {
                            self.place_on(window);
                            self.last_perch = Some(window);
                            self.riding = true;
                        }
                        PerchCarry::Still(window) => {
                            self.place_on(window);
                            self.last_perch = Some(window);
                            self.rest_perch();
                        }
                        PerchCarry::Yank => {
                            self.rest_perch();
                            return Some(Contact::Airborne);
                        }
                        PerchCarry::Lost => self.rest_perch(),
                    }
                } else if self.coasting() {
                    // The window is still moving on screen; we just have not
                    // been told yet. Stale rectangles would call this a fall.
                    self.coast_s += dt;
                    let t = self.coast_s;
                    if let Some(origin) = self.last_perch.map(|perch| perch.rect) {
                        self.position.x = origin.x
                            + self.hold_offset_x
                            + self.perch_velocity.x * t
                            + 0.5 * self.perch_acceleration.x * t * t;
                        self.position.y = origin.y
                            + self.perch_velocity.y * t
                            + 0.5 * self.perch_acceleration.y * t * t;
                    }
                    self.riding = true;
                    return Some(Contact::Standing);
                }

                match footing(self.position, snapshot, |window| self.swallowed_by(window)) {
                    Some(footing) if footing.y < self.position.y => {
                        self.position.y = footing.y;
                        Some(Contact::Lifted(footing.surface))
                    }
                    Some(footing) if footing.y == self.position.y => Some(Contact::Standing),
                    _ => Some(Contact::Airborne),
                }
            }
            State::Dragged => None,
        }
    }

    /// What a fresh window sample says about the Perch the sprite was on. #98.
    fn perch_carry(&mut self, snapshot: &WorldSnapshot, sample_s: f64) -> PerchCarry {
        let Some(previous) = self
            .last_perch
            .or_else(|| perch_at(self.previous_position, &self.previous_windows))
        else {
            return PerchCarry::Lost;
        };
        let Some(index) = snapshot
            .windows
            .iter()
            .position(|window| window.id == previous.id)
        else {
            return PerchCarry::Lost;
        };
        let current = snapshot.windows[index];
        // An edge you cannot see is gone, whether the sprite was landing or
        // already standing on it. #100.
        if !is_perch(index, self.position.x, snapshot) {
            return PerchCarry::Lost;
        }
        let delta = Point {
            x: current.rect.x - previous.rect.x,
            y: current.rect.y - previous.rect.y,
        };
        if delta.x == 0.0 && delta.y == 0.0 {
            return PerchCarry::Still(current);
        }

        let sample_s = sample_s.max(0.001);
        let velocity = Point {
            x: delta.x / sample_s,
            y: delta.y / sample_s,
        };
        let ax = (velocity.x - self.perch_velocity.x) / sample_s;
        let ay = (velocity.y - self.perch_velocity.y) / sample_s;
        let dv = (velocity.x - self.yank_reference.x).hypot(velocity.y - self.yank_reference.y);
        if dv > RIDE_ACCELERATION * YANK_WINDOW_S {
            return PerchCarry::Yank;
        }
        // From rest there is no curve yet — only a speed. Treating that
        // first sample as constant velocity avoids a fake launch that
        // snaps back when the drag holds.
        self.perch_acceleration = if self.perch_velocity.x == 0.0 && self.perch_velocity.y == 0.0 {
            Point::default()
        } else {
            Point { x: ax, y: ay }
        };
        self.perch_velocity = velocity;
        self.since_yank_ref_s += sample_s;
        if self.since_yank_ref_s >= YANK_WINDOW_S {
            self.yank_reference = velocity;
            self.since_yank_ref_s = 0.0;
        }
        self.coast_s = 0.0;
        PerchCarry::Ride(current)
    }

    fn rest_perch(&mut self) {
        self.perch_velocity = Point::default();
        self.perch_acceleration = Point::default();
        self.coast_s = 0.0;
        self.since_sample_s = 0.0;
        self.yank_reference = Point::default();
        self.since_yank_ref_s = 0.0;
    }

    fn coasting(&self) -> bool {
        self.perch_velocity.x != 0.0
            || self.perch_velocity.y != 0.0
            || self.perch_acceleration.x != 0.0
            || self.perch_acceleration.y != 0.0
    }

    /// Put the sprite back on `window` at the offset it was holding.
    fn place_on(&mut self, window: Window) {
        self.position.x = window.rect.x + self.hold_offset_x;
        self.position.y = window.rect.y;
    }

    /// Whether `window` has come to contain the sprite this tick: dragged over
    /// it, or walked into where two windows overlap. One that already had the
    /// sprite inside it is not swallowing it — a maximized window contains
    /// every smaller window in front of it, so the sprite is inside one from
    /// the moment it lands, and raising that window would otherwise fling the
    /// sprite to the top of the screen and keep it there. #78.
    ///
    /// An Engine method rather than part of `footing` because "come to" takes
    /// the previous tick to judge, and the Engine is what remembers one.
    fn swallowed_by(&self, window: &Window) -> bool {
        swallows(&window.rect, self.position)
            && !self.previous_windows.iter().any(|before| {
                before.id == window.id && swallows(&before.rect, self.previous_position)
            })
    }

    /// The Primitive being played, if any. Last of `playing`, because `play`
    /// stores the sequence reversed so the one on screen is the one on top.
    fn on_screen(&self) -> Option<Primitive> {
        self.playing.last().copied()
    }

    /// Under way on foot, rather than in the air with the same speed on it.
    fn is_walking(&self) -> bool {
        matches!(self.state, State::Grounded | State::Perched) && self.velocity.x != 0.0
    }

    /// Age the Behavior being played by `elapsed_ms`, moving on to each
    /// Primitive as the one before it runs out. True when the Primitive on
    /// screen changed.
    fn advance(&mut self, elapsed_ms: u32) -> bool {
        let mut left = elapsed_ms;
        let mut moved_on = false;

        while !self.playing.is_empty() && left >= self.primitive_ms {
            left -= self.primitive_ms;
            self.playing.pop();
            self.primitive_ms = PRIMITIVE_MS;
            moved_on = true;
        }
        if self.playing.is_empty() {
            self.primitive_ms = 0;
        } else {
            self.primitive_ms -= left;
        }

        moved_on
    }

    /// Start playing `primitives`, unless the State the sprite is in forbids
    /// any of them. True when it started.
    ///
    /// All or nothing: a Behavior is a sequence its author meant to be seen
    /// whole, and playing the half of it that fits leaves the sprite stopping
    /// mid-thought.
    fn play(&mut self, primitives: &[Primitive]) -> bool {
        if primitives.is_empty() || !self.permitted(primitives) {
            return false;
        }
        // Last first, so the Primitive on screen is the one on top.
        self.playing = primitives.iter().rev().copied().collect();
        self.primitive_ms = PRIMITIVE_MS;
        true
    }

    /// Whether the State the sprite is in permits every one of `primitives`.
    ///
    /// Expression carries in any State: being startled or speaking says nothing
    /// about where the sprite's feet are, which is why a Poke is answered
    /// mid-fall. Everything else settles or moves the sprite, and only means
    /// something while it is standing on a surface — there is no sitting down
    /// in mid-air, and none of it while asleep, which is a thing to be woken
    /// out of rather than to act from.
    fn permitted(&self, primitives: &[Primitive]) -> bool {
        primitives.iter().all(|primitive| match primitive {
            Primitive::React | Primitive::Talk => true,
            _ => matches!(self.state, State::Grounded | State::Perched),
        })
    }

    /// Every Primitive a named Behavior plays, the Behaviors it chains into
    /// included, or nothing when the Character does not declare it.
    ///
    /// Flattened when play starts rather than followed a link at a time,
    /// because a Behavior is one thing to abandon and one thing to refuse.
    /// Load-time validation rejects a chain that comes back on itself, and this
    /// stops on one anyway: the Engine is handed Behaviors rather than a
    /// validated Character, and hanging the frame loop is the one thing
    /// ADR-0002 promises no package can do.
    fn chain(&self, behavior: &str) -> Option<Vec<Primitive>> {
        let mut primitives = Vec::new();
        let mut walked: BTreeSet<&str> = BTreeSet::new();
        let mut current = self.behaviors.get_key_value(behavior)?;

        while walked.insert(current.0.as_str()) {
            primitives.extend(current.1.primitives.iter().copied());
            match current.1.then.as_deref() {
                Some(next) => match self.behaviors.get_key_value(next) {
                    Some(next) => current = next,
                    None => break,
                },
                None => break,
            }
        }

        Some(primitives)
    }
}

/// The State machine, in one place.
///
/// The only writer of State in the Engine: verbs move it before physics and
/// contacts move it after, and `tick` stores nothing but what these two
/// functions return. Geometry reports Surfaces and `integrate` reports
/// Contacts precisely so that neither has an opinion on what the sprite
/// becomes — a transition added anywhere else is the scattering this module
/// exists to end.
mod transition {
    use super::{State, Surface, Verb};

    /// What the sprite's body met while `Engine::integrate` moved it.
    #[derive(Clone, Copy)]
    pub enum Contact {
        /// Nothing underfoot: still in the air, or the footing is gone.
        Airborne,
        /// Came down on a surface at the end of a fall.
        Landed(Surface),
        /// Put on top of a window that had come to contain it. Not a landing:
        /// the sprite never left the ground it thought it had.
        Lifted(Surface),
        /// Still standing exactly where it stood.
        Standing,
        /// Reached a screen edge sideways, mid-air.
        Wall,
        /// Met the usable top while rising or climbing. #100.
        Ceiling,
    }

    /// Where the user's hand leaves the sprite before any physics runs, and
    /// whether a verb woke it. The wake is reported rather than left for the
    /// caller to re-derive, so what waking means is written here once.
    pub fn on_verbs(state: State, verbs: &[Verb]) -> (State, bool) {
        let woke = state == State::Asleep && !verbs.is_empty();

        if verbs.contains(&Verb::Grab) {
            return (State::Dragged, woke);
        }
        let state = match state {
            // Let go. Thrown or simply dropped, what follows is a fall.
            State::Dragged => State::Falling,
            // Woken. Whether it is still standing on anything is settled by
            // falling, the same as any other loss of footing.
            _ if woke => State::Falling,
            _ => state,
        };
        (state, woke)
    }

    /// Where this tick's physics leaves the sprite. `rested` is whether it
    /// has been untouched and still for long enough to nod off.
    pub fn on_contact(state: State, contact: Option<Contact>, rested: bool) -> State {
        match contact {
            // Physics had nothing to report: a held sprite, or a climb still
            // under way.
            None => state,
            Some(Contact::Wall) => State::Climbing,
            Some(Contact::Airborne | Contact::Ceiling) => State::Falling,
            Some(Contact::Landed(surface) | Contact::Lifted(surface)) => match surface {
                Surface::Floor => State::Grounded,
                Surface::Perch => State::Perched,
            },
            Some(Contact::Standing) if rested => State::Asleep,
            Some(Contact::Standing) => state,
        }
    }
}

/// Which of the Required Animation Set a Primitive plays.
fn animation_of(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Idle => "idle",
        // The Animation only. The motion belongs to the walk a proposal starts,
        // which outlives this Primitive's turn and ends on running out of Perch.
        Primitive::Walk => "walk",
        Primitive::Land => "land",
        Primitive::Sit => "sit",
        Primitive::Sleep => "sleep",
        Primitive::React => "react",
        Primitive::Talk => "talk",
        Primitive::Hold => "hold",
    }
}

/// Which of the Required Animation Set a State plays.
///
/// A dragged sprite dangles from the cursor, which is what `fall` already
/// draws; being picked up is not the same as Holding onto a moving Perch,
/// and the required set spends its ninth Animation on the latter. #98.
fn animation_for(state: State) -> &'static str {
    match state {
        State::Grounded => "idle",
        State::Falling | State::Dragged => "fall",
        State::Perched => "sit",
        // An optional Animation: a Character without climb art draws its walk
        // instead, resolved by the renderer, never by a missing sprite here.
        State::Climbing => "climb",
        State::Asleep => "sleep",
    }
}

fn thrown_velocity(snapshot: &WorldSnapshot) -> Option<Point> {
    snapshot.verbs.iter().find_map(|verb| match verb {
        Verb::Throw { velocity } => Some(*velocity),
        _ => None,
    })
}

/// What a fresh window sample says about the Perch the sprite was standing on.
enum PerchCarry {
    /// Dragged slowly: snap onto the new edge and coast until the next sample.
    Ride(Window),
    /// Same edge as last time: snap back if a coast overshot, then sit.
    Still(Window),
    /// Dragged hard enough to lose footing.
    Yank,
    /// Closed, minimized, or no longer somewhere the sprite can stand.
    Lost,
}

/// A surface the sprite can come to rest on.
struct Support {
    y: f64,
    surface: Surface,
}

/// What a Support is made of. The fact geometry reports — what standing on
/// one turns the sprite into is `transition`'s to say, not geometry's.
#[derive(Clone, Copy)]
enum Surface {
    /// The bottom of a display.
    Floor,
    /// A window's top edge.
    Perch,
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
        surface: Surface::Floor,
    });

    snapshot
        .windows
        .iter()
        .enumerate()
        .filter(|(index, window)| {
            window.rect.y >= position.y && is_perch(*index, position.x, snapshot)
        })
        .map(|(_, window)| Support {
            y: window.rect.y,
            surface: Surface::Perch,
        })
        .chain(floor)
        .min_by(|a, b| a.y.total_cmp(&b.y))
}

/// Whether the top edge of the window at `index` is somewhere the sprite can
/// stand at `x`.
///
/// Nearest support still wins over frontmost, so this only narrows the
/// candidates. Walking the windows in order and taking the first match would
/// instead have the sprite fall through an edge it can plainly see, whenever
/// the window in front of that edge is above the sprite and so no support at
/// all.
///
/// Three ways an edge is not a Perch, all of them a resting place the user
/// cannot see:
///
/// - Hidden behind a window drawn in front of it. That is what makes
///   `snapshot.windows` an ordered list rather than a set: an edge covered at
///   this x is not there to be seen, and the sprite resting on it is drawn
///   floating in the middle of the window that hides it. An edge still visible
///   at this x is a Perch whatever its depth.
/// - Over no display. Physics clamps to the union of the display frames and
///   not to the rectangle bounding them, so a window spanning two displays of
///   different heights hangs part of its edge over nothing. A sprite resting
///   out there is invisible and unclickable until the window moves.
/// - Too close to the usable top. The feet are the contact point and the art
///   hangs above them, so a title bar under the menu bar leaves only legs on
///   screen. #100.
fn is_perch(index: usize, x: f64, snapshot: &WorldSnapshot) -> bool {
    let window = &snapshot.windows[index].rect;
    window.spans_x(x)
        && displays_spanning(x, snapshot).any(|display| {
            window.y >= display.y + CEILING_CLEARANCE && window.y <= display.bottom()
        })
        && !snapshot.windows[..index].iter().any(|front| {
            front.rect.spans_x(x) && window.y >= front.rect.y && window.y <= front.rect.bottom()
        })
}

/// What a resting sprite is standing on: its Perch, unless a window in front
/// of that Perch has come to contain it — `swallowing` being the caller's
/// judgement of which windows those are, since it takes remembering the
/// previous tick to make. See `Engine::swallowed_by`.
///
/// A window dragged over the sprite, or walked into where it overlaps a lower
/// Perch, would otherwise leave it inside a rectangle rather than on anything.
/// Its top edge is the surface instead, which is also why two overlapping
/// windows settle on one Perch: the topmost edge wins, and it wins again next
/// tick.
///
/// A sprite on the floor is exempt. The floor is under every window and the
/// sprite is drawn in front of them all, so standing on the ground in front of
/// a window is not the trapped-inside case DESIGN.md decision 7 is about — and
/// every window hanging below the usable floor, as anything behind the Dock
/// does, contains the ground the sprite stands on.
fn footing(
    position: Point,
    snapshot: &WorldSnapshot,
    swallowing: impl Fn(&Window) -> bool,
) -> Option<Support> {
    if floor_under(position.x, snapshot) == Some(position.y) {
        return support_below(position, snapshot);
    }

    // The Perch it is on: a window whose top edge is exactly its own height.
    // Without one the sprite is standing on nothing and falls. A Perch that
    // moves out from under it leaves it in the air, and not on the first window
    // that happens to contain it. #78.
    let perch = snapshot
        .windows
        .iter()
        .position(|window| window.rect.spans_x(position.x) && window.rect.y == position.y)?;

    // Only a window in front of the Perch can swallow the sprite, which is why
    // the candidates stop at the Perch's own place in the order: what is
    // behind the Perch is behind the sprite too, so the edge it stands on is
    // still there to be seen.
    //
    // An edge you cannot see is gone, including one the sprite is already
    // standing on. Keeping a hidden Perch left it floating in mid-air
    // after alt-tab. A window that already contained the sprite still
    // swallows nothing, so this falls rather than hoisting it under the
    // menu bar. #100, and #78 still holds.
    let held = (is_perch(perch, position.x, snapshot)
        && !snapshot.windows[..perch].iter().any(&swallowing))
    .then_some(Support {
        y: position.y,
        surface: Surface::Perch,
    });

    snapshot.windows[..perch]
        .iter()
        .enumerate()
        // Swallowed only onto a top edge that is somewhere to stand: lifting
        // the sprite onto an edge that is hidden or off-screen strands it in
        // the place this is meant to get it out of.
        .filter(|(index, window)| swallowing(window) && is_perch(*index, position.x, snapshot))
        .map(|(_, window)| Support {
            y: window.rect.y,
            surface: Surface::Perch,
        })
        .chain(held)
        .chain(support_below(position, snapshot))
        .min_by(|a, b| a.y.total_cmp(&b.y))
}

/// The window whose top edge the sprite is standing on, if any.
fn perch_at(position: Point, windows: &[Window]) -> Option<Window> {
    windows
        .iter()
        .copied()
        .find(|window| window.rect.spans_x(position.x) && window.rect.y == position.y)
}

/// Whether the sprite is inside `window` rather than on top of it. A top edge
/// is a Perch to stand on, so only what is strictly below it is inside.
fn swallows(window: &Rect, position: Point) -> bool {
    window.spans_x(position.x) && window.y < position.y && position.y < window.bottom()
}

/// The bottom of the display the sprite is over, or nothing when it is over no
/// display at all — a sprite outside every display has nothing to land on.
fn floor_under(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    displays_spanning(x, snapshot)
        .map(Rect::bottom)
        .max_by(f64::total_cmp)
}

/// The highest the feet may go: the usable top plus the room the art needs
/// above them. A climb lets go here; a Throw bumps it. #100.
fn ceiling_over(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    displays_spanning(x, snapshot)
        .map(|display| display.y + CEILING_CLEARANCE)
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
/// nearest one when it is over no display at all. A sprite out there has no
/// floor under it and would otherwise fall for ever, so grabbing an edge is how
/// it gets back over a display.
///
/// "Over no display" is the union of the display frames, not the rectangle that
/// bounds them. Two displays side by side with a gap between them bound a
/// region the sprite can occupy and no screen covers, and letting go of it
/// there leaves it with no sideways speed to carry it out — it simply falls,
/// unseen, for as long as the app runs. A thrown sprite crosses such a gap
/// before gravity matters, which is why only a drop finds this.
fn wall_reached(x: f64, velocity_x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    if displays_spanning(x, snapshot).next().is_none() {
        return nearest_edge(x, snapshot);
    }

    // Still over a display, so the only wall left is the outermost edge, and
    // only while the sprite is moving into it. Every inner edge is somewhere to
    // cross rather than something to catch.
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

    if velocity_x > 0.0 && x >= right {
        Some(right)
    } else if velocity_x < 0.0 && x <= left {
        Some(left)
    } else {
        None
    }
}

/// The display edge nearest `x`, for a sprite that is over none of them.
///
/// Nearest rather than the one it came from: the sprite has no memory of that,
/// and the shortest way back to somewhere it can stand is the least surprising
/// place for it to reappear.
fn nearest_edge(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    snapshot
        .displays
        .iter()
        .flat_map(|display| [display.x, display.x + display.width])
        .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::DEFAULT_WEIGHT;

    /// One 1000x800 display with its top-left at the origin.
    fn one_display() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
        }
    }

    /// A window in a snapshot. Ids only have to differ, except in the tests
    /// that are about identity, which say so.
    fn window(id: WindowId, rect: Rect) -> Window {
        Window { id, rect }
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
            windows: vec![window(
                1,
                Rect {
                    x: 50.0,
                    y: 400.0,
                    width: 300.0,
                    height: 200.0,
                },
            )],
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

    /// The Behaviors the Blip Character declares: a greeting that chains
    /// into settling down, and a stroll.
    fn declared_behaviors() -> BTreeMap<String, Behavior> {
        BTreeMap::from([
            (
                "walk".to_string(),
                Behavior {
                    primitives: vec![Primitive::Walk],
                    then: None,
                    weight: DEFAULT_WEIGHT,
                    trigger: None,
                },
            ),
            (
                "greet".to_string(),
                Behavior {
                    primitives: vec![Primitive::React, Primitive::Talk],
                    then: Some("settle".to_string()),
                    weight: DEFAULT_WEIGHT,
                    trigger: None,
                },
            ),
            (
                "settle".to_string(),
                Behavior {
                    primitives: vec![Primitive::Sit, Primitive::Sleep],
                    then: None,
                    weight: DEFAULT_WEIGHT,
                    trigger: None,
                },
            ),
        ])
    }

    /// A sprite dropped at `position` with those Behaviors to play.
    fn a_character_at(position: Point) -> Engine {
        Engine::new(position).with_behaviors(declared_behaviors())
    }

    /// A sprite standing on the floor of one display, with those Behaviors to
    /// play.
    fn a_resting_sprite() -> Engine {
        let mut engine = a_character_at(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &snapshot(100));
        engine
    }

    fn proposing(behavior: &str) -> WorldSnapshot {
        WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: behavior.to_string(),
                dialogue: None,
            }),
            ..snapshot(100)
        }
    }

    /// What plays over `ticks` further 100ms ticks, as each Animation and the
    /// number of ticks it held the screen for.
    fn played(engine: &mut Engine, ticks: usize) -> Vec<(&'static str, usize)> {
        let mut run: Vec<(&'static str, usize)> = Vec::new();
        for _ in 0..ticks {
            let animation = engine.tick(&snapshot(100)).animation;
            match run.last_mut() {
                Some((last, count)) if *last == animation => *count += 1,
                _ => run.push((animation, 1)),
            }
        }
        run
    }

    /// #8's first criterion. `greet` is two Primitives and then the two of
    /// `settle`, so a chain is played as one Behavior rather than stopping at
    /// the word that joins them.
    #[test]
    fn a_behavior_plays_its_primitives_in_order_and_follows_the_one_it_names() {
        let mut engine = a_resting_sprite();

        assert_eq!(
            engine.tick(&proposing("greet")).animation,
            "react",
            "the proposal is applied on the tick it arrives"
        );

        assert_eq!(
            played(&mut engine, 29),
            [
                ("react", 5),
                ("talk", 6),
                ("sit", 6),
                ("sleep", 6),
                ("idle", 6),
            ],
            "each Primitive holds the screen for as long as the next, \
             and the sprite goes back to idling when the Behavior ends"
        );
    }

    /// #8: `land` is the last of the nine required Animations nothing could
    /// reach. Landing is not a State — the sprite is standing the moment it
    /// arrives — so the end of a fall is played as a Primitive over the
    /// standing, the same as any other Behavior.
    #[test]
    fn a_fall_ends_in_the_landing_animation_before_the_sprite_idles() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let landed = (0..40)
            .map(|_| engine.tick(&snapshot(100)))
            .find(|frame| frame.state == State::Grounded)
            .expect("a sprite dropped over a display lands on it");
        assert_eq!(landed.animation, "land");

        assert_eq!(
            played(&mut engine, 10),
            [("land", 5), ("idle", 5)],
            "and the landing gives way to idling rather than holding"
        );
    }

    /// #8's second criterion. The window it was sitting on closes mid-Behavior,
    /// and sitting in mid-air is not a thing the sprite can be doing.
    #[test]
    fn a_behavior_that_becomes_invalid_mid_play_is_abandoned() {
        let window = window(
            1,
            Rect {
                x: 50.0,
                y: 400.0,
                width: 300.0,
                height: 200.0,
            },
        );
        let on_a_window = WorldSnapshot {
            windows: vec![window],
            ..snapshot(100)
        };

        let mut engine = a_character_at(Point { x: 100.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &on_a_window).state, State::Perched);

        assert_eq!(
            engine
                .tick(&WorldSnapshot {
                    windows: vec![window],
                    ..proposing("settle")
                })
                .animation,
            "sit"
        );

        let fell = engine.tick(&snapshot(100));
        assert_eq!(fell.state, State::Falling);
        assert_eq!(fell.animation, "fall", "it stops sitting the moment it can");

        let after = played(&mut engine, 40);
        assert!(
            !after
                .iter()
                .any(|(animation, _)| ["sit", "sleep"].contains(animation)),
            "and the rest of the Behavior never comes back: {after:?}"
        );
    }

    /// #8's third criterion. Refused rather than deferred: a Behavior proposed
    /// for a sprite in mid-air was proposed for a sprite that no longer exists
    /// by the time it lands.
    #[test]
    fn a_behavior_the_state_forbids_is_refused() {
        let mut engine = a_character_at(Point { x: 100.0, y: 0.0 });
        let falling = engine.tick(&proposing("settle"));

        assert_eq!(falling.state, State::Falling);
        assert_eq!(falling.animation, "fall", "it goes on falling instead");

        let after = played(&mut engine, 40);
        assert!(
            !after
                .iter()
                .any(|(animation, _)| ["sit", "sleep"].contains(animation)),
            "and landing does not start what was refused: {after:?}"
        );
    }

    /// The rest of #8's third criterion. Falling is only one of the four States
    /// that are not standing on something, and the gate is one rule for all of
    /// them: asleep is a thing to be woken out of rather than acted from, and a
    /// sprite hauling itself up a screen edge has no more floor than one in
    /// mid-air. `settle` opens on `sit`, which neither State draws by itself,
    /// so the Animation says whether the gate let it through.
    #[test]
    fn a_behavior_that_settles_is_refused_asleep_and_mid_climb() {
        let mut engine = a_resting_sprite();
        assert_eq!(engine.tick(&snapshot(60_000)).state, State::Asleep);

        let asleep = engine.tick(&proposing("settle"));
        assert_eq!(asleep.state, State::Asleep);
        assert_eq!(asleep.animation, "sleep", "it stays asleep instead");

        let mut engine = a_character_at(Point { x: 900.0, y: 400.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        assert_eq!(
            engine
                .tick(&WorldSnapshot {
                    verbs: vec![Verb::Throw {
                        velocity: Point { x: 2000.0, y: 0.0 },
                    }],
                    ..snapshot(100)
                })
                .state,
            State::Climbing
        );

        let climbing = engine.tick(&proposing("settle"));
        assert_eq!(climbing.state, State::Climbing);
        assert_eq!(climbing.animation, "climb", "it goes on climbing");
    }

    /// Expression is the exception: being startled or speaking says nothing
    /// about where the sprite's feet are, so a Poke is answered mid-fall and so
    /// is a Behavior made only of those.
    #[test]
    fn a_behavior_of_expression_alone_plays_in_any_state() {
        let mut engine =
            Engine::new(Point { x: 100.0, y: 0.0 }).with_behaviors(BTreeMap::from([(
                "chatter".to_string(),
                Behavior {
                    primitives: vec![Primitive::Talk],
                    then: None,
                    weight: DEFAULT_WEIGHT,
                    trigger: None,
                },
            )]));

        let falling = engine.tick(&proposing("chatter"));
        assert_eq!(falling.state, State::Falling);
        assert_eq!(falling.animation, "talk");
    }

    /// Load-time validation rejects a chain that comes back on itself, so this
    /// is the second lock on the same door: the Engine is handed Behaviors
    /// rather than a validated Character, and a Behavior that could hang the
    /// frame loop is the one thing ADR-0002 promises no package can be.
    #[test]
    fn a_behavior_that_chains_back_to_itself_still_ends() {
        let pacing = |then: &str| Behavior {
            primitives: vec![Primitive::Idle],
            then: Some(then.to_string()),
            weight: DEFAULT_WEIGHT,
            trigger: None,
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 }).with_behaviors(BTreeMap::from([
            ("here".to_string(), pacing("there")),
            ("there".to_string(), pacing("here")),
        ]));
        settle(&mut engine, &snapshot(100));

        assert_eq!(engine.tick(&proposing("here")).animation, "idle");
        assert_eq!(
            played(&mut engine, 20),
            [("idle", 20)],
            "each Behavior of the loop is played once and the sprite goes back \
             to idling, rather than the tick never returning"
        );
    }

    /// A proposal the Character cannot play is refused, and refusing it is not
    /// an interruption: the Director names a Behavior, and a Character that was
    /// swapped or is simply older may not declare it.
    #[test]
    fn an_unknown_behavior_is_refused_without_disrupting_what_is_playing() {
        let mut engine = a_resting_sprite();
        engine.tick(&proposing("greet"));

        let unknown = engine.tick(&proposing("cartwheel"));
        assert_eq!(unknown.animation, "react", "still greeting");
        assert_eq!(
            unknown.animation_ms, 100,
            "and on its own clock, not restarted by the refusal"
        );
    }

    /// #10 suppresses Behaviors the user has recently *seen*, and the Shell
    /// keeps that list. A proposal is advisory, so what was proposed and what
    /// was played are different lists, and only the Engine knows the second.
    #[test]
    fn a_frame_names_the_behavior_that_started_and_not_one_that_was_refused() {
        let mut engine =
            Engine::new(Point { x: 100.0, y: 0.0 }).with_behaviors(declared_behaviors());

        let airborne = engine.tick(&proposing("settle"));
        assert_eq!(airborne.state, State::Falling);
        assert_eq!(
            airborne.behavior, None,
            "there is no sitting down in mid-air, so nobody saw it"
        );

        settle(&mut engine, &snapshot(100));
        assert_eq!(
            engine.tick(&proposing("settle")).behavior.as_deref(),
            Some("settle"),
            "on the floor it plays, and the Shell may remember it"
        );
        assert_eq!(
            engine.tick(&snapshot(100)).behavior,
            None,
            "starting is an event, not a state to hold"
        );
        assert_eq!(
            engine.tick(&proposing("cartwheel")).behavior,
            None,
            "nor does a Behavior nobody declares count as played"
        );
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
        assert_eq!(restarted.animation, "land", "it landed in that first tick");
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
            highest, 158.0,
            "it lets go with the art still on the usable frame, not at \
             the display's own top of 0"
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

    /// #6: a Grab overrides any State. The hand is the one input that outranks
    /// everything else the sprite might be doing — asleep, perched, halfway up
    /// a screen edge — because a companion you cannot pick up whenever you like
    /// is furniture.
    #[test]
    fn a_grab_takes_the_sprite_out_of_whatever_state_it_was_in() {
        let day = a_day_in_the_life();

        for state in [
            State::Grounded,
            State::Falling,
            State::Dragged,
            State::Perched,
            State::Climbing,
            State::Asleep,
        ] {
            // Replay the day only as far as the first tick in this State, so
            // the Grab lands on a sprite that is genuinely in it.
            let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
            assert!(
                day.iter().any(|s| engine.tick(s).state == state),
                "{state:?} is never reached, so the Grab is untested from it"
            );

            let grabbed = engine.tick(&WorldSnapshot {
                cursor: Point { x: 640.0, y: 360.0 },
                verbs: vec![Verb::Grab],
                ..snapshot(100)
            });

            assert_eq!(grabbed.state, State::Dragged, "grabbed while {state:?}");
            assert_eq!(grabbed.position, Point { x: 640.0, y: 360.0 });
        }
    }

    /// #6 fixes the verb set at five so no Character ever has to grow another
    /// Animation for a sixth. Summon opens the chat surface (#17) and Menu opens
    /// the tray's menu (#18); neither exists, so both are accepted and show
    /// nothing. They are still the user reaching for the sprite, so a sleeping
    /// one wakes — an interaction that left it snoring would read as ignored.
    #[test]
    fn a_summon_or_a_menu_wakes_the_sprite_without_moving_it() {
        for verb in [Verb::Summon, Verb::Menu] {
            let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
            let resting = settle(&mut engine, &snapshot(100));
            assert_eq!(resting.state, State::Grounded);

            // A full minute of nobody touching it.
            assert_eq!(engine.tick(&snapshot(60_000)).state, State::Asleep);

            let addressed = engine.tick(&WorldSnapshot {
                cursor: Point { x: 900.0, y: 700.0 },
                verbs: vec![verb],
                ..snapshot(100)
            });

            assert_eq!(addressed.state, State::Grounded, "awake, after {verb:?}");
            assert_eq!(
                addressed.position, resting.position,
                "{verb:?} does not walk the sprite to the cursor"
            );
            assert_eq!(addressed.animation, "idle", "and nothing is played for it");
        }
    }

    /// The other side of the wake that plays nothing: a wake is not an arrival
    /// only because the sprite is put straight back on the footing it fell
    /// asleep on. Woken with that footing gone, it is in the air like anything
    /// else, and the landing at the end of the fall is a real one.
    #[test]
    fn a_sprite_woken_with_its_perch_gone_still_lands() {
        let perch = window(
            2,
            Rect {
                x: 0.0,
                y: 400.0,
                width: 1000.0,
                height: 200.0,
            },
        );
        let resting = WorldSnapshot {
            windows: vec![perch],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &resting).state, State::Perched);
        assert_eq!(
            engine
                .tick(&WorldSnapshot {
                    elapsed_ms: 60_000,
                    ..resting
                })
                .state,
            State::Asleep
        );

        // Woken by a Summon in the tick the window it stood on goes.
        let woken = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Summon],
            ..snapshot(100)
        });
        assert_eq!(woken.state, State::Falling, "there is nothing under it");
        assert_ne!(woken.animation, "land", "it has not arrived anywhere yet");

        let arrived = (0..40)
            .map(|_| engine.tick(&snapshot(100)))
            .find(|frame| frame.state == State::Grounded)
            .expect("it reaches the floor");
        assert_eq!(
            arrived.animation, "land",
            "and the floor it reaches is an arrival like any other"
        );
    }

    /// #6: a Director proposal arriving during a Grab is deferred or dropped,
    /// never yanking the sprite. Being held is the one moment the sprite is the
    /// user's rather than the Director's.
    ///
    /// `settle` is a Behavior this Character does declare, so what refuses it
    /// is the State gate and not the name: a proposal nobody declares is
    /// refused on the way in, and would leave this rule unguarded.
    #[test]
    fn a_proposal_during_a_grab_never_moves_the_sprite() {
        let mut engine = a_character_at(Point { x: 500.0, y: 100.0 });
        let held = WorldSnapshot {
            cursor: Point { x: 300.0, y: 300.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        };
        engine.tick(&held);

        let proposed = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "settle".to_string(),
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
            proposed.animation, "fall",
            "it dangles from the cursor rather than sitting down in mid-air"
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

    /// A window wide enough to walk along, with its top edge at y=400.
    fn a_long_perch() -> WorldSnapshot {
        WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x: 100.0,
                    y: 400.0,
                    width: 800.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        }
    }

    /// The Director asking for a walk, by the name the Blip Character
    /// declares for a Behavior of one `walk` Primitive.
    fn walk() -> Option<BehaviorProposal> {
        Some(BehaviorProposal {
            behavior: "walk".to_string(),
            dialogue: None,
        })
    }

    /// The art has one heading, so the renderer mirrors it by `facing`. Only
    /// travel may turn the sprite: a stop, a doze, a straight fall all keep
    /// the last heading, or it would spin on the spot at rest.
    #[test]
    fn the_sprite_faces_the_way_it_travels_and_keeps_it_at_rest() {
        let mut engine = a_character_at(Point { x: 400.0, y: 0.0 });
        let perched = settle(&mut engine, &a_long_perch());
        assert_eq!(perched.facing, 1.0, "untraveled, it points right");

        // Thrown leftwards onto the Perch.
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 800.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..a_long_perch()
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: -300.0, y: 0.0 },
            }],
            ..a_long_perch()
        });
        let flying = engine.tick(&a_long_perch());
        assert_eq!(flying.facing, -1.0, "it flies the way it was thrown");

        let at_rest = settle(&mut engine, &a_long_perch());
        assert_eq!(at_rest.velocity.x, 0.0);
        assert_eq!(
            at_rest.facing, -1.0,
            "and coming to rest keeps the heading: {at_rest:?}"
        );

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let strolling = engine.tick(&a_long_perch());
        assert_eq!(
            strolling.velocity.x, -WALK_SPEED,
            "it walks the way it faces"
        );
        assert_eq!(strolling.facing, -1.0);
    }

    /// The Primitive is what walks, not the name over it. A Character is free to
    /// call a stroll anything, and a Director that proposes one gets a sprite
    /// that moves — which is what makes `walk` a Primitive rather than a name
    /// the Engine keeps a list of.
    #[test]
    fn a_walk_is_the_primitive_that_reaches_the_screen_and_not_the_behaviors_name() {
        let mut engine = Engine::new(Point { x: 200.0, y: 0.0 }).with_behaviors(BTreeMap::from([
            (
                // Opens on a Primitive that stands still, so a sprite that
                // moves before the walk comes up is one moved by the name.
                "amble".to_string(),
                Behavior {
                    primitives: vec![Primitive::React, Primitive::Walk],
                    then: None,
                    weight: DEFAULT_WEIGHT,
                    trigger: None,
                },
            ),
            (
                // Named for the walk it is not: a Character that declares this
                // has declared sitting down, whatever the Director reads into
                // the name.
                "walk".to_string(),
                Behavior {
                    primitives: vec![Primitive::Sit],
                    then: None,
                    weight: DEFAULT_WEIGHT,
                    trigger: None,
                },
            ),
        ]));
        settle(&mut engine, &a_long_perch());

        let reacting = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "amble".to_string(),
                dialogue: None,
            }),
            ..a_long_perch()
        });
        assert_eq!(reacting.animation, "react");
        assert_eq!(
            reacting.position.x, 200.0,
            "the Behavior opens on a Primitive that does not move it"
        );

        // `react` holds the screen for six of these ticks, so the eighth is
        // inside the walk's own turn rather than the hold-over after it.
        let strolling = (0..8).map(|_| engine.tick(&a_long_perch())).last().unwrap();
        assert_eq!(strolling.animation, "walk");
        assert!(
            strolling.position.x > 200.0,
            "and sets off once the walk comes up: {strolling:?}"
        );

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let sitting = engine.tick(&a_long_perch());
        assert_eq!(sitting.animation, "sit");
        assert_eq!(
            sitting.velocity.x, 0.0,
            "a Behavior called walk that does not walk stops the one under way"
        );
    }

    /// A proposal is gated on the State the tick ends in, not the one it opened
    /// with. A sprite woken by a Summon is put back on its feet by falling, so
    /// reading the State before that is asking a sprite that is standing on the
    /// floor whether it is in mid-air.
    #[test]
    fn a_proposal_is_gated_on_the_state_the_tick_ends_in() {
        let mut engine = a_resting_sprite();
        assert_eq!(engine.tick(&snapshot(60_000)).state, State::Asleep);

        let woken = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Summon],
            proposal: Some(BehaviorProposal {
                behavior: "greet".to_string(),
                dialogue: None,
            }),
            ..snapshot(100)
        });
        assert_eq!(woken.state, State::Grounded);
        assert_eq!(
            woken.animation, "react",
            "the greeting is played: {woken:?}"
        );
    }

    /// The other half of that rule: a Primitive that is not the sprite standing
    /// still leaves the walk running. A Poke interrupts what the sprite is
    /// doing, not where it is going, so the stroll it was on carries the
    /// reaction along rather than ending under it.
    #[test]
    fn a_poke_mid_stroll_is_reacted_to_without_stopping_the_walk() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let under_way = engine.tick(&a_long_perch());
        assert_eq!(under_way.velocity.x, WALK_SPEED);

        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..a_long_perch()
        });
        assert_eq!(poked.animation, "react");

        let strolling: Vec<Frame> = (0..12).map(|_| engine.tick(&a_long_perch())).collect();
        assert!(
            strolling.iter().all(|frame| frame.velocity.x == WALK_SPEED),
            "it goes on walking through the reaction and out the other side: {strolling:?}"
        );
    }

    #[test]
    fn the_sprite_walks_along_a_window_top_edge() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        let perched = settle(&mut engine, &a_long_perch());
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.x, 200.0, "it landed where it fell");

        let told = engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        assert_eq!(told.animation, "walk");

        // The tick after, because the Behavior is played once this tick's State
        // is settled and the sprite has already been moved by then. SPEC.md
        // asks only that a valid proposal be applied on the next tick.
        let setting_off = engine.tick(&a_long_perch());
        assert!(
            setting_off.position.x > 200.0,
            "it sets off: {setting_off:?}"
        );

        let carrying_on = engine.tick(&a_long_perch());
        assert_eq!(carrying_on.state, State::Perched, "still on the edge");
        assert_eq!(carrying_on.position.y, 400.0, "and at its height");
        assert!(
            carrying_on.position.x > setting_off.position.x,
            "and keeps going without being told again: {carrying_on:?}"
        );
    }

    /// Both ends, because a walk that only ever goes one way would leave the
    /// other end untested. Which way it goes is the way it was already
    /// heading, so the throw that puts it on the Perch also aims the walk.
    #[test]
    fn the_sprite_walks_off_either_end_of_a_perch() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());

        let off_the_right = walked_off(&mut engine);
        assert!(
            off_the_right.position.x > 900.0,
            "past the window's right edge: {off_the_right:?}"
        );
        assert_eq!(off_the_right.state, State::Grounded);
        assert_eq!(off_the_right.position.y, 800.0, "down on the floor");

        // Thrown back onto the Perch leftwards, so it walks off the other end.
        let mut engine = a_character_at(Point { x: 800.0, y: 100.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 800.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..a_long_perch()
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: -300.0, y: 0.0 },
            }],
            ..a_long_perch()
        });
        let perched = settle(&mut engine, &a_long_perch());
        assert_eq!(perched.state, State::Perched, "back on it: {perched:?}");

        let off_the_left = walked_off(&mut engine);
        assert!(
            off_the_left.position.x < 100.0,
            "past the window's left edge: {off_the_left:?}"
        );
        assert_eq!(off_the_left.state, State::Grounded);
        assert_eq!(off_the_left.position.y, 800.0);
    }

    /// Told to walk once, then left alone until it comes to rest again.
    fn walked_off(engine: &mut Engine) -> Frame {
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        (0..200)
            .map(|_| engine.tick(&a_long_perch()))
            .last()
            .expect("two hundred ticks produce two hundred frames")
    }

    /// #5: a walk under way keeps the sprite awake. Nodding off is for a sprite
    /// that has been left alone, and a Director prodding an idle sprite into a
    /// walk is exactly when the sleep timer is about to come due.
    #[test]
    fn a_walking_sprite_does_not_nod_off_mid_stride() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());

        // Poked, then left alone until it is one tick short of nodding off.
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..a_long_perch()
        });
        let nearly_asleep = engine.tick(&WorldSnapshot {
            elapsed_ms: SLEEP_AFTER_MS - 100,
            ..a_long_perch()
        });
        assert_eq!(nearly_asleep.state, State::Perched, "not asleep yet");

        // The tick the timer comes due is the tick it sets off walking.
        let setting_off = engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let strolling: Vec<Frame> = (0..20).map(|_| engine.tick(&a_long_perch())).collect();

        assert_eq!(setting_off.animation, "walk", "{setting_off:?}");
        assert!(
            strolling
                .iter()
                .all(|frame| frame.state == State::Perched && frame.animation == "walk"),
            "it walks the edge awake rather than sleeping its way along it: {strolling:?}"
        );
    }

    /// #5, and DESIGN.md decision 7: the bad case is a sprite trapped inside an
    /// occluded window, not a sprite standing on the ground in front of one.
    /// Windows routinely hang below the usable floor — anything behind the Dock
    /// does — and the floor is under everything, so being within one is the
    /// normal state of a sprite standing on the ground.
    #[test]
    fn a_window_over_the_floor_leaves_the_sprite_standing_on_it() {
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });
        let grounded = settle(&mut engine, &snapshot(100));
        assert_eq!(grounded.state, State::Grounded);
        assert_eq!(grounded.position.y, 800.0, "the usable floor");

        // A window is dragged over it, hanging below the usable floor.
        let covered = WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x: 0.0,
                    y: 100.0,
                    width: 1000.0,
                    height: 800.0,
                },
            )],
            ..snapshot(100)
        };
        let frames: Vec<Frame> = (0..10).map(|_| engine.tick(&covered)).collect();
        assert!(
            frames
                .iter()
                .all(|frame| frame.position == grounded.position && frame.state == State::Grounded),
            "it stays on the ground rather than flying up to the title bar: {frames:?}"
        );
    }

    /// The other half of what makes a window the sprite's footing: it has to
    /// have come to contain it. A window floating clear above the Perch is not
    /// something the sprite is inside, so it is not something to be lifted onto.
    #[test]
    fn a_window_floating_above_the_perch_is_not_footing() {
        let world = || WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: 0.0,
                        y: 100.0,
                        width: 1000.0,
                        height: 150.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: 0.0,
                        y: 400.0,
                        width: 1000.0,
                        height: 200.0,
                    },
                ),
            ],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 300.0 });

        let perched = settle(&mut engine, &world());
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0, "the edge below it");

        let frames: Vec<Frame> = (0..10).map(|_| engine.tick(&world())).collect();
        assert!(
            frames.iter().all(|frame| frame.position.y == 400.0),
            "and it stays there rather than being hoisted to the one above: {frames:?}"
        );
    }

    #[test]
    fn the_sprite_perches_on_a_window_top_edge_and_falls_when_the_window_goes() {
        let window = WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x: 50.0,
                    y: 400.0,
                    width: 300.0,
                    height: 200.0,
                },
            )],
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

    /// The sibling of the window closing: the window is still there, it has
    /// simply been dragged elsewhere. Nothing but its position changes, so a
    /// rule written against the window list rather than against what is under
    /// the sprite would keep it standing on thin air.
    #[test]
    fn the_sprite_falls_when_its_perch_moves_out_from_under_it() {
        let window = |x: f64| WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x,
                    y: 400.0,
                    width: 300.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let perched = settle(&mut engine, &window(50.0));
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0);

        // The same window, yanked out from under it rather than closed. #98.
        let dropped = engine.tick(&window(600.0));
        assert_eq!(dropped.state, State::Falling);

        let landed = settle(&mut engine, &window(600.0));
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0, "down to the floor it left");
    }

    /// A window the sprite can stand on, moved without changing size.
    fn perch(x: f64, y: f64) -> WorldSnapshot {
        WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x,
                    y,
                    width: 300.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        }
    }

    /// #98: a Perch dragged slowly is still underfoot. The sprite keeps the
    /// place it had on the edge rather than falling through the window that
    /// now contains it.
    #[test]
    fn the_sprite_rides_a_slowly_dragged_perch() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        let perched = settle(&mut engine, &perch(50.0, 400.0));
        assert_eq!(perched.state, State::Perched);
        let offset_x = perched.position.x - 50.0;

        let up = engine.tick(&perch(50.0, 380.0));
        assert_eq!(up.state, State::Perched, "up with the window: {up:?}");
        assert_eq!(
            up.position,
            Point {
                x: perched.position.x,
                y: 380.0
            }
        );
        assert_eq!(up.animation, "hold");
        assert!(up.riding, "the Shell polls fast only while this is set");

        let across = engine.tick(&perch(70.0, 380.0));
        assert_eq!(across.state, State::Perched, "sideways: {across:?}");
        assert_eq!(
            across.position,
            Point {
                x: 70.0 + offset_x,
                y: 380.0
            }
        );
        assert_eq!(across.animation, "hold");

        let down = engine.tick(&perch(70.0, 400.0));
        assert_eq!(down.state, State::Perched, "down with the window: {down:?}");
        assert_eq!(
            down.position,
            Point {
                x: 70.0 + offset_x,
                y: 400.0
            }
        );
        assert_eq!(down.animation, "hold");
    }

    /// #98: window geometry is reused between polls. The sprite has to keep
    /// the last Perch velocity on those ticks, or it hitch-steps at the poll
    /// rate while the window itself slides every frame.
    #[test]
    fn the_sprite_coasts_with_the_perch_between_polls() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));

        let mut moving = perch(50.0, 380.0);
        moving.poll_generation = 1;
        let caught_up = engine.tick(&moving);
        assert_eq!(caught_up.position.y, 380.0);
        assert_eq!(caught_up.animation, "hold");

        // Same rectangle, same generation: the assembler has not read again.
        // 20 points in 100 ms is 200 points/s, so 16 ms is 3.2 points further.
        moving.elapsed_ms = 16;
        let coasting = engine.tick(&moving);
        assert_eq!(coasting.state, State::Perched, "{coasting:?}");
        assert_eq!(coasting.animation, "hold");
        assert!(
            (coasting.position.y - 376.8).abs() < 1e-9,
            "it continues at the Perch's last speed, not waiting for the next poll: {coasting:?}"
        );
    }

    /// #98: a Perch that is speeding up is not at constant velocity between
    /// polls. Coasting with the last acceleration keeps the sprite on the
    /// window instead of hitching every time a new sample snaps it back.
    #[test]
    fn the_sprite_coasts_with_the_perch_acceleration_between_polls() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));

        let mut first = perch(50.0, 390.0);
        first.poll_generation = 1;
        assert_eq!(engine.tick(&first).position.y, 390.0);

        // 10 points, then 20: -100 points/s, then -200. Acceleration is
        // -1000 points/s² — under the yank gate, so it still rides.
        let mut faster = perch(50.0, 370.0);
        faster.poll_generation = 2;
        assert_eq!(engine.tick(&faster).position.y, 370.0);

        faster.elapsed_ms = 16;
        let coasting = engine.tick(&faster);
        assert_eq!(coasting.state, State::Perched, "{coasting:?}");
        assert_eq!(coasting.animation, "hold");
        // v Δt + ½ a Δt² = -200·0.016 + ½·(-1000)·0.016² = -3.328
        assert!(
            (coasting.position.y - 366.672).abs() < 1e-9,
            "it continues at the last speed and acceleration: {coasting:?}"
        );
    }

    /// #98: the ride ends when the Perch is still. Holding on is the motion,
    /// not a new way to sit.
    #[test]
    fn a_still_perch_returns_the_sprite_to_sitting() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));
        assert_eq!(engine.tick(&perch(50.0, 380.0)).animation, "hold");

        let still = engine.tick(&perch(50.0, 380.0));
        assert_eq!(still.state, State::Perched);
        assert_eq!(still.position.y, 380.0);
        assert_eq!(still.animation, "sit");
        assert!(!still.riding);
    }

    /// #85: what the window server's id buys, and geometry could not. The
    /// Perch closes, and a window of the same size — the same app's second
    /// document, say — is sitting a few points away. Matching by size and
    /// displacement called that the same window and slid the sprite onto it.
    /// The ids differ, so the Perch is gone and the sprite falls.
    #[test]
    fn a_perch_that_closes_is_not_the_same_window_as_one_of_its_size_nearby() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        assert_eq!(
            settle(&mut engine, &perch(50.0, 400.0)).state,
            State::Perched
        );

        let twin = WorldSnapshot {
            windows: vec![window(
                2,
                Rect {
                    x: 70.0,
                    y: 380.0,
                    width: 300.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        };

        let dropped = engine.tick(&twin);
        assert_eq!(dropped.state, State::Falling, "{dropped:?}");
        assert_eq!(
            settle(&mut engine, &twin).position.y,
            800.0,
            "and down to the floor, not carried onto a window it never stood on"
        );
    }

    /// #85, the other half: a second window of the same size that ends up
    /// nearer the Perch's old origin than the Perch itself. Geometry had no
    /// way to tell them apart and took the nearer one, which put the sprite on
    /// the wrong window at the wrong offset. The id picks the window it was
    /// actually standing on.
    #[test]
    fn a_twin_window_nearer_the_old_origin_does_not_steal_the_ride() {
        // Frontmost first: the sprite lands on the Perch, and the twin behind
        // it is the same size and overlaps it.
        let twins = |perch_x: f64, twin_x: f64| WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: perch_x,
                        y: 400.0,
                        width: 300.0,
                        height: 200.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: twin_x,
                        y: 400.0,
                        width: 300.0,
                        height: 200.0,
                    },
                ),
            ],
            ..snapshot(100)
        };

        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        let perched = settle(&mut engine, &twins(50.0, 60.0));
        assert_eq!(perched.position.y, 400.0);
        let offset_x = perched.position.x - 50.0;

        // Both are nudged right. The Perch moves 40 points, the twin 10, so
        // the twin is now the nearer match to where the Perch was.
        let ridden = engine.tick(&twins(90.0, 70.0));
        assert_eq!(ridden.state, State::Perched, "{ridden:?}");
        assert_eq!(
            ridden.position,
            Point {
                x: 90.0 + offset_x,
                y: 400.0
            },
            "carried by window 1, not snapped onto window 2"
        );
    }

    /// #85: a resize moves the top edge, so it is a move. Matching the Perch
    /// by size ruled every resize out and dropped the sprite off a window
    /// that was still under it; matching by id carries it, and the grip gate
    /// still governs how fast the edge may go.
    #[test]
    fn a_perch_resized_from_its_top_edge_is_ridden_like_one_that_moved() {
        let sized = |y: f64, height: f64| WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x: 50.0,
                    y,
                    width: 300.0,
                    height,
                },
            )],
            ..snapshot(100)
        };

        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        let perched = settle(&mut engine, &sized(400.0, 200.0));
        assert_eq!(perched.position.y, 400.0);

        // The top border dragged up 20 points: the window grows, the edge
        // moves, and the sprite goes with it.
        let taller = engine.tick(&sized(380.0, 220.0));
        assert_eq!(taller.state, State::Perched, "{taller:?}");
        assert_eq!(taller.position.y, 380.0);
        assert!(taller.riding);

        // The same border yanked 200 points: over the grip, so the sprite is
        // left behind rather than snapped onto the new edge.
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &sized(400.0, 200.0));
        let yanked = engine.tick(&sized(200.0, 400.0));
        assert_eq!(yanked.state, State::Falling, "{yanked:?}");
        assert_eq!(yanked.position.y, 400.0, "left where it stood");
    }

    /// #98: ride poll is 16 ms so the sprite can track, but the yank gate
    /// looks back ~100 ms. Six points in 16 ms after a 200 pt/s ride is
    /// 10_937 pt/s² — over `RIDE_ACCELERATION` — and 175 pt/s against the
    /// speed from a poll ago. Fast poll has to keep tracking; only the
    /// fall decision stays low-pass.
    #[test]
    fn a_short_sample_wobble_does_not_yank() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));
        assert_eq!(engine.tick(&perch(50.0, 380.0)).state, State::Perched);

        let mut wobble = perch(50.0, 374.0);
        wobble.elapsed_ms = 16;
        let riding = engine.tick(&wobble);
        assert_eq!(riding.state, State::Perched, "{riding:?}");
        assert_eq!(riding.position.y, 374.0);
        assert_eq!(riding.animation, "hold");
    }

    /// #98: a yank is a loss of footing even when the window moves up over
    /// the sprite. Lifted is for a *different* window that has come to
    /// contain it, not for the Perch it just lost.
    #[test]
    fn an_upward_yank_drops_the_sprite_rather_than_lifting_it() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));

        // Far enough to exceed the ride gate, near enough that the sprite is
        // still inside the rectangle — the case that used to Lift it. #98.
        let yanked = engine.tick(&perch(50.0, 250.0));
        assert_eq!(yanked.state, State::Falling, "{yanked:?}");
        assert_eq!(
            yanked.position.y, 400.0,
            "it is not carried onto the new edge"
        );
    }

    /// The same drop, with a window arriving over the sprite as its Perch
    /// leaves. Stepping up onto a window is a step up from something: with its
    /// Perch gone the sprite is in the air, and a window that comes over it in
    /// that same tick is not a rescue. #78.
    #[test]
    fn a_perch_that_moves_drops_the_sprite_even_as_a_window_arrives_over_it() {
        // Frontmost first: the window that arrives is in front of the Perch.
        let world = |cover_x: f64, perch_x: f64| WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: cover_x,
                        y: 100.0,
                        width: 900.0,
                        height: 500.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: perch_x,
                        y: 400.0,
                        width: 300.0,
                        height: 200.0,
                    },
                ),
            ],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 150.0 });

        let perched = settle(&mut engine, &world(-1000.0, 50.0));
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0);

        // In one poll: the Perch is dragged away and the other window arrives
        // over where the sprite was standing.
        let dropped = engine.tick(&world(0.0, 600.0));
        assert_eq!(
            dropped.state,
            State::Falling,
            "and not lifted onto the newcomer: {dropped:?}"
        );

        let landed = settle(&mut engine, &world(0.0, 600.0));
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0, "down to the floor it left");
    }

    /// Strictly inside: a top edge is a Perch to stand on, not somewhere the
    /// sprite has been swallowed.
    fn inside_a_window(position: Point, snapshot: &WorldSnapshot) -> bool {
        snapshot.windows.iter().any(|window| {
            position.x > window.rect.x
                && position.x < window.rect.x + window.rect.width
                && position.y > window.rect.y
                && position.y < window.rect.y + window.rect.height
        })
    }

    /// #5: the sprite is never left inside a window rectangle. The overlay is
    /// always on top, so a sprite standing inside one is not hidden by it — it
    /// is drawn floating in the middle of the window, sitting on nothing.
    #[test]
    fn a_window_dragged_over_the_sprite_lifts_it_onto_its_edge() {
        let perch = window(
            2,
            Rect {
                x: 0.0,
                y: 400.0,
                width: 1000.0,
                height: 200.0,
            },
        );
        let resting = WorldSnapshot {
            windows: vec![perch],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &resting).position.y, 400.0);

        // A second window is dragged over it, swallowing the edge it stands on.
        let covered = WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: 200.0,
                        y: 200.0,
                        width: 800.0,
                        height: 400.0,
                    },
                ),
                perch,
            ],
            ..snapshot(100)
        };
        let lifted = engine.tick(&covered);
        assert_eq!(
            lifted.position.y, 200.0,
            "up onto the new top edge: {lifted:?}"
        );
        assert_eq!(lifted.state, State::Perched);
        assert!(!inside_a_window(lifted.position, &covered));
    }

    /// And only onto an edge the sprite can stand on. The window dragged over
    /// it here has its own top edge over no display, so the sprite drops to the
    /// floor instead of being lifted off the screens.
    #[test]
    fn a_window_dragged_over_the_sprite_never_lifts_it_off_the_displays() {
        // Bottom-aligned displays of different heights: nothing covers
        // x 1000..2000 above y=300.
        let world = |windows: Vec<Window>| WorldSnapshot {
            displays: vec![
                one_display(),
                Rect {
                    x: 1000.0,
                    y: 300.0,
                    width: 1000.0,
                    height: 500.0,
                },
            ],
            windows,
            ..snapshot(100)
        };
        let perch = window(
            1,
            Rect {
                x: 1000.0,
                y: 500.0,
                width: 600.0,
                height: 200.0,
            },
        );
        let mut engine = Engine::new(Point {
            x: 1200.0,
            y: 350.0,
        });
        assert_eq!(settle(&mut engine, &world(vec![perch])).position.y, 500.0);

        // Dragged over the sprite, with its top edge up where only the taller
        // display reaches.
        let dragged = window(
            2,
            Rect {
                x: 1100.0,
                y: 100.0,
                width: 800.0,
                height: 500.0,
            },
        );
        let frames: Vec<Frame> = (0..40)
            .map(|_| engine.tick(&world(vec![dragged, perch])))
            .collect();
        assert!(
            frames.iter().all(|frame| frame.position.y >= 300.0),
            "it is never put where no display covers it: {frames:?}"
        );
        let landed = frames.last().expect("forty ticks produce forty frames");
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0, "down to the floor: {landed:?}");
    }

    /// #78, and the other side of the rule above: a window only swallows the
    /// sprite when it is drawn in front of the Perch the sprite stands on. This
    /// one is dragged across the desktop behind that Perch, so the edge under
    /// the sprite stays in plain sight and the sprite stays on it.
    #[test]
    fn a_window_behind_the_perch_does_not_swallow_the_sprite() {
        let perch = window(
            2,
            Rect {
                x: 200.0,
                y: 400.0,
                width: 400.0,
                height: 200.0,
            },
        );
        // Frontmost first: the Perch is in front, the dragged window behind it.
        let world = |x: f64| WorldSnapshot {
            windows: vec![
                perch,
                window(
                    1,
                    Rect {
                        x,
                        y: 100.0,
                        width: 900.0,
                        height: 500.0,
                    },
                ),
            ],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 400.0, y: 150.0 });

        let perched = settle(&mut engine, &world(-1000.0));
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0);

        // Dragged in from the left, under the sprite and over its Perch.
        let covered = engine.tick(&world(0.0));
        assert_eq!(
            covered.position.y, 400.0,
            "still on the edge in front of it: {covered:?}"
        );
        assert_eq!(covered.state, State::Perched);
    }

    /// #100: a Perch you cannot see is gone. Alt-tab, or clicking another
    /// window, puts that window in front and the edge under the sprite
    /// disappears. Staying there left it floating in mid-air on a title bar
    /// nobody can see. It falls.
    ///
    /// It is not hoisted onto the raised window's own top edge. That window
    /// contained the sprite before it was raised, so it swallows nothing
    /// (#78), and that edge sits under the menu bar besides.
    #[test]
    fn a_window_raised_over_the_perch_drops_the_sprite() {
        let maximized = window(
            1,
            Rect {
                // Its top edge under the menu bar, its bottom on the usable
                // floor.
                x: 0.0,
                y: 30.0,
                width: 1000.0,
                height: 770.0,
            },
        );
        let perch = window(
            2,
            Rect {
                x: 200.0,
                y: 400.0,
                width: 400.0,
                height: 200.0,
            },
        );
        // Frontmost first, so the sprite lands on the Perch in front of the
        // maximized window and is inside that window from the moment it does.
        let world = |windows: Vec<Window>| WorldSnapshot {
            windows,
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 400.0, y: 150.0 });

        let perched = settle(&mut engine, &world(vec![perch, maximized]));
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0, "the window it fell onto");

        // The user clicks the maximized window, which comes to the front.
        let raised = world(vec![maximized, perch]);
        let frames: Vec<Frame> = (0..40).map(|_| engine.tick(&raised)).collect();
        assert!(
            frames.iter().any(|frame| frame.state == State::Falling),
            "the hidden edge is gone, so it falls: {frames:?}"
        );
        assert!(
            frames.iter().all(|frame| frame.position.y > 30.0),
            "and is not hoisted under the menu bar: {frames:?}"
        );
        let landed = frames.last().expect("forty ticks produce forty frames");
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0, "down to the floor: {landed:?}");
    }

    /// The same rule met by walking rather than by a window moving: the sprite
    /// strolls along one edge into the middle of a window that overlaps it.
    #[test]
    fn a_walk_under_an_overlapping_window_steps_up_onto_it() {
        let overlapping = || WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: 500.0,
                        y: 250.0,
                        width: 500.0,
                        height: 400.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: 0.0,
                        y: 400.0,
                        width: 1000.0,
                        height: 200.0,
                    },
                ),
            ],
            ..snapshot(100)
        };
        let mut engine = a_character_at(Point { x: 100.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &overlapping()).position.y, 400.0);

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..overlapping()
        });
        let walked: Vec<Frame> = (0..40).map(|_| engine.tick(&overlapping())).collect();

        assert!(
            walked.iter().any(|frame| frame.position.y == 250.0),
            "it steps up onto the window it walked under: {walked:?}"
        );
        assert!(
            walked
                .iter()
                .filter(|frame| matches!(
                    frame.state,
                    State::Grounded | State::Perched | State::Asleep
                ))
                .all(|frame| !inside_a_window(frame.position, &overlapping())),
            "and never comes to rest inside one: {walked:?}"
        );
    }

    /// Two windows overlapping in x, the upper one hanging below the usable
    /// floor as a window behind the Dock does.
    fn overlapping_windows() -> WorldSnapshot {
        WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: 0.0,
                        y: 300.0,
                        width: 600.0,
                        height: 600.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: 200.0,
                        y: 500.0,
                        width: 600.0,
                        height: 300.0,
                    },
                ),
            ],
            ..snapshot(100)
        }
    }

    /// #5: overlapping windows resolve to one Perch and stay there. Two edges
    /// under one sprite is the arrangement that would have it flicking between
    /// them, one per tick, for as long as both windows are open.
    #[test]
    fn overlapping_windows_resolve_to_one_perch_without_jitter() {
        let mut engine = Engine::new(Point { x: 400.0, y: 0.0 });
        let falling: Vec<Frame> = (0..40)
            .map(|_| engine.tick(&overlapping_windows()))
            .collect();
        let landed = falling.last().expect("forty ticks produce forty frames");

        assert_eq!(landed.state, State::Perched);
        assert_eq!(landed.position.y, 300.0, "the upper of the two edges");
        assert!(
            falling.iter().all(|frame| frame.position.y <= 300.0),
            "and it never fell past that edge to the lower one: {falling:?}"
        );

        let settled: Vec<(Point, State)> = (0..20)
            .map(|_| {
                let frame = engine.tick(&overlapping_windows());
                (frame.position, frame.state)
            })
            .collect();
        assert!(
            settled
                .iter()
                .all(|resting| *resting == (landed.position, landed.state)),
            "it sits still rather than flicking between them: {settled:?}"
        );
    }

    /// #5: a top edge hidden behind the window in front of it is not a Perch.
    /// The sprite falls past the covered edge to the floor.
    #[test]
    fn an_edge_hidden_behind_the_window_in_front_of_it_is_not_a_perch() {
        // Frontmost first, and the second window's top edge falls inside the
        // first: only its lower half is anywhere on screen.
        let covered = WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: 100.0,
                        y: 300.0,
                        width: 800.0,
                        height: 500.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: 100.0,
                        y: 600.0,
                        width: 800.0,
                        height: 200.0,
                    },
                ),
            ],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 400.0 });

        let landed = settle(&mut engine, &covered);
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(
            landed.position.y, 800.0,
            "past the hidden edge and down to the floor: {landed:?}"
        );
    }

    /// #5: a top edge no display covers is not a Perch either. One window
    /// straddles two displays of different heights, and the sprite is thrown
    /// out over the shorter one, where that edge hangs over nothing.
    #[test]
    fn an_edge_over_no_display_is_not_a_perch() {
        // Bottom-aligned displays of different heights, the ordinary
        // arrangement: nothing covers x 1000..2000 above y=300.
        let world = WorldSnapshot {
            displays: vec![
                one_display(),
                Rect {
                    x: 1000.0,
                    y: 300.0,
                    width: 1000.0,
                    height: 500.0,
                },
            ],
            windows: vec![window(
                1,
                Rect {
                    x: 700.0,
                    y: 100.0,
                    width: 1000.0,
                    height: 400.0,
                },
            )],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 900.0, y: 50.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 50.0 },
            verbs: vec![Verb::Grab],
            ..world.clone()
        });
        // Thrown right, hard enough to cross onto the shorter display before
        // gravity has taken it down to that display's top.
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 2000.0, y: 0.0 },
            }],
            ..world.clone()
        });

        let landed = settle(&mut engine, &world);
        assert_eq!(landed.state, State::Grounded);
        assert!(
            world
                .displays
                .iter()
                .any(|display| display.spans_x(landed.position.x)
                    && landed.position.y >= display.y
                    && landed.position.y <= display.bottom()),
            "it came to rest somewhere a display covers: {landed:?}"
        );
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
        let mut engine = Engine::new(Point { x: 100.0, y: 400.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 200.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });

        // A production tick, not the 100 ms the other tests use to settle: on
        // a long tick gravity wins a 200-point throw in one step and this
        // would say the sprite never rose. Started below the ceiling so the
        // rise is not stopped in the same tick. #100.
        let thrown = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point {
                    x: 300.0,
                    y: -200.0,
                },
            }],
            ..snapshot(16)
        });

        assert_eq!(thrown.state, State::Falling);
        assert_eq!(thrown.velocity.x, 300.0, "gravity does not slow the arc");
        assert!(thrown.position.x > 200.0, "it travels across: {thrown:?}");
        assert!(
            thrown.position.y < 400.0,
            "an upward throw rises before it falls: {thrown:?}"
        );
    }

    /// #100: a hard upward flick used to spend most of the flight above the
    /// usable frame. The usable top is a ceiling, and the sprite is heavy
    /// enough to come back down onto a Surface.
    #[test]
    fn a_hard_upward_throw_stays_on_the_display_and_lands() {
        let mut engine = Engine::new(Point { x: 500.0, y: 400.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 500.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(16)
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point {
                    x: 400.0,
                    y: -2500.0,
                },
            }],
            ..snapshot(16)
        });

        let flight: Vec<Frame> = (0..180).map(|_| engine.tick(&snapshot(16))).collect();
        let highest = flight
            .iter()
            .map(|frame| frame.position.y)
            .min_by(|a, b| a.total_cmp(b))
            .expect("the flight produces frames");
        assert!(
            highest >= 0.0,
            "left the display at y={highest}, {flight:?}"
        );
        assert!(
            flight.iter().all(|frame| frame.position.y <= 800.0),
            "fell past the floor: {flight:?}"
        );
        let landed = flight.last().expect("the flight produces frames");
        assert_eq!(landed.state, State::Grounded, "it came to rest: {landed:?}");
        assert_eq!(landed.position.y, 800.0, "on the floor: {landed:?}");
    }

    /// #100: a title bar under the menu bar is the usable top, not a Perch.
    /// Landing there leaves only the feet on screen, and often unclickable.
    #[test]
    fn a_window_flush_with_the_usable_top_is_not_a_perch() {
        let under_the_menu_bar = WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x: 0.0,
                    y: 30.0,
                    width: 1000.0,
                    height: 770.0,
                },
            )],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });

        let landed = settle(&mut engine, &under_the_menu_bar);
        assert_eq!(
            landed.state,
            State::Grounded,
            "past the title bar: {landed:?}"
        );
        assert_eq!(landed.position.y, 800.0, "down to the floor: {landed:?}");
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
            windows: vec![window(
                1,
                Rect {
                    x: 50.0,
                    y: 400.0,
                    width: 300.0,
                    height: 200.0,
                },
            )],
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
                velocity: Point { x: 0.0, y: -2500.0 },
            }],
            ..window.clone()
        });
        assert_eq!(
            thrown.state,
            State::Falling,
            "the edge above it is not a surface from underneath: {thrown:?}"
        );
        assert!(
            thrown.position.y < 400.0,
            "it rises through the top edge: {thrown:?}"
        );

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

    /// Two displays with a gap between them, which is what "the union of
    /// visible display frames, not their bounding rectangle" is about. A sprite
    /// that lands in the gap is over no display at all: nothing holds it up,
    /// nothing draws it, and nothing brings it back.
    fn displays_with_a_gap() -> WorldSnapshot {
        WorldSnapshot {
            displays: vec![
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 800.0,
                },
                Rect {
                    x: 1500.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 800.0,
                },
            ],
            elapsed_ms: 100,
            ..WorldSnapshot::default()
        }
    }

    /// #4: the sprite is caught by the union of the displays, not by the
    /// rectangle that bounds them.
    ///
    /// Dropped rather than thrown, because that is the case that actually
    /// strands it. A throw carries enough sideways speed to clear the gap
    /// before gravity matters; letting go over the gap leaves the sprite with
    /// no horizontal velocity at all, so it stays over nothing. `floor_under`
    /// finds no display at that x, `support_below` returns nothing, and it
    /// falls for ever with no edge to catch.
    #[test]
    fn a_sprite_dropped_into_the_gap_between_displays_is_caught_rather_than_lost() {
        let mut engine = Engine::new(Point { x: 900.0, y: 100.0 });

        // Carried by hand into the gap, then let go still.
        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: 1200.0,
                y: 100.0,
            },
            verbs: vec![Verb::Grab],
            ..displays_with_a_gap()
        });
        // Let go. The gap is caught on that same tick rather than after a
        // fall, because the sprite is already over nothing when the hand opens.
        engine.tick(&displays_with_a_gap());

        let landed = settle(&mut engine, &displays_with_a_gap());
        assert!(
            landed.position.y <= 800.0,
            "it came to rest rather than falling for ever: {landed:?}"
        );
        assert!(
            landed.position.x <= 1000.0 || landed.position.x >= 1500.0,
            "and over a display rather than in the gap: {landed:?}"
        );
    }

    /// The same gap from the other side, so the fix cannot be a one-sided clamp.
    #[test]
    fn the_gap_catches_a_sprite_dropped_nearer_its_far_edge() {
        let mut engine = Engine::new(Point {
            x: 1600.0,
            y: 100.0,
        });

        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: 1400.0,
                y: 100.0,
            },
            verbs: vec![Verb::Grab],
            ..displays_with_a_gap()
        });
        engine.tick(&displays_with_a_gap());

        let landed = settle(&mut engine, &displays_with_a_gap());
        assert!(landed.position.y <= 800.0, "it rests: {landed:?}");
        assert!(
            landed.position.x >= 1500.0,
            "recovered to the nearer display, which is the right-hand one: {landed:?}"
        );
    }

    /// #4 asks for an L-shaped arrangement, where the bounding rectangle and
    /// the union differ in y rather than in x. A second display dropped below
    /// the first leaves the top-right of the bounding rectangle covering no
    /// screen at all.
    ///
    /// It does not strand the sprite, and this says so rather than assuming it:
    /// every x in the bounding rectangle is spanned by some display, so there is
    /// always a floor somewhere below.
    #[test]
    fn an_l_shaped_arrangement_leaves_the_sprite_a_floor_everywhere() {
        let l_shaped = || WorldSnapshot {
            displays: vec![
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 800.0,
                },
                Rect {
                    x: 1000.0,
                    y: 400.0,
                    width: 1000.0,
                    height: 800.0,
                },
            ],
            elapsed_ms: 100,
            ..WorldSnapshot::default()
        };

        // Over the second display but above where it begins: inside the
        // bounding rectangle, outside the union.
        let mut engine = Engine::new(Point {
            x: 1500.0,
            y: 100.0,
        });
        let landed = settle(&mut engine, &l_shaped());

        assert_eq!(landed.state, State::Grounded);
        assert_eq!(
            landed.position.y, 1200.0,
            "it falls past the empty space onto the lower display"
        );
        assert_eq!(landed.position.x, 1500.0, "and does not drift sideways");
    }

    /// And a diagonal arrangement, the other case #4 names: two displays meeting
    /// at a corner. The x ranges still touch, so there is no gap to fall into,
    /// and the sprite reaches the lower display's floor.
    #[test]
    fn a_diagonal_arrangement_leaves_the_sprite_a_floor_everywhere() {
        let diagonal = || WorldSnapshot {
            displays: vec![
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 800.0,
                },
                Rect {
                    x: 1000.0,
                    y: 800.0,
                    width: 1000.0,
                    height: 800.0,
                },
            ],
            elapsed_ms: 100,
            ..WorldSnapshot::default()
        };

        let mut engine = Engine::new(Point {
            x: 1500.0,
            y: 100.0,
        });
        let landed = settle(&mut engine, &diagonal());

        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 1600.0);
        assert!(
            landed.position.x >= 1000.0 && landed.position.x <= 2000.0,
            "over the display it landed on: {landed:?}"
        );
    }

    /// Displays that touch have no gap, and the sprite must cross freely. A
    /// clamp that treats every display edge as a wall would trap it on one
    /// screen.
    #[test]
    fn a_sprite_crosses_freely_between_displays_that_touch() {
        let touching = || WorldSnapshot {
            displays: vec![
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 800.0,
                },
                Rect {
                    x: 1000.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 800.0,
                },
            ],
            elapsed_ms: 100,
            ..WorldSnapshot::default()
        };

        let mut engine = Engine::new(Point { x: 900.0, y: 100.0 });
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..touching()
        });
        // Gently, so it lands on the second display rather than sailing past
        // it to the outer edge — which is a catch, and a different test.
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 400.0, y: 0.0 },
            }],
            ..touching()
        });

        let landed = settle(&mut engine, &touching());
        assert!(
            landed.position.x > 1000.0,
            "it crossed onto the second display: {landed:?}"
        );
        assert_eq!(landed.position.y, 800.0, "and stands on its floor");
        assert_eq!(
            landed.state,
            State::Grounded,
            "rather than climbing an edge"
        );
    }

    /// #84: Do Not Disturb refuses Director proposals while the Character stays
    /// visible on screen.
    #[test]
    fn a_proposal_offered_under_do_not_disturb_is_not_applied() {
        let mut engine = a_resting_sprite();
        engine.set_do_not_disturb(true);

        let refused = engine.tick(&proposing("greet"));

        assert_eq!(
            refused.animation, "idle",
            "the proposal was refused and the sprite stays idle"
        );
        assert_eq!(
            refused.behavior, None,
            "no Behavior started playing on this frame"
        );
        assert!(
            refused.position.y <= 800.0,
            "the Character stays on screen: {refused:?}"
        );
    }

    /// #84: turning Do Not Disturb off resumes proposals on the next wake
    /// without reconstructing the Engine.
    #[test]
    fn the_same_proposal_is_applied_once_do_not_disturb_is_off() {
        let mut engine = a_resting_sprite();
        engine.set_do_not_disturb(true);

        engine.tick(&proposing("greet"));

        engine.set_do_not_disturb(false);
        let applied = engine.tick(&proposing("greet"));

        assert_eq!(
            applied.animation, "react",
            "the proposal is applied once Do Not Disturb is off"
        );
        assert_eq!(applied.behavior, Some("greet".to_string()));
    }

    /// #84: Poke still plays `react` from the Engine, the user-initiated
    /// reaction.
    #[test]
    fn poke_still_plays_react_while_do_not_disturb_is_on() {
        let mut engine = a_resting_sprite();
        engine.set_do_not_disturb(true);

        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });

        assert_eq!(
            poked.animation, "react",
            "Poke still plays react while Do Not Disturb is on"
        );
    }

    /// #84: Grab and Throw still move the sprite while Do Not Disturb is on.
    #[test]
    fn grab_and_throw_still_move_the_sprite_under_do_not_disturb() {
        let mut engine = a_resting_sprite();
        let start = engine.tick(&snapshot(100)).position;
        engine.set_do_not_disturb(true);

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 400.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        let grabbed = engine.tick(&WorldSnapshot {
            cursor: Point { x: 600.0, y: 100.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });

        assert_eq!(grabbed.state, State::Dragged);
        assert!(
            (grabbed.position.x - 600.0).abs() < 0.1,
            "Grab moved the sprite: {grabbed:?}"
        );

        let thrown = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point {
                    x: 500.0,
                    y: -200.0,
                },
            }],
            ..snapshot(100)
        });

        assert_eq!(thrown.state, State::Falling);
        assert!(
            thrown.position.x > start.x,
            "Throw moved the sprite: start={start:?}, thrown={thrown:?}"
        );
    }

    /// #84: unprompted Director dialogue is not spoken while Do Not Disturb is
    /// on.
    #[test]
    fn unprompted_director_dialogue_is_not_spoken_under_do_not_disturb() {
        let mut engine = a_resting_sprite();
        engine.set_do_not_disturb(true);

        let silent = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "greet".to_string(),
                dialogue: Some("hello there".to_string()),
            }),
            ..snapshot(100)
        });

        assert_eq!(
            silent.dialogue, None,
            "unprompted dialogue is refused under Do Not Disturb"
        );
    }

    /// #84: idle Behaviors do not start while Do Not Disturb is on, and the
    /// buddy settles into sleep.
    #[test]
    fn idle_behaviors_do_not_start_and_the_buddy_settles_to_sleep() {
        let mut engine = a_resting_sprite();
        engine.set_do_not_disturb(true);

        let quietening: Vec<Frame> = (0..20)
            .map(|_| {
                engine.tick(&WorldSnapshot {
                    proposal: Some(BehaviorProposal {
                        behavior: "greet".to_string(),
                        dialogue: None,
                    }),
                    ..snapshot(100)
                })
            })
            .collect();

        assert!(
            quietening.iter().all(|frame| frame.animation == "idle"),
            "proposals do not start Behaviors while Do Not Disturb is on"
        );

        let asleep = engine.tick(&snapshot(60_000));
        assert_eq!(
            asleep.state,
            State::Asleep,
            "the sprite settles to sleep without Director proposals waking it"
        );
    }

    /// #119: dialogue with an empty behavior plays `talk` for PRIMITIVE_MS,
    /// independent of bubble duration.
    #[test]
    fn dialogue_with_empty_behavior_plays_talk() {
        let mut engine = a_resting_sprite();

        let spoken = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: String::new(),
                dialogue: Some("hello there".to_string()),
            }),
            ..snapshot(100)
        });

        assert_eq!(
            spoken.animation, "talk",
            "dialogue with no Behavior plays talk"
        );
        assert_eq!(spoken.dialogue.as_deref(), Some("hello there"));

        let playing: Vec<&'static str> = (0..10)
            .map(|_| engine.tick(&snapshot(100)).animation)
            .collect();

        let talk_ticks = playing.iter().filter(|&&anim| anim == "talk").count();
        assert!(
            talk_ticks == 5,
            "talk holds for PRIMITIVE_MS (600ms = 5 more ticks after the first): {playing:?}"
        );
    }

    /// #119: dialogue with a playable Behavior plays both — the Behavior and
    /// its dialogue are independent.
    #[test]
    fn dialogue_with_a_playable_behavior_plays_the_behavior_not_talk() {
        let mut engine = a_resting_sprite();

        let spoken = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "greet".to_string(),
                dialogue: Some("hi".to_string()),
            }),
            ..snapshot(100)
        });

        assert_eq!(
            spoken.animation, "react",
            "greet plays its own animation, not talk"
        );
        assert_eq!(spoken.dialogue.as_deref(), Some("hi"));
        assert_eq!(spoken.behavior, Some("greet".to_string()));
    }

    /// #119: a Behavior with no dialogue does not play talk.
    #[test]
    fn a_behavior_without_dialogue_does_not_play_talk() {
        let mut engine = a_resting_sprite();

        let silent = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "greet".to_string(),
                dialogue: None,
            }),
            ..snapshot(100)
        });

        assert_eq!(
            silent.animation, "react",
            "greet plays its own animation"
        );
        assert_eq!(silent.dialogue, None);
        assert_eq!(silent.behavior, Some("greet".to_string()));
    }
}
