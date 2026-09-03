//! The Engine: `WorldSnapshot` in, `Frame` out, once per tick.
//!
//! Pure and synchronous. It reads no clock, holds no timers and performs no
//! I/O, so time reaches it only as elapsed milliseconds on a snapshot. That is
//! what lets every spatial property be tested by constructing snapshots and
//! asserting frames, with no windowing system, no model and no waiting.

use crate::character::{Behavior, CursorReaction, Primitive};
pub use crate::window_source::WindowId;
use std::collections::{BTreeMap, BTreeSet};

mod geometry;
mod transition;
use geometry::*;

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
    /// A right-click on the sprite. Opens the same menu the tray icon opens.
    Menu,
    /// A double-click on the sprite. Opens the chat surface, which is #17;
    /// until it exists the verb is accepted and shows nothing.
    ///
    /// Accepted now rather than added later because the verb set is fixed at
    /// five: every verb is a tax on every Character that will ever exist, and a
    /// sixth would mean a ninth Required Animation.
    Summon,
}

/// The one cue an interaction earned this tick, for the Shell to draw and to
/// sound.
///
/// The Engine picks it because it is the only place that knows both the verbs
/// and the `State::Dragged` transitions, and because it is pure — the choice is
/// decided where a test can reach it without a window. The drawing and the
/// synthesis are the webview's, so a cue costs no Character art and no manifest
/// field, the way the speech bubble already costs none. The vocabulary is
/// global: no Character declares a pitch or a colour. #277.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// A left click.
    Poke,
    /// A double click. The Shell plays it over a Poke cue still in flight: the
    /// first click of the pair emitted its Poke before anything could know a
    /// second was coming.
    Summon,
    /// A right click.
    Menu,
    /// The sprite left its footing for the cursor.
    Pickup,
    /// The sprite was let go standing still.
    Drop,
    /// The sprite was let go moving. The Drop cue played harder rather than a
    /// sixth shape and a sixth sound.
    Throw,
}

impl Cue {
    /// The name the webview's cue machine keys its visual and its sound by.
    ///
    /// A name rather than a serialized enum, following `Frame::animation`: what
    /// crosses to the webview is already a table lookup on the other side.
    pub fn name(self) -> &'static str {
        match self {
            Self::Poke => "poke",
            Self::Summon => "summon",
            Self::Menu => "menu",
            Self::Pickup => "pickup",
            Self::Drop => "drop",
            Self::Throw => "throw",
        }
    }

    /// The cue a verb carries on its own.
    ///
    /// `Grab` and `Throw` carry none: a cue keyed on `Grab` would fire on every
    /// tick of a drag, and a `Throw` is answered by the transition out of
    /// `Dragged` — which a slow release, emitting no verb at all, has to be
    /// answered by anyway.
    fn of_verb(verb: &Verb) -> Option<Self> {
        match verb {
            Verb::Poke => Some(Self::Poke),
            Verb::Summon => Some(Self::Summon),
            Verb::Menu => Some(Self::Menu),
            Verb::Grab | Verb::Throw { .. } => None,
        }
    }
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
    /// all be derived from these rectangles. The only thing the Engine knows
    /// about the Dock is its Perch's reserved id, which makes its side a wall
    /// (#176).
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
    /// Whether the user addressed the buddy this tick: a Poke, a Summon, a
    /// Menu or a Dwell. The Shell wakes the session Director from this bit.
    pub addressed: bool,
    /// The cue this interaction earned, if one landed. A one-tick pulse like
    /// `dialogue`, and at most one a tick — the precedence is in `tick`. #277.
    pub cue: Option<Cue>,
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

/// How long a poked sprite stands still after reacting, before it may move
/// again. A tuning knob. `PRIMITIVE_MS` is the length of the `react` art, not
/// of a pause that reads as the character noticing you; 2.5 s is long enough
/// to register and short enough that a click never feels like a freeze. #177.
const POKE_COOLDOWN_MS: u32 = 2_500;

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

/// How far from a horizontal display edge the feet must stay so the full
/// sprite is on-screen when standing (not climbing). The Engine does not know
/// the art's size; 64 is 16px at 4×, enough for a character of typical width.
/// Climb frames assume the wall is in the middle of the frame, so this inset
/// applies only to non-climb states.
const EDGE_CLEARANCE: f64 = 64.0;

/// Points per second squared. The yank gate is this times `YANK_WINDOW_S`:
/// a change in Perch speed larger than that, measured against the speed
/// from about one idle poll ago, drops the sprite. The last 16 ms slope
/// treats WindowServer jitter as a yank. #98.
pub const RIDE_ACCELERATION: f64 = 10_000.0;

/// How far back the yank gate looks. Fast poll still tracks the window;
/// only the fall decision stays on this cadence. #98.
const YANK_WINDOW_S: f64 = 0.1;

/// Cursor proximity radius in points. The cursor enters Near when it crosses
/// this threshold around the sprite (#152).
///
/// A tuning knob: close enough to react before the hand is on top of the sprite,
/// far enough that the buddy does not jump at every scroll or window move.
const NEAR_RADIUS: f64 = 150.0;

/// How long the cursor must rest on the sprite to count as Dwell, in milliseconds (#152).
///
/// A tuning knob: long enough that passing over the sprite does not wake the
/// Director, short enough that intentionally addressing feels immediate.
const DWELL_MS: u32 = 400;

/// Minimum cursor velocity toward the sprite to count as a Rush, in points per second (#152).
///
/// A tuning knob: fast enough to read as a startle (a flick or a fast approach),
/// slow enough that ordinary cursor travel near the sprite does not trigger it.
const RUSH_VELOCITY: f64 = 800.0;

/// Maximum time the chase Primitive pursues the cursor before giving up, in milliseconds (#153).
///
/// A tuning knob: long enough to catch a cursor that is moving or that starts far away,
/// short enough that a chase that will never close still disengages in time for the
/// sprite to do something else. Boredom is the realism.
const CHASE_TIMEOUT_MS: u32 = 8000;

/// How close the sprite's x must be to the cursor's x to count as arrival, in points (#153).
///
/// A tuning knob: close enough to read as catching it, loose enough that the
/// sprite does not overshoot and backtrack.
const CHASE_ARRIVAL_THRESHOLD: f64 = 30.0;

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
    /// Character's reactions to cursor proximity (#152).
    near_reaction: CursorReaction,
    rush_reaction: CursorReaction,
    /// Whether the cursor is currently within NEAR_RADIUS of the sprite (#152).
    cursor_near: bool,
    /// Milliseconds the cursor has rested on the sprite without pressing (#152).
    cursor_dwell_ms: u32,
    /// Whether this dwell session has already addressed the Director (#152).
    cursor_dwell_addressed: bool,
    /// Last observed cursor position, for Rush velocity calculation (#152).
    last_cursor: Point,
    /// Last observed cursor velocity, for Rush detection (#152).
    cursor_velocity: Point,
    /// Whether a Rush has already been reported this Near session (#152).
    /// Prevents repeated Rush reactions while the cursor stays near.
    rush_reported: bool,
    /// Milliseconds since the chase Primitive started (#153).
    chase_ms: u32,
    /// Milliseconds left of standing still after a Poke. While it runs, a
    /// proposal that would move the sprite is refused; a Grab, a Throw or
    /// losing the ground ends it at once. #177.
    poke_cooldown_ms: u32,
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
            near_reaction: CursorReaction::default(),
            rush_reaction: CursorReaction::default(),
            cursor_near: false,
            cursor_dwell_ms: 0,
            cursor_dwell_addressed: false,
            last_cursor: Point::default(),
            cursor_velocity: Point::default(),
            rush_reported: false,
            chase_ms: 0,
            poke_cooldown_ms: 0,
        }
    }

    /// The Behaviors this Character declares. Nothing else reaches the Engine
    /// from a Character Package: art is the renderer's, and a Behavior is
    /// Primitives the Engine already owns.
    pub fn with_behaviors(mut self, behaviors: BTreeMap<String, Behavior>) -> Self {
        self.behaviors = behaviors;
        self
    }

    /// The Character's cursor reactions (#152).
    pub fn with_cursor_reactions(mut self, near: CursorReaction, rush: CursorReaction) -> Self {
        self.near_reaction = near;
        self.rush_reaction = rush;
        self
    }

    /// #84: toggle Do Not Disturb. The Character stays visible but stops
    /// starting things: no Director proposals are applied and no unprompted
    /// dialogue is spoken. Poke, Grab, and Throw still work.
    ///
    /// A walk already under way has to be sat down too. Walk velocity
    /// outlives the Primitive that started it, so refusing the next
    /// proposal would otherwise leave the sprite pacing.
    pub fn set_do_not_disturb(&mut self, enabled: bool) {
        self.do_not_disturb = enabled;
        if enabled
            && matches!(self.state, State::Grounded | State::Perched)
            && (self.is_walking()
                || matches!(self.on_screen(), Some(Primitive::Walk | Primitive::Chase)))
        {
            let _ = self.play(&[Primitive::Sit]);
            self.velocity.x = 0.0;
        }
    }

    pub fn do_not_disturb(&self) -> bool {
        self.do_not_disturb
    }

    /// Swap the Character this Engine is playing without moving the sprite.
    ///
    /// A switch is a new set of Behaviors and cursor reactions, not a new
    /// body: dropping the sprite so it can fall as someone else would be a
    /// teleport the user did not ask for.
    pub fn retarget(
        &mut self,
        behaviors: BTreeMap<String, Behavior>,
        near: CursorReaction,
        rush: CursorReaction,
    ) {
        self.behaviors = behaviors;
        self.near_reaction = near;
        self.rush_reaction = rush;
        self.playing.clear();
        self.primitive_ms = 0;
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

        // Chase times out on wall time, not Primitive turns, so the clock
        // runs while Chase is on screen (#153).
        if self.on_screen() == Some(Primitive::Chase) {
            self.chase_ms = self.chase_ms.saturating_add(snapshot.elapsed_ms);
        }

        // What was already playing ages before anything new starts, so a
        // Primitive begun this tick gets its whole turn rather than losing this
        // tick's milliseconds to the one it replaced.
        let mut started = self.advance(snapshot.elapsed_ms);

        // `woke` marks a sprite a verb roused, so the footing it is put back
        // on is not mistaken for one it arrived at. See the landing below.
        let (state, woke) = transition::on_verbs(self.state, &snapshot.verbs);

        // The cooldown is a thing the sprite does on its feet. Being picked up
        // or losing the ground ends it, because a cooldown that outlived the
        // ground would refuse the first walk after the landing. `state` is
        // what the verbs made of it; a fall the world causes is decided at
        // `on_contact` below, so that clear lands one tick late — harmless,
        // since `permitted` refuses a walk while Falling anyway.
        self.poke_cooldown_ms = match state {
            State::Grounded | State::Perched => {
                self.poke_cooldown_ms.saturating_sub(snapshot.elapsed_ms)
            }
            _ => 0,
        };

        // Any Verb aborts chase like it aborts walk (#153).
        if !snapshot.verbs.is_empty() && self.on_screen() == Some(Primitive::Chase) {
            self.playing.clear();
            self.primitive_ms = 0;
            self.chase_ms = 0;
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
        //
        // The pickup and the drop cue are decided here rather than from a verb,
        // and this is the only place they can be: `Verb::Grab` is present on
        // every tick the sprite is held, and a slow release emits no verb at
        // all. The transition is the edge, and both States are in hand only
        // between `on_verbs` above and `on_contact` below. #277.
        let mut cue = None;
        if state == State::Dragged {
            if self.state != State::Dragged {
                cue = Some(Cue::Pickup);
            }
            self.position = snapshot.cursor;
            self.velocity = Point::default();
        } else if self.state == State::Dragged {
            // Let go. With velocity that is a Throw; without, it simply drops.
            let thrown = thrown_velocity(snapshot);
            cue = Some(if thrown.is_some() {
                Cue::Throw
            } else {
                Cue::Drop
            });
            self.velocity = thrown.unwrap_or_default();
        }

        // The cursor already arrives every tick for hit-testing, so noticing
        // costs no new sensing (#152, #153).
        let cursor_distance =
            (snapshot.cursor.x - self.position.x).hypot(snapshot.cursor.y - self.position.y);
        let was_near = self.cursor_near;
        self.cursor_near = cursor_distance < NEAR_RADIUS;

        // Cursor velocity for Rush detection: points moved since last tick.
        let cursor_moved = Point {
            x: snapshot.cursor.x - self.last_cursor.x,
            y: snapshot.cursor.y - self.last_cursor.y,
        };
        self.cursor_velocity = if snapshot.elapsed_ms > 0 {
            Point {
                x: cursor_moved.x * 1000.0 / f64::from(snapshot.elapsed_ms),
                y: cursor_moved.y * 1000.0 / f64::from(snapshot.elapsed_ms),
            }
        } else {
            Point::default()
        };
        self.last_cursor = snapshot.cursor;

        // Crossing into Near plays the Character's near_reaction, including under DND (#152).
        if self.cursor_near && !was_near {
            self.rush_reported = false;
            match self.near_reaction {
                CursorReaction::Indifferent => {}
                CursorReaction::Speak => {
                    started |= self.play(&[Primitive::Talk]);
                }
                CursorReaction::Face => {
                    self.facing = if snapshot.cursor.x > self.position.x {
                        1.0
                    } else {
                        -1.0
                    };
                }
                CursorReaction::Toward => {
                    // One-shot walk, not pursuit: Chase is the Primitive that follows (#153).
                    self.facing = if snapshot.cursor.x > self.position.x {
                        1.0
                    } else {
                        -1.0
                    };
                    started |= self.play(&[Primitive::Walk]);
                }
                CursorReaction::Away => {
                    self.facing = if snapshot.cursor.x < self.position.x {
                        1.0
                    } else {
                        -1.0
                    };
                    started |= self.play(&[Primitive::Walk]);
                }
                CursorReaction::React => {
                    started |= self.play(&[Primitive::React]);
                }
            }
        }

        // Leaving Near does not cancel a walk the reaction started. Chase is
        // what follows a cursor; this walk keeps going until it runs out (#153).
        if !self.cursor_near && was_near {
            self.cursor_dwell_ms = 0;
            self.cursor_dwell_addressed = false;
            self.rush_reported = false;
        }

        // Rush (startle): once per Near session, or a fast cursor still in the
        // radius retriggers every tick (#152).
        if self.cursor_near && !self.rush_reported {
            let velocity_magnitude = self.cursor_velocity.x.hypot(self.cursor_velocity.y);
            // Speed while already Near is the whole test; direction is not
            // measured, so a flick past startles the same as a flick at it.
            let toward = cursor_distance < NEAR_RADIUS * 1.2 && velocity_magnitude > RUSH_VELOCITY;
            if toward {
                self.rush_reported = true;
                match self.rush_reaction {
                    CursorReaction::Indifferent => {}
                    CursorReaction::Speak => {
                        started |= self.play(&[Primitive::Talk]);
                    }
                    CursorReaction::Face => {
                        self.facing = if snapshot.cursor.x > self.position.x {
                            1.0
                        } else {
                            -1.0
                        };
                    }
                    CursorReaction::Toward => {
                        self.facing = if snapshot.cursor.x > self.position.x {
                            1.0
                        } else {
                            -1.0
                        };
                        started |= self.play(&[Primitive::Walk]);
                    }
                    CursorReaction::Away => {
                        self.facing = if snapshot.cursor.x < self.position.x {
                            1.0
                        } else {
                            -1.0
                        };
                        started |= self.play(&[Primitive::Walk]);
                    }
                    CursorReaction::React => {
                        started |= self.play(&[Primitive::React]);
                    }
                }
            }
        }

        // Dwell is the cursor on the art, not merely Near. 30 points is a
        // typical sprite half-width; `NEAR_RADIUS` would address from a
        // window away (#152).
        //
        // Counts as addressing: `addressed` makes the next Director wake reactive.
        let mut addressed = false;
        if cursor_distance < 30.0 {
            self.cursor_dwell_ms = self.cursor_dwell_ms.saturating_add(snapshot.elapsed_ms);
            if self.cursor_dwell_ms >= DWELL_MS && !self.cursor_dwell_addressed {
                self.cursor_dwell_addressed = true;
                addressed = true;
                started |= self.play(&[Primitive::Talk]);
            }
        } else {
            self.cursor_dwell_ms = 0;
            self.cursor_dwell_addressed = false;
        }

        // Walking is the Engine's, deciding to walk is not: nothing else here
        // moves the sprite of its own accord. A walk needs no ending — it lasts
        // until the sprite runs out of Perch, which is the whole point of it —
        // so the velocity holds when the Behavior that started it is over.
        // What does stop it is a Primitive that is the sprite standing still
        // (`walk sit` would otherwise slide along the edge it sat down on),
        // and a Poke or a Summon — answered with the other verbs further down —
        // which zeroes the feet and starts the cooldown (#177).
        //
        // Chase (#153) steers walk velocity toward the cursor's x along the
        // ground: y is a fall, not a pursuit. Arrival is a swat, not overlap;
        // without a timeout the sprite would walk off the display after a
        // cursor that never stops.
        if matches!(state, State::Grounded | State::Perched) {
            match self.on_screen() {
                Some(Primitive::Walk) => self.velocity.x = self.facing * WALK_SPEED,
                Some(Primitive::Chase) => {
                    let target_x = snapshot.cursor.x;
                    let distance_x = (target_x - self.position.x).abs();

                    if distance_x < CHASE_ARRIVAL_THRESHOLD {
                        self.velocity.x = 0.0;
                        self.playing.clear();
                        self.primitive_ms = 0;
                        started |= self.play(&[Primitive::React]);
                    } else if self.chase_ms >= CHASE_TIMEOUT_MS {
                        self.velocity.x = 0.0;
                        self.playing.clear();
                        self.primitive_ms = 0;
                        started = true;
                    } else {
                        self.facing = if target_x > self.position.x {
                            1.0
                        } else {
                            -1.0
                        };
                        self.velocity.x = self.facing * WALK_SPEED;
                    }
                }
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

        // Standing at an edge: inset to keep the full sprite on-screen and
        // face away. Dragged follows the cursor including over edges (#39);
        // Falling must reach the wall to trigger Contact::Wall. Riding and
        // coasting have their own display-edge logic (#128); edge correction
        // must not fight them. Climb frames assume the wall is in the middle,
        // so clipping is intentional. A held walk is excluded the same way:
        // it drives at the edge on purpose — that is how the wall is reached
        // and climbed — and correcting it mid-travel set the sprite back a
        // step each time it closed the gap, a visible stutter, because the
        // held velocity re-derives `facing` every tick and the flip below
        // never survived to turn the walk around.
        let stationary = matches!(self.state, State::Grounded | State::Perched | State::Asleep)
            && self.velocity.x == 0.0
            && !self.riding
            && self.coast_s == 0.0;
        if stationary {
            if let Some((edge_x, face_direction)) = at_horizontal_edge(self.position.x, snapshot) {
                // A Perch narrower than `EDGE_CLEARANCE` would lose the sprite
                // if the inset ran anyway.
                let can_inset = if self.state == State::Perched {
                    perch_at(
                        Point {
                            x: edge_x,
                            y: self.position.y,
                        },
                        &snapshot.windows,
                    )
                    .is_some()
                } else {
                    true
                };

                if can_inset {
                    self.position.x = edge_x;
                }
                self.facing = face_direction;
            }
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
                    // #177: mid-cooldown, `play` refuses a chain that would move
                    // the sprite — whole, greeting included, since a Behavior
                    // is one thing to refuse. A line or a gesture on its own
                    // plays. What comes after the cooldown is the Director's
                    // fresh call, not the interrupted walk resuming.
                    if self.play(&primitives) {
                        started = true;
                        behavior = Some(proposal.behavior.clone());
                    }
                } else if proposal.behavior.is_empty()
                    && proposal.dialogue.is_some()
                    && self.play(&[Primitive::Talk])
                {
                    started = true;
                }
            }
        }

        // A Poke is answered, whatever else is going on. Being prodded is the
        // one thing a companion must never ignore, and it reads as alive
        // exactly because it interrupts — including a Behavior, and including
        // its own reaction: prodded again, it reacts again from the beginning.
        //
        // A Summon is the second click of a pair, emitted in place of that
        // click's Poke (#277), so it is answered the same way — a double-click
        // that did visibly less than a single click would read as a miss.
        // Menu is not: the tray menu opening is its response.
        if snapshot
            .verbs
            .iter()
            .any(|verb| matches!(verb, Verb::Poke | Verb::Summon))
        {
            // On its feet, a Poke also stops them: the reaction is the
            // character noticing you, and a character that notices you does
            // not keep strolling past. Mid-air it changes nothing about the
            // flight. #177.
            if matches!(self.state, State::Grounded | State::Perched) {
                self.velocity.x = 0.0;
                self.poke_cooldown_ms = POKE_COOLDOWN_MS;
            }
            started |= self.play(&[Primitive::React]);
        }
        // A click is how the user tests the Director, and a Summon or a Menu
        // is the same reach for the sprite. Dwell sets the same bit; the Shell
        // reads one field for all of them.
        addressed |= snapshot
            .verbs
            .iter()
            .any(|verb| matches!(verb, Verb::Poke | Verb::Summon | Verb::Menu));

        // One cue a tick, and a hand transition outranks a click: the verb that
        // shares a tick with a pickup or a drop is the incidental one — a
        // right-click during a drag, say — and the hand is what the user is
        // plainly doing. Among the click verbs the first is taken, which is
        // every case there is: two clicks cannot land inside one tick.
        let cue = cue.or_else(|| snapshot.verbs.iter().find_map(Cue::of_verb));

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
            addressed,
            cue,
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

                if let Some(wall) = wall_reached(self.position.x, self.velocity.x, snapshot)
                    .or_else(|| dock_side_reached(self.position, snapshot))
                {
                    // Arriving at a screen edge sideways is a catch, not a stop.
                    // It also keeps the sprite inside the displays. The Dock's
                    // side catches the same way: a low throw or a drop under
                    // the Dock climbs out instead of resting behind it.
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
                let next_y = self.position.y - CLIMB_SPEED * dt;

                // Climbing the Dock's side ends on its top: the Dock is a
                // Perch, and the feet reaching its top edge is a landing, not
                // a ceiling. The step inward is what the climb was for — a
                // sprite that stopped clear of the side is standing beside the
                // Dock, not on it. #176.
                if let Some(dock) = dock_in(snapshot) {
                    if self.position.y > dock.y && next_y <= dock.y {
                        if let Some(x) = dock_top_at(self.position.x, dock) {
                            self.position = Point { x, y: dock.y };
                            return Some(Contact::Landed(Surface::Perch));
                        }
                    }
                }
                self.position.y = next_y;

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

                // A walk on the floor beside the Dock stops clear of its
                // side and climbs it (#176). Placed there once rather than
                // corrected back a step each tick: the held walk would
                // re-close the gap and stutter, the way the edge inset did
                // before #141. The Engine does not clamp `dt`, so a step
                // longer than the Dock is wide would cross it unseen; the
                // snapshot assembler caps `elapsed_ms` at one poll interval,
                // which is what keeps a step small.
                if let Some(side) = dock_side_reached(self.position, snapshot) {
                    self.position.x = side;
                    self.velocity = Point::default();
                    return Some(Contact::Wall);
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
                        let coasted = Point {
                            x: origin.x
                                + self.hold_offset_x
                                + self.perch_velocity.x * t
                                + 0.5 * self.perch_acceleration.x * t * t,
                            y: origin.y
                                + self.perch_velocity.y * t
                                + 0.5 * self.perch_acceleration.y * t * t,
                        };
                        // A coast places the sprite with no sample to approve
                        // it, and it runs on every tick the poll is late
                        // rather than on one — so an extrapolation off the
                        // displays is a long absence, not a frame of one. It
                        // lets go instead. #128.
                        if !on_a_display(coasted, snapshot) {
                            self.rest_perch();
                            return Some(Contact::Airborne);
                        }
                        self.position = coasted;
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
        // already standing on it. #100. Asked at the arrival x, because both
        // answers below place the sprite and a sideways ride carries it as
        // far as the window went. #128.
        if !is_perch(index, self.arrival_x(current), snapshot) {
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

    /// Where `place_on` will put the feet, which is what a ride has to ask
    /// its questions about rather than about where they are. #128.
    fn arrival_x(&self, window: Window) -> f64 {
        window.rect.x + self.hold_offset_x
    }

    /// Put the sprite back on `window` at the offset it was holding.
    fn place_on(&mut self, window: Window) {
        self.position.x = self.arrival_x(window);
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

        // A new Chase is a new pursuit; leftover ms from the last one would
        // time out mid-stride (#153).
        if primitives.contains(&Primitive::Chase) {
            self.chase_ms = 0;
        }
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
    ///
    /// Motion has one more condition: the cooldown after a Poke (#177). It is
    /// checked here rather than at the proposal, because a proposal is not the
    /// only thing that starts a walk — a cursor reaction or a chase does too,
    /// and every one of them comes through this gate.
    fn permitted(&self, primitives: &[Primitive]) -> bool {
        let on_feet = matches!(self.state, State::Grounded | State::Perched);
        primitives.iter().all(|primitive| match primitive {
            Primitive::React | Primitive::Talk => true,
            Primitive::Walk | Primitive::Chase => on_feet && self.poke_cooldown_ms == 0,
            _ => on_feet,
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
        // No chase Animation in the required set; walk is the motion (#153).
        Primitive::Chase => "walk",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::DEFAULT_WEIGHT;
    use crate::window_source::DOCK_PERCH_ID;

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

    /// Whether a display covers `position` — the invariant #5 and #85
    /// criterion 9 are about, asked of a point rather than of a resting place.
    fn covered(position: Point, snapshot: &WorldSnapshot) -> bool {
        snapshot.displays.iter().any(|display| {
            display.spans_x(position.x) && position.y >= display.y && position.y <= display.bottom()
        })
    }

    /// A Perch on a second display too short to hold it, `x` points along.
    /// Nothing covers x 1000..2000 outside y 300..500, and nothing at all
    /// covers past x 2000, so a window dragged right runs its edge out of the
    /// displays while still spanning the sprite.
    fn strip_perch(x: f64) -> WorldSnapshot {
        WorldSnapshot {
            displays: vec![
                one_display(),
                Rect {
                    x: 1000.0,
                    y: 300.0,
                    width: 1000.0,
                    height: 200.0,
                },
            ],
            windows: vec![window(
                1,
                Rect {
                    x,
                    y: 450.0,
                    width: 600.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        }
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

    /// A held walk drives the sprite at the display edge on purpose: that is
    /// how it reaches the wall and climbs. The edge correction is for coming
    /// to rest (#123) and must not fight the travel — correcting a moving
    /// sprite teleports it back a step each time it closes the gap, a visible
    /// stutter, and the wall becomes unreachable.
    #[test]
    fn a_walk_reaches_the_display_edge_without_being_set_back() {
        let mut engine = a_character_at(Point { x: 300.0, y: 0.0 });
        settle(&mut engine, &snapshot(16));
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 300.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(16)
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: -200.0, y: 0.0 },
            }],
            ..snapshot(16)
        });
        settle(&mut engine, &snapshot(16));
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..snapshot(16)
        });

        let mut previous = engine.tick(&snapshot(16));
        for _ in 0..400 {
            let frame = engine.tick(&snapshot(16));
            if frame.state == State::Climbing {
                return; // the wall was reached, which is what a walk is for
            }
            assert!(
                frame.position.x <= previous.position.x,
                "walking left never moves the sprite back right: {} -> {}",
                previous.position.x,
                frame.position.x
            );
            previous = frame;
        }
        panic!("the walk never reached the display edge: {previous:?}");
    }

    /// The Dock on `one_display`, as the snapshot assembler adds it: frontmost,
    /// wearing the reserved id, reaching down to the floor.
    fn dock() -> Rect {
        Rect {
            x: 400.0,
            y: 700.0,
            width: 200.0,
            height: 100.0,
        }
    }

    fn dock_snapshot(elapsed_ms: u32) -> WorldSnapshot {
        WorldSnapshot {
            windows: vec![window(DOCK_PERCH_ID, dock())],
            ..snapshot(elapsed_ms)
        }
    }

    /// Where a climb up the Dock's left side comes to rest, and where one up
    /// its right side does: a half sprite in from the corner, on the top.
    fn on_the_dock_from_the_left() -> Point {
        Point {
            x: dock().x + EDGE_CLEARANCE,
            y: dock().y,
        }
    }

    fn on_the_dock_from_the_right() -> Point {
        Point {
            x: dock().x + dock().width - EDGE_CLEARANCE,
            y: dock().y,
        }
    }

    /// The Dock is the one thing on screen drawn in front of the sprite, so a
    /// walk that carries on under it puts the sprite where nobody can see or
    /// grab it. Its side is a wall: the walk stops short of it without being
    /// set back a step (the #141 stutter), and climbs onto the top — a Perch
    /// the sprite already knows how to stand on. #176.
    #[test]
    fn a_walk_into_the_dock_climbs_onto_it_rather_than_behind_it() {
        let dock = dock();
        let mut engine = a_character_at(Point { x: 300.0, y: 0.0 });
        settle(&mut engine, &dock_snapshot(100));
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..dock_snapshot(16)
        });

        let mut previous = engine.tick(&dock_snapshot(16));
        let mut climbed = false;
        for _ in 0..400 {
            let frame = engine.tick(&dock_snapshot(16));
            let behind = frame.position.y > dock.y
                && frame.position.x > dock.x
                && frame.position.x < dock.x + dock.width;
            assert!(!behind, "behind the Dock: {:?}", frame.position);
            if frame.state == State::Grounded {
                assert!(
                    frame.position.x >= previous.position.x,
                    "walking right never sets the sprite back: {} -> {}",
                    previous.position.x,
                    frame.position.x
                );
            }
            climbed |= frame.state == State::Climbing;
            if frame.state == State::Perched {
                assert!(climbed, "on the Dock's top without climbing its side");
                assert_eq!(frame.position, on_the_dock_from_the_left(), "{frame:?}");
                return;
            }
            previous = frame;
        }
        panic!("the walk never reached the Dock's top: {previous:?}");
    }

    /// Standing on the Dock's top is standing above its side, not behind it:
    /// a walk along the top meets no wall.
    #[test]
    fn a_walk_along_the_dock_top_meets_no_wall() {
        let mut engine = a_character_at(Point { x: 500.0, y: 0.0 });
        let landed = settle(&mut engine, &dock_snapshot(100));
        assert_eq!(
            (landed.state, landed.position.y),
            (State::Perched, dock().y)
        );
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..dock_snapshot(16)
        });

        for _ in 0..20 {
            let frame = engine.tick(&dock_snapshot(16));
            assert_ne!(frame.state, State::Climbing, "{frame:?}");
            assert_eq!(frame.position.y, dock().y, "{frame:?}");
        }
    }

    /// Let go with the cursor over the Dock and the sprite falls behind it. It
    /// does not come to rest there: it climbs the nearer side out, however far
    /// out that is — a real Dock is wide, and the way out from the middle of
    /// one is a long way sideways.
    #[test]
    fn a_sprite_dropped_behind_the_dock_climbs_out_onto_it() {
        let mut engine = a_character_at(Point { x: 300.0, y: 0.0 });
        settle(&mut engine, &dock_snapshot(100));
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 480.0, y: 760.0 },
            verbs: vec![Verb::Grab],
            ..dock_snapshot(16)
        });

        let frame = settle(&mut engine, &dock_snapshot(16));
        assert_eq!(frame.state, State::Perched, "{frame:?}");
        assert_eq!(frame.position, on_the_dock_from_the_left());
    }

    /// The nearer side is whichever one it is. Dropped in the right-hand half,
    /// the sprite climbs out to the right, not back across everything it was
    /// hidden behind.
    #[test]
    fn a_sprite_dropped_near_the_docks_right_end_climbs_out_that_side() {
        let mut engine = a_character_at(Point { x: 300.0, y: 0.0 });
        settle(&mut engine, &dock_snapshot(100));
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 560.0, y: 760.0 },
            verbs: vec![Verb::Grab],
            ..dock_snapshot(16)
        });

        let frame = settle(&mut engine, &dock_snapshot(16));
        assert_eq!(frame.state, State::Perched, "{frame:?}");
        assert_eq!(frame.position, on_the_dock_from_the_right());
    }

    /// The sprite need not walk into the Dock to end up behind it: an
    /// autohidden Dock slides out around whatever is standing there, asleep
    /// included. The sprite that was resting in front of nothing is now behind
    /// something, and climbs out the same way.
    #[test]
    fn a_dock_that_unhides_around_a_sleeping_sprite_wakes_it_onto_the_top() {
        let mut engine = a_character_at(Point { x: 450.0, y: 0.0 });
        settle(&mut engine, &snapshot(100));
        let asleep = engine.tick(&snapshot(SLEEP_AFTER_MS));
        assert_eq!(asleep.state, State::Asleep, "{asleep:?}");

        let frame = settle(&mut engine, &dock_snapshot(16));
        assert_eq!(frame.state, State::Perched, "{frame:?}");
        assert_eq!(frame.position, on_the_dock_from_the_left());
    }

    /// Behind the Dock means behind the Dock's own display. Displays stack
    /// vertically as well as side by side, and a display below this one shares
    /// its x-range: a sprite on that floor is under the Dock's columns and
    /// behind none of it. Treating it as hidden climbs it onto a Dock on
    /// another screen, or loops it for ever — climb, top out, fall, climb.
    #[test]
    fn a_sprite_on_a_display_below_the_docks_is_not_behind_the_dock() {
        let below = Rect {
            x: 0.0,
            y: 800.0,
            width: 1000.0,
            height: 800.0,
        };
        let stacked = WorldSnapshot {
            displays: vec![one_display(), below],
            ..dock_snapshot(16)
        };
        let standing = Point {
            x: 500.0,
            y: below.bottom(),
        };

        let mut engine = a_character_at(standing);
        for _ in 0..80 {
            let frame = engine.tick(&stacked);
            assert_ne!(frame.state, State::Climbing, "{frame:?}");
            assert_eq!(frame.position, standing, "{frame:?}");
        }
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
    /// the tray's menu (#18); neither is here, and neither moves the sprite.
    /// They are still the user reaching for it, so a sleeping one wakes — an
    /// interaction that left it snoring would read as ignored — and the
    /// Director hears it, or the loudest interactions there are would be the
    /// ones it never learned of (#277).
    #[test]
    fn a_summon_or_a_menu_wakes_the_sprite_and_addresses_the_director() {
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
            assert!(
                addressed.addressed,
                "{verb:?} is the user reaching for the sprite"
            );
        }
    }

    /// #277: a double-click is a Poke and then a Summon, and the Summon stands
    /// where the second click's Poke used to. It plays the reaction that Poke
    /// brought, or a double-click would visibly do less than a single click.
    #[test]
    fn a_summon_plays_the_reaction() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        settle(&mut engine, &snapshot(100));

        let summoned = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Summon],
            ..snapshot(100)
        });
        assert_eq!(summoned.animation, "react");
    }

    /// #277: the cue is the Engine's to pick, because it is the only place that
    /// knows both the verbs and the `Dragged` transitions. A click verb carries
    /// its own cue, and it is a pulse — the tick after is silent, or a single
    /// click would sound for as long as the reaction plays.
    #[test]
    fn a_click_verb_carries_its_own_cue() {
        for (verb, cue) in [
            (Verb::Poke, Cue::Poke),
            (Verb::Summon, Cue::Summon),
            (Verb::Menu, Cue::Menu),
        ] {
            let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
            settle(&mut engine, &snapshot(100));

            let clicked = engine.tick(&WorldSnapshot {
                verbs: vec![verb],
                ..snapshot(100)
            });
            assert_eq!(clicked.cue, Some(cue), "{verb:?}");

            let after = engine.tick(&snapshot(100));
            assert_eq!(after.cue, None, "and rides one tick only, after {verb:?}");
        }
    }

    /// #277: `Verb::Grab` is present on every tick the sprite is held, so a cue
    /// keyed on the verb would sound sixty times a second for as long as the
    /// drag lasts. The pickup cue keys on entering `Dragged`.
    #[test]
    fn the_pickup_cue_fires_on_the_transition_and_not_every_held_tick() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &snapshot(100));

        let held = WorldSnapshot {
            cursor: Point { x: 200.0, y: 200.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        };
        let grabbed = engine.tick(&held);
        assert_eq!(grabbed.state, State::Dragged);
        assert_eq!(grabbed.cue, Some(Cue::Pickup));

        let carried: Vec<Option<Cue>> = (0..5).map(|_| engine.tick(&held).cue).collect();
        assert!(
            carried.iter().all(Option::is_none),
            "held, and cued once: {carried:?}"
        );
    }

    /// #277: there is no drop verb and a sixth is not allowed, so both cues key
    /// on leaving `Dragged` — which is also what tells them apart, a throw
    /// being the only one of the two that carries a velocity.
    #[test]
    fn letting_go_cues_a_drop_and_letting_go_moving_cues_a_throw() {
        for (release, cue) in [
            (Vec::new(), Cue::Drop),
            (
                vec![Verb::Throw {
                    velocity: Point {
                        x: 400.0,
                        y: -200.0,
                    },
                }],
                Cue::Throw,
            ),
        ] {
            let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
            settle(&mut engine, &snapshot(100));
            engine.tick(&WorldSnapshot {
                cursor: Point { x: 200.0, y: 200.0 },
                verbs: vec![Verb::Grab],
                ..snapshot(100)
            });

            let released = engine.tick(&WorldSnapshot {
                cursor: Point { x: 200.0, y: 200.0 },
                verbs: release.clone(),
                ..snapshot(100)
            });
            assert_eq!(released.state, State::Falling);
            assert_eq!(released.cue, Some(cue), "released with {release:?}");

            let falling = engine.tick(&snapshot(100));
            assert_eq!(falling.cue, None, "and once, after {release:?}");
        }
    }

    /// Menu plays nothing. The tray menu opening is its response, and a
    /// reaction under a context menu would be a sprite gesturing at a list.
    #[test]
    fn a_menu_plays_nothing() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        settle(&mut engine, &snapshot(100));

        let menu_opened = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Menu],
            ..snapshot(100)
        });
        assert_eq!(menu_opened.animation, "idle");
    }

    /// Menu interrupts what the sprite is doing, not where it is going: the
    /// walk it was on carries on through the menu being shown. This matches
    /// Poke: both are interactions that acknowledge the user without stopping
    /// the sprite's motion.
    #[test]
    fn a_menu_mid_stroll_does_not_stop_the_walk() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let under_way = engine.tick(&a_long_perch());
        assert_eq!(under_way.velocity.x, WALK_SPEED);

        let menu_opened = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Menu],
            ..a_long_perch()
        });
        assert_eq!(
            menu_opened.velocity.x, WALK_SPEED,
            "the walk continues while the menu is open"
        );

        let strolling: Vec<Frame> = (0..12).map(|_| engine.tick(&a_long_perch())).collect();
        assert!(
            strolling.iter().all(|frame| frame.velocity.x == WALK_SPEED),
            "and keeps going after the menu is dismissed: {strolling:?}"
        );
    }

    /// Menu during a drag behaves like Poke: the sprite stays grabbed. The
    /// menu blocks, so the drag is paused while it is shown, but releasing the
    /// button after dismissing the menu still ends the Grab.
    #[test]
    fn a_menu_during_a_drag_does_not_drop_the_sprite() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &snapshot(100));

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 100.0, y: 0.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        let dragging = engine.tick(&WorldSnapshot {
            cursor: Point { x: 200.0, y: 0.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        assert_eq!(dragging.state, State::Dragged);

        let menu_opened = engine.tick(&WorldSnapshot {
            cursor: Point { x: 200.0, y: 0.0 },
            verbs: vec![Verb::Menu, Verb::Grab],
            ..snapshot(100)
        });
        assert_eq!(
            menu_opened.state,
            State::Dragged,
            "the sprite stays grabbed while the menu is open"
        );

        let released = engine.tick(&WorldSnapshot {
            cursor: Point { x: 200.0, y: 0.0 },
            verbs: Vec::new(),
            ..snapshot(100)
        });
        assert_eq!(
            released.state,
            State::Falling,
            "releasing after dismissing the menu drops it as usual"
        );
    }

    /// Menu during a chase behaves like Poke during a walk: the chase is
    /// interrupted (aborted), but the sprite's current velocity persists.
    /// This pins the Menu verb's contract against the chase Primitive (#164).
    #[test]
    fn a_menu_during_a_chase_aborts_the_chase() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        engine.play(&[Primitive::Chase]);

        let chasing = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: 500.0,
                y: engine.position.y,
            },
            ..a_long_perch()
        });
        assert_eq!(chasing.animation, "walk", "chase is under way");
        assert!(
            chasing.velocity.x.abs() > 0.0,
            "sprite is moving toward cursor"
        );

        let menu_opened = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: 500.0,
                y: engine.position.y,
            },
            verbs: vec![Verb::Menu],
            ..a_long_perch()
        });
        assert!(
            engine.on_screen() != Some(Primitive::Chase),
            "chase is aborted by Menu"
        );
        assert!(
            menu_opened.velocity.x.abs() > 0.0,
            "but velocity persists like during walk interrupt"
        );
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

    /// #6: a click is a Poke, and a Poke addresses the Director. That is how
    /// the user tests the session: react on screen, and a reactive wake with
    /// `happened: poked`. Dwell (#152) is the other addressing path; a click
    /// must not wait for it.
    #[test]
    fn a_poke_addresses_the_director() {
        let mut engine = Engine::new(Point { x: 500.0, y: 100.0 });
        settle(&mut engine, &snapshot(100));

        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });
        assert!(
            poked.addressed,
            "the Shell wakes the session from Frame.addressed"
        );
        assert_eq!(poked.animation, "react");
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

    /// #277: the Summon that replaced the second click's Poke keeps its stop
    /// and its cooldown too, so a double-click on a strolling sprite holds it
    /// exactly as it did before. Menu is the verb that lets the walk carry on.
    #[test]
    fn a_summon_mid_stroll_stops_the_walk_like_a_poke() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        assert_eq!(engine.tick(&a_long_perch()).velocity.x, WALK_SPEED);

        let summoned = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Summon],
            ..a_long_perch()
        });
        assert_eq!(summoned.velocity.x, 0.0, "the summon stops the feet");

        let held: Vec<Frame> = (0..24).map(|_| engine.tick(&a_long_perch())).collect();
        assert!(
            held.iter()
                .all(|frame| frame.position.x == summoned.position.x),
            "it stays put through the cooldown: {held:?}"
        );
    }

    /// #177: a Poke stops the stroll. Reacting while carrying on walking read
    /// as something that happened to the animation rather than to the
    /// character, so the sprite stands for a beat and only then may move.
    #[test]
    fn a_poke_mid_stroll_stops_the_walk_for_a_beat() {
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
        assert!(
            poked.addressed,
            "a click mid-stroll still tests the Director"
        );
        assert_eq!(poked.velocity.x, 0.0, "the click stops the feet");

        let held: Vec<Frame> = (0..24).map(|_| engine.tick(&a_long_perch())).collect();
        assert!(
            held.iter()
                .all(|frame| frame.position.x == poked.position.x),
            "it stays put through the cooldown: {held:?}"
        );
        assert_eq!(
            held.last().unwrap().animation,
            animation_for(State::Perched),
            "and rests the way a perched sprite does once the reaction is over"
        );
    }

    /// The hold is a pause the Director cannot walk through: a proposal that
    /// moves the sprite is refused until it is over, while one that only
    /// speaks is not — the buddy was just addressed, and answering is fine.
    #[test]
    fn a_walk_proposed_mid_cooldown_waits_and_one_after_it_does_not() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..a_long_perch()
        });
        for _ in 0..10 {
            engine.tick(&a_long_perch());
        }

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let refused = engine.tick(&a_long_perch());
        assert_eq!(
            refused.velocity.x, 0.0,
            "a walk mid-cooldown waits: {refused:?}"
        );
        assert_eq!(refused.position.x, poked.position.x);

        let greeted = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "greet".to_string(),
                dialogue: None,
            }),
            ..a_long_perch()
        });
        assert_eq!(greeted.animation, "react", "speaking mid-cooldown is fine");

        for _ in 0..30 {
            engine.tick(&a_long_perch());
        }
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let resumed = engine.tick(&a_long_perch());
        assert_eq!(
            resumed.velocity.x, WALK_SPEED,
            "after the cooldown a walk is taken up: {resumed:?}"
        );
    }

    /// The hand outranks standing still: a Grab mid-cooldown picks the sprite up
    /// at once, and once it is thrown and lands, nothing of the cooldown is left
    /// to refuse the next walk.
    #[test]
    fn a_grab_mid_cooldown_takes_over_at_once_and_clears_the_cooldown() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..a_long_perch()
        });

        let grabbed = engine.tick(&WorldSnapshot {
            cursor: Point { x: 300.0, y: 200.0 },
            verbs: vec![Verb::Grab],
            ..a_long_perch()
        });
        assert_eq!(grabbed.state, State::Dragged);
        assert_eq!(grabbed.position, Point { x: 300.0, y: 200.0 });

        let thrown = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 0.0, y: 0.0 },
            }],
            ..a_long_perch()
        });
        assert_eq!(thrown.state, State::Falling);
        settle(&mut engine, &a_long_perch());

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let walking = engine.tick(&a_long_perch());
        assert_eq!(
            walking.velocity.x, WALK_SPEED,
            "the hand ended the cooldown, not the clock: {walking:?}"
        );
    }

    /// Losing the ground mid-cooldown is a fall like any other, and the fall
    /// ends the cooldown: the first walk after the landing is not refused.
    #[test]
    fn losing_the_ground_mid_cooldown_falls_at_once_and_clears_the_cooldown() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..a_long_perch()
        });

        let dropped = engine.tick(&snapshot(100));
        assert_eq!(dropped.state, State::Falling, "{dropped:?}");

        settle(&mut engine, &snapshot(100));
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..snapshot(100)
        });
        let walking = engine.tick(&snapshot(100));
        assert_eq!(
            walking.velocity.x, WALK_SPEED,
            "nothing of the cooldown survives the fall: {walking:?}"
        );
    }

    /// The cooldown's edge, pinned to the tick. A tick is 100 ms and the Poke
    /// sets POKE_COOLDOWN_MS after that tick's decrement, so the cooldown is exactly
    /// POKE_COOLDOWN_MS / 100 ticks after the Poke: a walk proposed one tick sooner
    /// is refused, one proposed on that tick is taken up.
    #[test]
    fn the_cooldown_ends_on_the_tick_it_says_it_does() {
        let ticks = (POKE_COOLDOWN_MS / 100) as usize;
        let propose_after = |plain: usize| {
            let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
            settle(&mut engine, &a_long_perch());
            engine.tick(&WorldSnapshot {
                verbs: vec![Verb::Poke],
                ..a_long_perch()
            });
            for _ in 0..plain {
                engine.tick(&a_long_perch());
            }
            engine.tick(&WorldSnapshot {
                proposal: walk(),
                ..a_long_perch()
            });
            engine.tick(&a_long_perch()).velocity.x
        };

        assert_eq!(
            propose_after(ticks - 2),
            0.0,
            "one tick short, still settling"
        );
        assert_eq!(
            propose_after(ticks - 1),
            WALK_SPEED,
            "on the tick, taken up"
        );
    }

    /// The cooldown is one gate for every path that starts motion, not a check
    /// at the proposal alone: a cursor reaction that would walk the sprite
    /// toward the pointer is refused mid-cooldown, and walks once it is over.
    #[test]
    fn a_cursor_reaction_cannot_walk_the_sprite_mid_cooldown() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Toward, CursorReaction::Indifferent);
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });

        let beside = Point {
            x: engine.position.x + 100.0,
            y: engine.position.y,
        };
        let near = || WorldSnapshot {
            cursor: beside,
            ..snapshot(100)
        };
        let approached = engine.tick(&near());
        assert_ne!(approached.animation, "walk", "{approached:?}");
        assert_eq!(
            approached.velocity.x, 0.0,
            "the cursor does not move settling feet"
        );

        for _ in 0..26 {
            engine.tick(&snapshot(100));
        }
        let again = engine.tick(&near());
        assert_eq!(
            again.animation, "walk",
            "once the cooldown is over it walks: {again:?}"
        );
    }

    /// The same cooldown on the display floor: the mechanism is the sprite's,
    /// not the Perch's, and at rest on the floor it shows `idle`.
    #[test]
    fn a_poke_mid_stroll_on_the_floor_stops_the_walk_too() {
        let mut engine = a_character_at(Point { x: 300.0, y: 0.0 });
        settle(&mut engine, &snapshot(100));
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..snapshot(100)
        });
        assert_eq!(engine.tick(&snapshot(100)).velocity.x, WALK_SPEED);

        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..snapshot(100)
        });
        let held: Vec<Frame> = (0..24).map(|_| engine.tick(&snapshot(100))).collect();
        assert!(
            held.iter()
                .all(|frame| frame.position.x == poked.position.x),
            "{held:?}"
        );
        assert_eq!(held.last().unwrap().animation, "idle");
    }

    /// A second Poke restarts the cooldown rather than queuing another one.
    #[test]
    fn a_second_poke_mid_cooldown_restarts_the_cooldown() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());
        let poke = || WorldSnapshot {
            verbs: vec![Verb::Poke],
            ..a_long_perch()
        };

        engine.tick(&poke());
        for _ in 0..20 {
            engine.tick(&a_long_perch());
        }
        engine.tick(&poke());
        for _ in 0..20 {
            engine.tick(&a_long_perch());
        }

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let still_held = engine.tick(&a_long_perch());
        assert_eq!(
            still_held.velocity.x, 0.0,
            "the second Poke started the cooldown over: {still_held:?}"
        );

        for _ in 0..6 {
            engine.tick(&a_long_perch());
        }
        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let released = engine.tick(&a_long_perch());
        assert_eq!(released.velocity.x, WALK_SPEED, "{released:?}");
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
    /// simply been yanked elsewhere in one poll. A slow drag is ridden (#98),
    /// so what drops the sprite here is the speed, not the move.
    #[test]
    fn a_perch_yanked_out_from_under_the_sprite_drops_it() {
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
        assert_eq!(
            up.velocity,
            Point::default(),
            "#85 sub-decision 2: the ride is a position offset, so flinging a \
             window must not launch the sprite ballistically"
        );

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
        assert_eq!(across.velocity, Point::default());

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
        assert_eq!(down.velocity, Point::default());
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

    /// #85 criterion 2: the two sides of the gate on a downward drag. The
    /// slow one is ridden; the fast one outruns the sprite, which is left in
    /// the air and re-lands on the same edge, now below it.
    #[test]
    fn a_slow_descent_is_ridden_and_a_fast_one_leaves_the_sprite_behind() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));

        // 20 points in 100 ms: 200 pt/s, well under the gate.
        let ridden = engine.tick(&perch(50.0, 420.0));
        assert_eq!(ridden.state, State::Perched, "{ridden:?}");
        assert_eq!(ridden.position.y, 420.0);
        assert!(ridden.riding);

        // 200 points in the same poll, and the window is gone from under it.
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        settle(&mut engine, &perch(50.0, 400.0));
        let dropped = engine.tick(&perch(50.0, 600.0));
        assert_eq!(dropped.state, State::Falling, "{dropped:?}");
        assert_eq!(dropped.position.y, 400.0, "left in the air where it stood");

        let landed = settle(&mut engine, &perch(50.0, 600.0));
        assert_eq!(landed.state, State::Perched);
        assert_eq!(landed.position.y, 600.0, "onto the same edge, now below it");
    }

    /// #85 criterion 5: a sideways yank with the edge still plainly under the
    /// sprite. The window keeps spanning where the sprite stands, so the
    /// visibility re-check in `perch_carry` is satisfied throughout and the
    /// gate is the only thing that can drop it — which is what makes the two
    /// guards individually load-bearing rather than covering for each other.
    #[test]
    fn a_sideways_yank_leaves_the_sprite_standing_where_it_was() {
        let wide = |x: f64| WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x,
                    y: 400.0,
                    width: 900.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });
        let perched = settle(&mut engine, &wide(50.0));
        assert_eq!(perched.position, Point { x: 500.0, y: 400.0 });

        // 150 points left in one poll, and the edge still runs under the
        // sprite: -100..800 spans 500. A ride would slide it to 350.
        let dropped = engine.tick(&wide(-100.0));
        assert_eq!(dropped.state, State::Falling, "{dropped:?}");
        assert_eq!(
            dropped.position,
            Point { x: 500.0, y: 400.0 },
            "left standing where it was, not carried along the edge"
        );

        let landed = settle(&mut engine, &wide(-100.0));
        assert_eq!(landed.state, State::Perched);
        assert_eq!(
            landed.position,
            Point { x: 500.0, y: 400.0 },
            "and drops straight back onto the edge under it"
        );
    }

    /// #85 criterion 9 and sub-decision 1: the ride stops where the displays
    /// do. Two displays, the right one short, and the Perch is dragged slowly
    /// down until its edge is below the shorter display's usable floor —
    /// somewhere no display covers, and so not somewhere to stand. The sprite
    /// lets go rather than being carried out there behind the Dock.
    #[test]
    fn a_ride_never_carries_the_sprite_where_no_display_covers() {
        // Nothing covers x 1000..2000 outside y 300..500.
        let world = |y: f64| WorldSnapshot {
            displays: vec![
                one_display(),
                Rect {
                    x: 1000.0,
                    y: 300.0,
                    width: 1000.0,
                    height: 200.0,
                },
            ],
            windows: vec![window(
                1,
                Rect {
                    x: 1000.0,
                    y,
                    width: 600.0,
                    height: 200.0,
                },
            )],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point {
            x: 1200.0,
            y: 350.0,
        });
        assert_eq!(settle(&mut engine, &world(450.0)).position.y, 450.0);

        // 20 points a poll — 200 pt/s, well under the gate — so every step of
        // this is a ride, right up to the one that would leave the displays.
        for y in [470.0, 490.0] {
            let ridden = engine.tick(&world(y));
            assert_eq!(ridden.state, State::Perched, "{ridden:?}");
            assert_eq!(ridden.position.y, y);
        }

        let dropped = engine.tick(&world(510.0));
        assert_eq!(dropped.state, State::Falling, "{dropped:?}");
        assert_eq!(
            dropped.position.y, 490.0,
            "it let go rather than riding to a point no display covers"
        );

        let landed = settle(&mut engine, &world(510.0));
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(
            landed.position.y, 500.0,
            "down to the shorter display's floor"
        );
    }

    /// #128: the sideways half of the same criterion. Vertically the Perch is
    /// what moves, so asking where the sprite stands asks where it will be;
    /// sideways the sprite moves with the edge, and the x it is leaving
    /// answers for a point the ride is about to abandon. It has to be off the
    /// displays on the tick it is drawn there, which is why this watches every
    /// tick rather than settling first.
    #[test]
    fn a_sideways_ride_never_carries_the_sprite_where_no_display_covers() {
        let mut engine = Engine::new(Point {
            x: 1950.0,
            y: 350.0,
        });
        let perched = settle(&mut engine, &strip_perch(1400.0));
        assert_eq!(
            perched.position,
            Point {
                x: 1950.0,
                y: 450.0
            }
        );

        // 20 points a poll — 200 pt/s, well under the gate — dragged right
        // until the sprite's hold on the edge is past x 2000, which the third
        // step is: 1460 plus the 550 it holds at.
        for x in [1420.0, 1440.0, 1460.0, 1480.0] {
            let frame = engine.tick(&strip_perch(x));
            assert!(
                covered(frame.position, &strip_perch(x)),
                "carried off the displays with the window at {x}: {frame:?}"
            );
        }

        // And it stays on a display for every tick of the fall that follows.
        for _ in 0..40 {
            let frame = engine.tick(&strip_perch(1480.0));
            assert!(
                covered(frame.position, &strip_perch(1480.0)),
                "fell off the displays after letting go: {frame:?}"
            );
        }
    }

    /// #128 again, on the other way a ride places the sprite. Between polls
    /// the window list is stale and the sprite coasts on the last Perch
    /// velocity (#98), which no sample gets to approve — so a coast runs off
    /// the displays for as many ticks as the poll is late rather than for one.
    #[test]
    fn a_coast_never_carries_the_sprite_where_no_display_covers() {
        let mut engine = Engine::new(Point {
            x: 1950.0,
            y: 350.0,
        });
        settle(&mut engine, &strip_perch(1400.0));

        // One fresh sample 20 points to the right: 200 pt/s of ride to coast
        // on, and 1990 is the last hold this side of the union's edge.
        let mut sample = strip_perch(1420.0);
        sample.poll_generation = 1;
        assert_eq!(engine.tick(&sample).position.x, 1970.0);

        // The same generation from here on: the assembler has not read again,
        // and 200 pt/s of coast crosses x 2000 on the second tick.
        for _ in 0..8 {
            let frame = engine.tick(&sample);
            assert!(
                covered(frame.position, &sample),
                "coasted off the displays: {frame:?}"
            );
        }
    }

    /// #85 criterion 8: maximizing moves the top edge a long way in one step,
    /// which is a yank by definition. The new edge is perfectly good to stand
    /// on — only the gate catches this — so a naive implementation snaps the
    /// sprite up to it. The sprite is left behind instead.
    #[test]
    fn a_maximized_perch_leaves_the_sprite_behind_rather_than_snapping_it_up() {
        let world = |rect: Rect| WorldSnapshot {
            windows: vec![window(1, rect)],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        assert_eq!(
            settle(
                &mut engine,
                &world(Rect {
                    x: 50.0,
                    y: 400.0,
                    width: 300.0,
                    height: 200.0,
                })
            )
            .position
            .y,
            400.0
        );

        // Zoomed to fill the usable frame: the edge lands clear of the
        // ceiling clearance, so it would be a Perch if the sprite could reach.
        let maximized = engine.tick(&world(Rect {
            x: 0.0,
            y: 130.0,
            width: 1000.0,
            height: 670.0,
        }));
        assert_eq!(maximized.state, State::Falling, "{maximized:?}");
        assert_eq!(
            maximized.position.y, 400.0,
            "not snapped onto the new edge at 130"
        );
    }

    /// #85 criterion 7: a minimized window leaves the window server's
    /// on-screen list, so the Engine sees a close and nothing more. Confirmed
    /// rather than assumed, against a real window server: a window of our own
    /// is in `CGWindowListCopyWindowInfo(.optionOnScreenOnly)` by its
    /// `kCGWindowNumber`, absent while miniaturized, and back on deminiaturize.
    #[test]
    fn a_minimized_perch_drops_the_sprite_as_a_closed_one_does() {
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        assert_eq!(
            settle(&mut engine, &perch(50.0, 400.0)).state,
            State::Perched
        );

        let dropped = engine.tick(&snapshot(100));
        assert_eq!(dropped.state, State::Falling, "{dropped:?}");
        assert_eq!(dropped.position.y, 400.0, "left where it stood");
        assert_eq!(settle(&mut engine, &snapshot(100)).position.y, 800.0);
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
        // #85 criterion 4 wants somewhere to land, so there is a second window
        // below the Perch, clear of its bottom edge and so plainly visible.
        let world = |y: f64| WorldSnapshot {
            windows: vec![
                window(
                    1,
                    Rect {
                        x: 50.0,
                        y,
                        width: 300.0,
                        height: 200.0,
                    },
                ),
                window(
                    2,
                    Rect {
                        x: 50.0,
                        y: 650.0,
                        width: 300.0,
                        height: 150.0,
                    },
                ),
            ],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &world(400.0)).position.y, 400.0);

        // Far enough to exceed the ride gate, near enough that the sprite is
        // still inside the rectangle — the case that used to Lift it. #98.
        let yanked = engine.tick(&world(250.0));
        assert_eq!(yanked.state, State::Falling, "{yanked:?}");
        assert_eq!(
            yanked.position.y, 400.0,
            "it is not carried onto the new edge"
        );

        let landed = settle(&mut engine, &world(250.0));
        assert_eq!(landed.state, State::Perched, "{landed:?}");
        assert_eq!(
            landed.position.y, 650.0,
            "the window passed through it, and it fell to the next Perch below"
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
            covered(landed.position, &world),
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

    /// #84: walk velocity outlives the Primitive that started it, so refusing
    /// the next proposal is not enough. Sit is what stops the feet.
    #[test]
    fn toggling_do_not_disturb_stops_a_walk_and_sits_the_sprite_down() {
        let mut engine = a_character_at(Point { x: 200.0, y: 0.0 });
        settle(&mut engine, &a_long_perch());

        engine.tick(&WorldSnapshot {
            proposal: walk(),
            ..a_long_perch()
        });
        let strolling = engine.tick(&a_long_perch());
        assert_eq!(
            strolling.animation, "walk",
            "precondition: the sprite is walking"
        );
        assert_ne!(
            strolling.velocity.x, 0.0,
            "precondition: the walk has a heading"
        );

        engine.set_do_not_disturb(true);
        let settled = engine.tick(&a_long_perch());

        assert_eq!(settled.animation, "sit");
        assert_eq!(settled.velocity.x, 0.0, "sit is what stops the walk");
        assert_eq!(
            settled.state,
            State::Perched,
            "quiet is sit, not gone and not asleep"
        );

        let rest: Vec<Frame> = (0..20).map(|_| engine.tick(&a_long_perch())).collect();
        assert!(
            rest.iter()
                .all(|frame| frame.velocity.x == 0.0 && frame.animation == "sit"),
            "the walk does not resume: {rest:?}"
        );
    }

    #[test]
    fn a_sprite_standing_at_the_left_edge_faces_right() {
        let mut engine = Engine::new(Point { x: 0.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 0.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        let _thrown_left = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: -500.0, y: 0.0 },
            }],
            ..snapshot(100)
        });

        let at_left_edge = settle(&mut engine, &snapshot(100));
        assert_eq!(
            at_left_edge.state,
            State::Grounded,
            "sprite lands on the floor after hitting left edge"
        );
        assert_eq!(
            at_left_edge.facing, 1.0,
            "sprite at left edge must face right (away from edge)"
        );
    }

    #[test]
    fn a_sprite_standing_at_the_right_edge_faces_left() {
        let mut engine = Engine::new(Point { x: 500.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 500.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        let _thrown_right = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 2000.0, y: 0.0 },
            }],
            ..snapshot(100)
        });

        let at_right_edge = settle(&mut engine, &snapshot(100));
        assert_eq!(
            at_right_edge.state,
            State::Grounded,
            "sprite lands on the floor after hitting right edge"
        );
        assert_eq!(
            at_right_edge.facing, -1.0,
            "sprite at right edge must face left (away from edge)"
        );
    }

    #[test]
    fn a_sprite_at_left_edge_is_fully_on_screen() {
        let mut engine = Engine::new(Point { x: 50.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 50.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: -500.0, y: 0.0 },
            }],
            ..snapshot(100)
        });

        let at_left_edge = settle(&mut engine, &snapshot(100));
        assert_eq!(
            at_left_edge.position.x, EDGE_CLEARANCE,
            "sprite position must be inset by EDGE_CLEARANCE from left edge"
        );
    }

    #[test]
    fn a_sprite_at_right_edge_is_fully_on_screen() {
        let mut engine = Engine::new(Point { x: 950.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 950.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 500.0, y: 0.0 },
            }],
            ..snapshot(100)
        });

        let at_right_edge = settle(&mut engine, &snapshot(100));
        let right_edge = one_display().x + one_display().width;
        assert_eq!(
            at_right_edge.position.x,
            right_edge - EDGE_CLEARANCE,
            "sprite position must be inset by EDGE_CLEARANCE from right edge"
        );
    }

    #[test]
    fn climbing_preserves_wall_centered_frames_and_may_clip() {
        let mut engine = Engine::new(Point { x: 900.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        let climbing = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 2000.0, y: 0.0 },
            }],
            ..snapshot(100)
        });

        assert_eq!(climbing.state, State::Climbing);
        assert_eq!(
            climbing.position.x, 1000.0,
            "during climb, position stays at the wall edge for wall-centered frames"
        );
    }

    #[test]
    fn dragging_to_the_edge_does_not_snap_position_or_facing() {
        let mut engine = Engine::new(Point { x: 900.0, y: 400.0 });

        // Establish facing -1.0 by throwing left and settling at the right edge
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 500.0, y: 0.0 },
            }],
            ..snapshot(100)
        });
        let at_right = settle(&mut engine, &snapshot(100));
        assert_eq!(
            at_right.facing, -1.0,
            "facing left after settling at right edge"
        );

        // Now drag to the left edge
        let dragged = engine.tick(&WorldSnapshot {
            cursor: Point { x: 10.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });

        assert_eq!(dragged.state, State::Dragged);
        assert_eq!(
            dragged.position.x, 10.0,
            "dragged sprite follows cursor exactly, even near edge"
        );
        assert_eq!(
            dragged.facing, -1.0,
            "facing unchanged while dragged; no snap to face away"
        );
    }

    #[test]
    fn after_climb_ends_at_edge_standing_is_inset_and_faces_away() {
        let mut engine = Engine::new(Point { x: 900.0, y: 400.0 });

        engine.tick(&WorldSnapshot {
            cursor: Point { x: 900.0, y: 400.0 },
            verbs: vec![Verb::Grab],
            ..snapshot(100)
        });
        engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Throw {
                velocity: Point { x: 2000.0, y: 0.0 },
            }],
            ..snapshot(100)
        });

        let landed = settle(&mut engine, &snapshot(100));
        assert_eq!(landed.state, State::Grounded);

        let right_edge = one_display().x + one_display().width;
        assert_eq!(
            landed.position.x,
            right_edge - EDGE_CLEARANCE,
            "after climb ends, standing position is inset from edge"
        );
        assert_eq!(
            landed.facing, -1.0,
            "after climb ends, sprite faces away from right edge"
        );
    }

    #[test]
    fn a_perched_sprite_near_the_edge_is_not_shoved_off_its_ledge() {
        // Window near right edge that does NOT include the edge snap position.
        // Right edge snap would be at x=936. Window spans x=950 to x=990.
        let narrow_perch = WorldSnapshot {
            windows: vec![window(
                1,
                Rect {
                    x: 950.0,
                    y: 200.0,
                    width: 40.0,
                    height: 100.0,
                },
            )],
            ..snapshot(100)
        };

        // Start the sprite on the window, away from edges, so no correction yet
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });
        settle(&mut engine, &snapshot(100));

        // Grab and place on the narrow perch near right edge
        engine.tick(&WorldSnapshot {
            cursor: Point { x: 970.0, y: 200.0 },
            verbs: vec![Verb::Grab],
            ..narrow_perch.clone()
        });
        let placed = engine.tick(&WorldSnapshot {
            verbs: vec![],
            ..narrow_perch.clone()
        });

        assert_eq!(placed.state, State::Perched, "sprite is on the perch");
        assert!(
            placed.position.x >= 950.0 && placed.position.x <= 990.0,
            "sprite stays on perch span [950, 990], not moved to x=936"
        );
        assert_eq!(placed.facing, -1.0, "sprite faces away from right edge");
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

        assert_eq!(silent.animation, "react", "greet plays its own animation");
        assert_eq!(silent.dialogue, None);
        assert_eq!(silent.behavior, Some("greet".to_string()));
    }

    // Cursor awareness tests (#152, #153): scripted pointer tracks with no windowing system.

    /// #152: Near reaction with indifferent keeps doing whatever it was doing.
    #[test]
    fn near_indifferent_keeps_the_sprite_doing_whatever_it_was_doing() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Indifferent, CursorReaction::Indifferent);

        // Cursor far away: idle.
        let far = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: 1000.0,
                y: 100.0,
            },
            ..snapshot(16)
        });
        assert_eq!(far.animation, "idle");

        // Cursor enters Near radius: still idle.
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: engine.position.x + 50.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        assert_eq!(near.animation, "idle", "indifferent means no reaction");
    }

    /// #152: Near reaction with speak plays talk.
    #[test]
    fn near_speak_plays_talk_when_cursor_enters_radius() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Speak, CursorReaction::Indifferent);

        // Cursor enters Near radius.
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: engine.position.x + 50.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        assert_eq!(near.animation, "talk", "speak reaction plays talk");
    }

    /// #152: Near reaction with face turns to face the cursor.
    #[test]
    fn near_face_turns_the_sprite_toward_the_cursor() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Face, CursorReaction::Indifferent);
        engine.facing = -1.0; // Start facing left.

        // Cursor enters Near radius to the right.
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: engine.position.x + 50.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        assert_eq!(near.facing, 1.0, "sprite faces right toward the cursor");
    }

    /// #152: Near reaction with toward walks toward the cursor.
    #[test]
    fn near_toward_walks_the_sprite_toward_the_cursor() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Toward, CursorReaction::Indifferent);

        // Cursor enters Near radius to the right.
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: engine.position.x + 100.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        assert_eq!(near.animation, "walk", "toward reaction starts a walk");
        assert_eq!(near.facing, 1.0, "walks toward the cursor");
        assert_eq!(
            near.velocity.x, WALK_SPEED,
            "toward must move right when the cursor is to the right, not merely face it"
        );
    }

    /// #152: Near reaction with away walks away from the cursor.
    #[test]
    fn near_away_walks_the_sprite_away_from_the_cursor() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Away, CursorReaction::Indifferent);

        // Cursor enters Near radius to the right.
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: engine.position.x + 100.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        assert_eq!(near.animation, "walk", "away reaction starts a walk");
        assert_eq!(near.facing, -1.0, "walks away from the cursor");
        assert_eq!(
            near.velocity.x, -WALK_SPEED,
            "away must move left when the cursor is to the right"
        );
    }

    /// #152: Near reaction with react plays react.
    #[test]
    fn near_react_plays_react_when_cursor_enters_radius() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::React, CursorReaction::Indifferent);

        // Cursor enters Near radius.
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: engine.position.x + 50.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        assert_eq!(near.animation, "react", "react reaction plays react");
    }

    /// #152: Rush (startle) plays the rush_reaction once when cursor rushes at sprite.
    #[test]
    fn rush_plays_the_rush_reaction_once_on_high_velocity_approach() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Indifferent, CursorReaction::React);

        // Move cursor quickly toward sprite (high velocity). The first sample is
        // outside Near so `last_cursor` has a position to measure speed from.
        let mut last_x = engine.position.x + 300.0;
        for _ in 0..3 {
            let frame = engine.tick(&WorldSnapshot {
                cursor: Point {
                    x: last_x,
                    y: engine.position.y,
                },
                ..snapshot(16)
            });
            last_x -= 100.0; // Fast approach: 100 points per 16ms = high velocity
            if frame.animation == "react" {
                // Continue for a few more ticks to verify it doesn't repeat.
                for _ in 0..5 {
                    let _staying = engine.tick(&WorldSnapshot {
                        cursor: Point {
                            x: engine.position.x + 50.0,
                            y: engine.position.y,
                        },
                        ..snapshot(16)
                    });
                }
                return;
            }
        }
        panic!("Rush reaction was not triggered");
    }

    /// #152: Rush `toward` walks at the cursor, not away from it. Facing
    /// alone is not enough — the art can face one way and the feet the other.
    #[test]
    fn rush_toward_walks_at_the_cursor_not_away() {
        let mut engine = a_resting_sprite()
            .with_cursor_reactions(CursorReaction::Indifferent, CursorReaction::Toward);

        let start_x = engine.position.x;
        // High-velocity approach from the right, ending still to the right.
        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: start_x + 300.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });
        let rushed = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: start_x + 80.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });

        assert_eq!(rushed.animation, "walk", "rush toward starts a walk");
        assert_eq!(rushed.facing, 1.0, "faces the cursor on the right");
        assert_eq!(
            rushed.velocity.x, WALK_SPEED,
            "feet travel toward the cursor, not away from it"
        );
    }

    /// #153: Chase steers toward cursor's x, swats on arrival, disengages.
    #[test]
    fn chase_walks_toward_cursor_and_swats_on_arrival() {
        let mut engine = a_resting_sprite();
        let sprite_x = engine.position.x;

        // Start chase with cursor close by (just outside arrival threshold), so
        // the first tick is pursuit and not a swat.
        engine.play(&[Primitive::Chase]);

        // First tick: should be chasing (walking).
        let chasing = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + CHASE_ARRIVAL_THRESHOLD + 10.0,
                y: engine.position.y,
            },
            ..snapshot(100)
        });
        assert_eq!(chasing.animation, "walk", "chase uses walk art");
        assert_eq!(chasing.facing, 1.0, "faces toward cursor");

        // Move cursor very close (within arrival threshold) or let sprite walk close enough.
        for _ in 0..20 {
            let frame = engine.tick(&WorldSnapshot {
                cursor: Point {
                    x: sprite_x + 50.0, // Fixed target within walking distance
                    y: engine.position.y,
                },
                ..snapshot(100)
            });

            // When sprite gets close enough, it should swat.
            if frame.animation == "react" {
                return;
            }
        }

        panic!(
            "Chase did not swat on arrival. Sprite position: {}, cursor at: {}",
            engine.position.x,
            sprite_x + 50.0
        );
    }

    /// #153: Chase times out if cursor never arrives.
    #[test]
    fn chase_times_out_if_cursor_escapes() {
        let mut engine = a_resting_sprite();
        let sprite_x = engine.position.x;

        engine.play(&[Primitive::Chase]);

        // Chase a cursor that keeps escaping (moving away).
        let mut cursor_x = sprite_x + 300.0;
        let mut ticks = 0;
        while ticks < (CHASE_TIMEOUT_MS / 100) + 5 {
            let _frame = engine.tick(&WorldSnapshot {
                cursor: Point {
                    x: cursor_x,
                    y: engine.position.y,
                },
                ..snapshot(100)
            });
            cursor_x += 50.0; // Stays ahead so arrival never fires; timeout is the path under test.
            ticks += 1;

            // After timeout, chase should give up.
            if ticks > (CHASE_TIMEOUT_MS / 100) && engine.on_screen() != Some(Primitive::Chase) {
                return;
            }
        }
        panic!("Chase did not time out as expected");
    }

    /// #153: Any Verb aborts chase.
    #[test]
    fn chase_aborts_on_any_verb() {
        let mut engine = a_resting_sprite();
        engine.play(&[Primitive::Chase]);

        // Poke aborts chase.
        let poked = engine.tick(&WorldSnapshot {
            verbs: vec![Verb::Poke],
            cursor: Point {
                x: engine.position.x + 200.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });

        assert_eq!(
            poked.animation, "react",
            "Poke aborts chase and plays react"
        );
        assert!(
            engine.on_screen() != Some(Primitive::Chase),
            "chase is no longer playing"
        );
    }

    /// #153: Chase never plays under Do Not Disturb.
    #[test]
    fn chase_is_refused_under_do_not_disturb() {
        let mut engine = a_resting_sprite();
        engine.set_do_not_disturb(true);

        // Propose a behavior with chase.
        let frame = engine.tick(&WorldSnapshot {
            proposal: Some(BehaviorProposal {
                behavior: "chase-test".to_string(),
                dialogue: None,
            }),
            cursor: Point {
                x: engine.position.x + 200.0,
                y: engine.position.y,
            },
            ..snapshot(16)
        });

        assert_eq!(frame.animation, "idle", "DND refuses chase proposals");
    }

    /// #152: Near and Rush reactions still play under Do Not Disturb (like Poke).
    #[test]
    fn cursor_reactions_play_under_do_not_disturb() {
        let mut engine =
            a_resting_sprite().with_cursor_reactions(CursorReaction::Speak, CursorReaction::React);
        engine.set_do_not_disturb(true);

        let sprite_x = engine.position.x;
        let sprite_y = engine.position.y;

        // Start with cursor far away. Seeds `last_cursor` so the enter below is
        // a crossing, not a spawn.
        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 500.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });

        // Slowly approach to avoid triggering Rush. Outside Near, speed cannot
        // Rush at all; the last step (160 → 149) is 11 points in 16 ms =
        // 687 pt/s, under `RUSH_VELOCITY`.
        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 450.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });

        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 400.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });

        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 250.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });

        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 200.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });

        engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 160.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });

        // Near reaction still plays under DND when cursor enters radius (NEAR_RADIUS = 150).
        let near = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 149.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });
        assert_eq!(
            near.animation, "talk",
            "Near reaction (speak) plays under DND (answers the user)"
        );
    }

    /// #152: Dwell addresses the Director when the cursor rests on the sprite
    /// for the threshold duration.
    #[test]
    fn dwell_addresses_the_director() {
        let mut engine = a_resting_sprite();
        let sprite_x = engine.position.x;
        let sprite_y = engine.position.y;

        // Cursor moves onto the sprite.
        let on_sprite = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x,
                y: sprite_y,
            },
            ..snapshot(16)
        });
        assert!(!on_sprite.addressed, "not addressed yet");

        // Cursor rests for half the dwell threshold.
        let dwelling = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x,
                y: sprite_y,
            },
            elapsed_ms: DWELL_MS / 2,
            ..snapshot(16)
        });
        assert!(!dwelling.addressed, "still not addressed yet");

        // Cursor continues to rest, crossing the threshold.
        let addressed = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x,
                y: sprite_y,
            },
            elapsed_ms: DWELL_MS / 2 + 1,
            ..snapshot(16)
        });
        assert!(addressed.addressed, "addressed after dwell threshold");
        assert_eq!(addressed.animation, "talk", "plays talk's first moment");

        // Continuing to dwell doesn't address again.
        let still_dwelling = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x,
                y: sprite_y,
            },
            ..snapshot(16)
        });
        assert!(
            !still_dwelling.addressed,
            "addressed only once per dwell session"
        );
    }

    /// #152: A cursor that only passes through the sprite without dwelling
    /// does not address the Director.
    #[test]
    fn passing_cursor_does_not_address() {
        let mut engine = a_resting_sprite();
        let sprite_x = engine.position.x;
        let sprite_y = engine.position.y;

        // Cursor approaches and passes over the sprite quickly.
        let approach = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x - 50.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });
        assert!(!approach.addressed, "not addressed while approaching");

        let over = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x,
                y: sprite_y,
            },
            elapsed_ms: 16,
            ..snapshot(16)
        });
        assert!(
            !over.addressed,
            "not addressed: only on sprite for 16ms, below threshold"
        );

        let past = engine.tick(&WorldSnapshot {
            cursor: Point {
                x: sprite_x + 50.0,
                y: sprite_y,
            },
            ..snapshot(16)
        });
        assert!(!past.addressed, "not addressed after passing through");
    }
}
