//! The Engine: `WorldSnapshot` in, `Frame` out, once per tick.
//!
//! Pure and synchronous. It reads no clock, holds no timers and performs no
//! I/O, so time reaches it only as elapsed milliseconds on a snapshot. That is
//! what lets every spatial property be tested by constructing snapshots and
//! asserting frames, with no windowing system, no model and no waiting.

use crate::character::{Behavior, Primitive};
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
    /// The Behavior that started playing on this frame, if a proposal was
    /// taken. An event like `dialogue`, and for the Shell rather than the
    /// renderer: a proposal is advisory, so what the Director suggested and
    /// what the user actually saw are different lists, and repetition is
    /// suppressed on the second.
    pub behavior: Option<String>,
}

/// How long one Primitive holds the screen.
///
/// ponytail: one duration for every Primitive of every Character. The Engine
/// deliberately does not know an Animation's length — fps and loop mode are the
/// Character Manifest's — so it cannot play a Primitive until the art runs out.
/// Art shorter than the turn costs nothing: `loop = once` holds its last frame
/// for the remainder, which is what a brief startle looks like. Longer art is
/// the ceiling, and the placeholder already sits on it — `fps sleep = 1` over
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
const GRAVITY: f64 = 1800.0;

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
    /// contain the sprite from one that contained it all along. See `footing`.
    previous_windows: Vec<Rect>,
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
            playing: Vec::new(),
            primitive_ms: 0,
            facing: 1.0,
        }
    }

    /// The Behaviors this Character declares. Nothing else reaches the Engine
    /// from a Character Package: art is the renderer's, and a Behavior is
    /// Primitives the Engine already owns.
    pub fn with_behaviors(mut self, behaviors: BTreeMap<String, Behavior>) -> Self {
        self.behaviors = behaviors;
        self
    }

    pub fn tick(&mut self, snapshot: &WorldSnapshot) -> Frame {
        let dt = f64::from(snapshot.elapsed_ms) / 1000.0;

        // Where it stood before this tick moved it, which is half of what says
        // whether a window has come to contain it. See `footing`.
        let previously = self.position;

        // Set by a sprite that is woken, so the footing it is put back on is
        // not mistaken for one it arrived at. See the landing below.
        let mut woke = false;

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
                woke = true;
            }
        }

        // Being addressed is not being left alone, so a proposal holds off the
        // sleep timer without waking a sprite that has already nodded off —
        // only a verb does that. The timer is otherwise still running when the
        // Behavior is played at the end of the tick, and a sprite that nods off
        // first is asleep when the gate reads its State. #5.
        if snapshot.proposal.is_some() {
            self.idle_ms = 0;
        }

        // What was already playing ages before anything new starts, so a
        // Primitive begun this tick gets its whole turn rather than losing this
        // tick's milliseconds to the one it replaced.
        let mut started = self.advance(snapshot.elapsed_ms);
        let mut landed = false;

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

        // Walking is the Engine's, deciding to walk is not: nothing else here
        // moves the sprite of its own accord. A walk needs no ending — it lasts
        // until the sprite runs out of Perch, which is the whole point of it —
        // so the velocity holds when the Behavior that started it is over, and
        // through a Poke's reaction, which interrupts what the sprite is doing
        // rather than where it is going. What does stop it is a Primitive that
        // is the sprite standing still: `walk sit` would otherwise slide along
        // the edge it sat down on.
        if matches!(self.state, State::Grounded | State::Perched) {
            match self.on_screen() {
                Some(Primitive::Walk) => self.velocity.x = self.facing * WALK_SPEED,
                Some(Primitive::Idle | Primitive::Sit | Primitive::Sleep) => self.velocity.x = 0.0,
                _ => {}
            }
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
                            landed = true;
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
            // moves, closes or resizes — or when the sprite walks off the end
            // of it — the sprite is in the air again, carrying whatever speed
            // it walked off with.
            State::Grounded | State::Perched | State::Asleep => {
                self.position.x += self.velocity.x * dt;

                match footing(self.position, previously, snapshot, &self.previous_windows) {
                    Some(footing) if footing.y < self.position.y => {
                        self.position.y = footing.y;
                        self.state = footing.state;
                    }
                    Some(footing) if footing.y == self.position.y => {
                        // Still moving is still awake. A walk proposed just
                        // before the timer comes due would otherwise leave the
                        // sprite gliding along the edge playing `sleep`.
                        if self.idle_ms >= SLEEP_AFTER_MS && self.velocity.x == 0.0 {
                            self.state = State::Asleep;
                        }
                    }
                    _ => self.state = State::Falling,
                }
            }
            State::Dragged => {}
        }

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
        if landed && !woke {
            started |= self.play(&[Primitive::Land]);
        }

        // A proposal is advisory, so a Behavior this Character does not declare
        // is refused rather than reported, and refusing it interrupts nothing.
        //
        // After the sprite has been moved, so the State the gate reads is the
        // one the tick ends in. A walk therefore takes its first step on the
        // tick after the proposal, which is what SPEC.md asks for.
        let mut behavior = None;
        if let Some(proposal) = &snapshot.proposal {
            if let Some(primitives) = self.chain(&proposal.behavior) {
                if self.play(&primitives) {
                    started = true;
                    behavior = Some(proposal.behavior.clone());
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
            behavior,
        }
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

/// What a resting sprite is standing on: its Perch, unless a window in front of
/// that Perch has come to contain it.
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
    previously: Point,
    snapshot: &WorldSnapshot,
    previous_windows: &[Rect],
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
        .position(|window| window.spans_x(position.x) && window.y == position.y)?;

    snapshot
        .windows
        .iter()
        // Only a window in front of the Perch can swallow the sprite. What is
        // behind the Perch is behind the sprite too, so the edge it stands on
        // is still there to be seen. Windows arrive frontmost first, so the
        // Perch's own place in that order is where the candidates stop.
        .take(perch)
        .filter(|window| swallows(window, position))
        // And only a window that has *come to* contain it: dragged over the
        // sprite, or walked into where two windows overlap. A window that
        // already had the sprite inside it is not swallowing it — a maximized
        // window contains every smaller window in front of it, so the sprite
        // is inside one from the moment it lands, and raising that window
        // would otherwise fling the sprite to the top of the screen and keep
        // it there. #78.
        .filter(|window| {
            !previous_windows
                .iter()
                .any(|before| before == *window && swallows(before, previously))
        })
        .map(|window| Support {
            y: window.y,
            state: State::Perched,
        })
        .chain(support_below(position, snapshot))
        .min_by(|a, b| a.y.total_cmp(&b.y))
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

    /// The Behaviors the placeholder Character declares: a greeting that chains
    /// into settling down, and a stroll.
    fn declared_behaviors() -> BTreeMap<String, Behavior> {
        BTreeMap::from([
            (
                "walk".to_string(),
                Behavior {
                    primitives: vec![Primitive::Walk],
                    then: None,
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

    /// #8: `land` is the last of the eight required Animations nothing could
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
        let window = Rect {
            x: 50.0,
            y: 400.0,
            width: 300.0,
            height: 200.0,
        };
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
        assert_eq!(climbing.animation, "walk", "it goes on climbing");
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
        let perch = Rect {
            x: 0.0,
            y: 400.0,
            width: 1000.0,
            height: 200.0,
        };
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
            windows: vec![Rect {
                x: 100.0,
                y: 400.0,
                width: 800.0,
                height: 200.0,
            }],
            ..snapshot(100)
        }
    }

    /// The Director asking for a walk, by the name the placeholder Character
    /// declares for a Behavior of one `walk` Primitive.
    fn walk() -> Option<BehaviorProposal> {
        Some(BehaviorProposal {
            behavior: "walk".to_string(),
            dialogue: None,
        })
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
            windows: vec![Rect {
                x: 0.0,
                y: 100.0,
                width: 1000.0,
                height: 800.0,
            }],
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
                Rect {
                    x: 0.0,
                    y: 100.0,
                    width: 1000.0,
                    height: 150.0,
                },
                Rect {
                    x: 0.0,
                    y: 400.0,
                    width: 1000.0,
                    height: 200.0,
                },
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

    /// The sibling of the window closing: the window is still there, it has
    /// simply been dragged elsewhere. Nothing but its position changes, so a
    /// rule written against the window list rather than against what is under
    /// the sprite would keep it standing on thin air.
    #[test]
    fn the_sprite_falls_when_its_perch_moves_out_from_under_it() {
        let window = |x: f64| WorldSnapshot {
            windows: vec![Rect {
                x,
                y: 400.0,
                width: 300.0,
                height: 200.0,
            }],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 100.0, y: 0.0 });

        let perched = settle(&mut engine, &window(50.0));
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0);

        // The same window, dragged out from under it rather than closed.
        let dropped = engine.tick(&window(600.0));
        assert_eq!(dropped.state, State::Falling);

        let landed = settle(&mut engine, &window(600.0));
        assert_eq!(landed.state, State::Grounded);
        assert_eq!(landed.position.y, 800.0, "down to the floor it left");
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
                Rect {
                    x: cover_x,
                    y: 100.0,
                    width: 900.0,
                    height: 500.0,
                },
                Rect {
                    x: perch_x,
                    y: 400.0,
                    width: 300.0,
                    height: 200.0,
                },
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
            position.x > window.x
                && position.x < window.x + window.width
                && position.y > window.y
                && position.y < window.bottom()
        })
    }

    /// #5: the sprite is never left inside a window rectangle. The overlay is
    /// always on top, so a sprite standing inside one is not hidden by it — it
    /// is drawn floating in the middle of the window, sitting on nothing.
    #[test]
    fn a_window_dragged_over_the_sprite_lifts_it_onto_its_edge() {
        let perch = Rect {
            x: 0.0,
            y: 400.0,
            width: 1000.0,
            height: 200.0,
        };
        let resting = WorldSnapshot {
            windows: vec![perch],
            ..snapshot(100)
        };
        let mut engine = Engine::new(Point { x: 500.0, y: 0.0 });
        assert_eq!(settle(&mut engine, &resting).position.y, 400.0);

        // A second window is dragged over it, swallowing the edge it stands on.
        let covered = WorldSnapshot {
            windows: vec![
                Rect {
                    x: 200.0,
                    y: 200.0,
                    width: 800.0,
                    height: 400.0,
                },
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

    /// #78, and the other side of the rule above: a window only swallows the
    /// sprite when it is drawn in front of the Perch the sprite stands on. This
    /// one is dragged across the desktop behind that Perch, so the edge under
    /// the sprite stays in plain sight and the sprite stays on it.
    #[test]
    fn a_window_behind_the_perch_does_not_swallow_the_sprite() {
        let perch = Rect {
            x: 200.0,
            y: 400.0,
            width: 400.0,
            height: 200.0,
        };
        // Frontmost first: the Perch is in front, the dragged window behind it.
        let world = |x: f64| WorldSnapshot {
            windows: vec![
                perch,
                Rect {
                    x,
                    y: 100.0,
                    width: 900.0,
                    height: 500.0,
                },
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

    /// #78 as the overlay harness met it: a maximized window fills the display,
    /// so every smaller window is inside it and so is a sprite standing on one.
    /// That window has swallowed nothing — it contained the sprite before the
    /// sprite landed — and lifting the sprite out of it puts it on the top edge
    /// of the display, where it stays for the rest of the session.
    #[test]
    fn a_window_the_sprite_landed_inside_never_lifts_it_out() {
        let world = || WorldSnapshot {
            windows: vec![
                // Maximized and frontmost, its top edge under the menu bar.
                Rect {
                    x: 0.0,
                    y: 30.0,
                    width: 1000.0,
                    height: 730.0,
                },
                Rect {
                    x: 200.0,
                    y: 400.0,
                    width: 400.0,
                    height: 200.0,
                },
            ],
            ..snapshot(100)
        };
        // Below the maximized window's top edge, as a sprite that starts in the
        // middle of the usable frame is.
        let mut engine = Engine::new(Point { x: 400.0, y: 150.0 });

        let perched = settle(&mut engine, &world());
        assert_eq!(perched.state, State::Perched);
        assert_eq!(perched.position.y, 400.0, "the window it fell onto");

        let frames: Vec<Frame> = (0..10).map(|_| engine.tick(&world())).collect();
        assert!(
            frames
                .iter()
                .all(|frame| frame.position == perched.position),
            "and it stays there rather than climbing to the menu bar: {frames:?}"
        );
    }

    /// The same rule met by walking rather than by a window moving: the sprite
    /// strolls along one edge into the middle of a window that overlaps it.
    #[test]
    fn a_walk_under_an_overlapping_window_steps_up_onto_it() {
        let overlapping = || WorldSnapshot {
            windows: vec![
                Rect {
                    x: 500.0,
                    y: 250.0,
                    width: 500.0,
                    height: 400.0,
                },
                Rect {
                    x: 0.0,
                    y: 400.0,
                    width: 1000.0,
                    height: 200.0,
                },
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
                Rect {
                    x: 0.0,
                    y: 300.0,
                    width: 600.0,
                    height: 600.0,
                },
                Rect {
                    x: 200.0,
                    y: 500.0,
                    width: 600.0,
                    height: 300.0,
                },
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
}
