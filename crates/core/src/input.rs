//! Turning a pointer into interaction verbs.
//!
//! The Shell knows two things each tick: where the cursor is, and whether the
//! primary button is down. This turns that pair into the verbs the Engine
//! understands, and it is pure so that every gesture can be tested by writing
//! one down rather than by performing it.
//!
//! A press over the sprite is ambiguous until it ends. Held still and released,
//! it was a Poke. Moved, or held long enough, it was the beginning of a Grab.
//! Deciding immediately would make every click yank the sprite to the cursor
//! and drop it, which is the difference between a companion you can prod and
//! one you cannot touch without moving.
//!
//! Four of the five verbs are decided here. Menu is not: #18 owns the menu
//! itself and the tray icon that has to open the same one, so the right-click
//! that opens it arrives with them rather than ahead of them.

use crate::engine::{Point, Verb};

/// How far the cursor travels before a press becomes a Grab, in points.
///
/// A tuning knob. Large enough that a click with an unsteady hand stays a
/// click, small enough that picking the sprite up feels immediate.
const DRAG_THRESHOLD: f64 = 4.0;

/// How long a press is held before it becomes a Grab without moving at all.
///
/// A tuning knob, and the other half of the same decision: pressing and
/// holding is how you pick something up when you do not want to move it yet.
const DRAG_DELAY_MS: u32 = 180;

/// How close together two clicks must be to count as one double-click, in
/// milliseconds.
///
/// A tuning knob, near the macOS default. Long enough for a deliberate
/// double-click with an ordinary hand, short enough that prodding the sprite
/// twice because it was fun stays two Pokes.
const DOUBLE_CLICK_MS: u32 = 400;

/// How fast the hand must still be moving for a release to be a Throw, in
/// points per second.
///
/// A tuning knob, and the difference between putting the sprite down and
/// flinging it. Nobody holds a mouse perfectly still, so testing for no
/// movement at all would make every release a throw at whatever speed the
/// last twitch happened to be.
const THROW_MIN_SPEED: f64 = 80.0;

/// How long a throw's velocity is measured over, in milliseconds.
///
/// A tuning knob. Measuring the last tick alone makes the throw hostage to one
/// jittery sample, and measuring the whole gesture turns a flick at the end
/// into an average that ignores it.
const VELOCITY_WINDOW_MS: u32 = 60;

/// What the pointer is currently doing to the sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The button is up, or was pressed somewhere that is not the sprite.
    Idle,
    /// Pressed on the sprite, and not yet known to be a Poke or a Grab.
    Pressed,
    /// Holding the sprite.
    Grabbing,
}

/// One tick of pointer input.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    phase: Phase,
    /// The button last tick, so a press is a transition rather than a state.
    /// Without this, dragging a window under a resting sprite picks the sprite
    /// up the moment the cursor crosses it.
    was_held: bool,
    /// Where the press began, so travel is measured from it rather than from
    /// the previous tick — a slow drag never moves far in 16ms.
    pressed_at: Point,
    pressed_ms: u32,
    /// `None` until the first tick. Assuming an origin would make the first
    /// tick's travel the whole cursor position, and a Grab that began inside
    /// the velocity window would be thrown at a speed nobody moved at.
    cursor: Option<Point>,
    /// Cursor travel and the time it took, in two buckets. The older one is
    /// kept so that the window never empties: a release lands on whatever tick
    /// it lands on, and one that arrived just after a reset would otherwise
    /// measure a flick as stillness.
    travel: Point,
    travel_ms: u32,
    prior: Point,
    prior_ms: u32,
    /// Since the last click, so the next one can tell a double-click from a
    /// second prod. `u32::MAX` means there is no click to pair with.
    since_click_ms: u32,
    /// Whether this run of clicks has already summoned. Spending the pair
    /// alone would only re-arm it on the click after, so drumming on the
    /// sprite would open a chat surface every second click.
    summoned: bool,
}

impl Default for Pointer {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            was_held: false,
            pressed_at: Point::default(),
            pressed_ms: 0,
            cursor: None,
            travel: Point::default(),
            travel_ms: 0,
            prior: Point::default(),
            prior_ms: 0,
            since_click_ms: u32::MAX,
            summoned: false,
        }
    }
}

impl Pointer {
    /// The verbs this tick, given where the cursor is and whether the primary
    /// button is down.
    ///
    /// `over_sprite` is the Shell's alpha hit-test. It decides whether a press
    /// belongs to the sprite at all; once a Grab is under way the cursor is
    /// free to leave the art, which is what dragging is.
    pub fn update(
        &mut self,
        over_sprite: bool,
        held: bool,
        cursor: Point,
        elapsed_ms: u32,
    ) -> Vec<Verb> {
        let moved = match self.cursor {
            Some(previous) => Point {
                x: cursor.x - previous.x,
                y: cursor.y - previous.y,
            },
            None => Point::default(),
        };
        self.cursor = Some(cursor);
        self.sample(moved, elapsed_ms);
        self.since_click_ms = self.since_click_ms.saturating_add(elapsed_ms);

        let pressed = held && !self.was_held;
        self.was_held = held;

        match (self.phase, held) {
            // Pressing anywhere else is somebody else's click, and a button
            // already down when the cursor arrives was never pressed here.
            (Phase::Idle, true) if over_sprite && pressed => {
                self.phase = Phase::Pressed;
                self.pressed_at = cursor;
                self.pressed_ms = 0;
                Vec::new()
            }
            (Phase::Idle, _) => Vec::new(),

            (Phase::Pressed, true) => {
                self.pressed_ms = self.pressed_ms.saturating_add(elapsed_ms);
                if self.dragging_yet() {
                    self.phase = Phase::Grabbing;
                    // A gesture between two clicks is not a double-click.
                    self.since_click_ms = u32::MAX;
                    vec![Verb::Grab]
                } else {
                    Vec::new()
                }
            }
            // Let go without ever becoming a drag: a click, and so a Poke.
            //
            // Where the sprite is now does not come into it. It moves on its
            // own — it can fall out from under a held press — and a click that
            // landed on it is a click on it. Requiring the release to be over
            // the art as well would make a falling sprite impossible to prod.
            //
            // Twice in quick succession it is also a Summon. The Poke stays:
            // swallowing the reaction to the second click while waiting to see
            // whether a third arrives would leave the sprite looking dead for
            // as long as it took to decide.
            (Phase::Pressed, false) => {
                self.phase = Phase::Idle;
                let paired = self.since_click_ms <= DOUBLE_CLICK_MS;
                self.since_click_ms = 0;
                // A gap ends the run, and the next pair may summon again.
                self.summoned &= paired;
                if paired && !self.summoned {
                    self.summoned = true;
                    vec![Verb::Poke, Verb::Summon]
                } else {
                    vec![Verb::Poke]
                }
            }

            (Phase::Grabbing, true) => vec![Verb::Grab],
            // A Throw only when the hand was moving. Released still, the sprite
            // simply drops, which the Engine does for any Grab that stops.
            (Phase::Grabbing, false) => {
                self.phase = Phase::Idle;
                let velocity = self.velocity();
                if velocity.x.hypot(velocity.y) < THROW_MIN_SPEED {
                    Vec::new()
                } else {
                    vec![Verb::Throw { velocity }]
                }
            }
        }
    }

    /// Whether the sprite is being held.
    ///
    /// The Shell suspends its hit-test while this is true.
    pub fn grabbing(&self) -> bool {
        self.phase == Phase::Grabbing
    }

    /// Whether this pointer is in the middle of a gesture at all, held or not
    /// yet decided.
    ///
    /// What `press_target` arbitrates on, and wider than `grabbing` on purpose:
    /// a press that has not yet become a Grab still belongs to the Instance it
    /// landed on. Asking only about a held Grab would let the cursor slide off
    /// a pressed sprite onto its neighbour and start a second gesture there,
    /// leaving one press picking up two Instances.
    pub fn gesturing(&self) -> bool {
        self.phase != Phase::Idle
    }

    fn dragging_yet(&self) -> bool {
        let travelled = self.cursor.is_some_and(|cursor| {
            (cursor.x - self.pressed_at.x).hypot(cursor.y - self.pressed_at.y) >= DRAG_THRESHOLD
        });
        travelled || self.pressed_ms >= DRAG_DELAY_MS
    }

    /// Accumulate recent travel, rolling the older half out as it ages.
    ///
    /// Two buckets rather than a ring of samples: the window is allowed to be
    /// anywhere between half and all of `VELOCITY_WINDOW_MS`, which is close
    /// enough for a throw and is a great deal less bookkeeping.
    fn sample(&mut self, moved: Point, elapsed_ms: u32) {
        if self.travel_ms >= VELOCITY_WINDOW_MS / 2 {
            self.prior = self.travel;
            self.prior_ms = self.travel_ms;
            self.travel = Point::default();
            self.travel_ms = 0;
        }
        self.travel.x += moved.x;
        self.travel.y += moved.y;
        self.travel_ms = self.travel_ms.saturating_add(elapsed_ms);
    }

    /// Points per second, from the recent travel.
    fn velocity(&self) -> Point {
        let millis = self.travel_ms.saturating_add(self.prior_ms);
        if millis == 0 {
            return Point::default();
        }
        let seconds = f64::from(millis) / 1000.0;
        Point {
            x: (self.travel.x + self.prior.x) / seconds,
            y: (self.travel.y + self.prior.y) / seconds,
        }
    }
}

/// Which Instance the pointer is acting on, given what the cursor is over and
/// which Instance is already being held.
///
/// One cursor and several sprites need an arbiter, and every Instance holding
/// its own `Pointer` is not one: fed the same hit-test, two overlapping sprites
/// would both be picked up by one press. This decides first and each `Pointer`
/// is then told whether the press was its own, so at most one Instance is ever
/// in a gesture.
///
/// `hits` is the alpha hit-test per Instance, in the order they are drawn, and
/// `holding` is the index of the Instance in the middle of a Grab.
pub fn press_target(hits: &[bool], holding: Option<usize>) -> Option<usize> {
    // A held Instance keeps the press wherever the cursor goes. This is #6's
    // rule — a drag that outruns the art must not be dropped — and with
    // several sprites it says something more: the drag must not pass to a
    // sprite the cursor happened to cross on its way.
    //
    // An index past the end is an Instance dismissed mid-drag. The press falls
    // back to the cursor rather than to whichever Instance shuffled into that
    // position.
    if holding.is_some_and(|index| index < hits.len()) {
        return holding;
    }

    // The last hit, not the first: sprites are drawn in roster order, so the
    // last one drawn is the one on top, and the one on top is the one the user
    // sees under the cursor.
    hits.iter().rposition(|hit| *hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    /// A tick of the frame loop, at roughly the rate it runs.
    const TICK: u32 = 16;

    /// A whole click: press and release without moving.
    fn click(pointer: &mut Pointer) -> Vec<Verb> {
        pointer.update(true, true, at(100.0, 100.0), TICK);
        pointer.update(true, false, at(100.0, 100.0), TICK)
    }

    /// Waits out the double-click interval with the button up.
    fn pause(pointer: &mut Pointer) {
        for _ in 0..DOUBLE_CLICK_MS / TICK + 2 {
            pointer.update(true, false, at(100.0, 100.0), TICK);
        }
    }

    /// #6: two clicks in quick succession are a Summon, the deliberate act that
    /// opens the chat surface.
    ///
    /// The second click is still a Poke. A double-click is two clicks, and
    /// swallowing the reaction to the first would leave the sprite looking dead
    /// for as long as it took to decide.
    #[test]
    fn two_quick_clicks_poke_twice_and_summon() {
        let mut pointer = Pointer::default();

        assert_eq!(click(&mut pointer), vec![Verb::Poke]);
        assert_eq!(click(&mut pointer), vec![Verb::Poke, Verb::Summon]);
    }

    /// The interval is the whole of what separates prodding the sprite twice
    /// from asking it for the chat surface.
    #[test]
    fn two_clicks_further_apart_are_two_pokes() {
        let mut pointer = Pointer::default();

        assert_eq!(click(&mut pointer), vec![Verb::Poke]);
        pause(&mut pointer);
        assert_eq!(click(&mut pointer), vec![Verb::Poke]);
    }

    /// A pair is spent for the whole run of clicks, not re-armed by the next
    /// one. Drumming on the sprite would otherwise open a chat surface every
    /// second click, and #17 makes that a window each time.
    #[test]
    fn drumming_on_the_sprite_summons_once() {
        let mut pointer = Pointer::default();

        let drummed: Vec<Vec<Verb>> = (0..6).map(|_| click(&mut pointer)).collect();

        assert_eq!(
            drummed,
            vec![
                vec![Verb::Poke],
                vec![Verb::Poke, Verb::Summon],
                vec![Verb::Poke],
                vec![Verb::Poke],
                vec![Verb::Poke],
                vec![Verb::Poke],
            ]
        );
    }

    /// And re-arms once the drumming stops: two clicks after a pause are a
    /// fresh double-click, not part of the run that already summoned.
    #[test]
    fn a_pause_re_arms_the_double_click() {
        let mut pointer = Pointer::default();
        click(&mut pointer);
        assert_eq!(click(&mut pointer), vec![Verb::Poke, Verb::Summon]);
        click(&mut pointer);

        pause(&mut pointer);

        assert_eq!(click(&mut pointer), vec![Verb::Poke]);
        assert_eq!(click(&mut pointer), vec![Verb::Poke, Verb::Summon]);
    }

    /// Clicking, picking the sprite up and putting it down, then clicking again
    /// is two clicks with a gesture between them, not a double-click.
    #[test]
    fn a_grab_between_two_clicks_is_not_a_double_click() {
        let mut pointer = Pointer::default();
        assert_eq!(click(&mut pointer), vec![Verb::Poke]);

        pointer.update(true, true, at(100.0, 100.0), TICK);
        assert_eq!(
            pointer.update(true, true, at(120.0, 100.0), TICK),
            vec![Verb::Grab]
        );
        pointer.update(true, false, at(120.0, 100.0), TICK);

        assert_eq!(click(&mut pointer), vec![Verb::Poke]);
    }

    #[test]
    fn a_click_on_the_sprite_is_a_poke_and_never_moves_it() {
        let mut pointer = Pointer::default();

        assert_eq!(pointer.update(true, true, at(100.0, 100.0), TICK), vec![]);
        assert_eq!(
            pointer.update(true, true, at(100.0, 100.0), TICK),
            vec![],
            "a press alone commits to nothing"
        );

        assert_eq!(
            pointer.update(true, false, at(100.0, 100.0), TICK),
            vec![Verb::Poke]
        );
        assert!(!pointer.grabbing());
    }

    /// The bug this whole shape exists to avoid: deciding on the press would
    /// snap the sprite to the cursor and drop it on every click.
    #[test]
    fn a_click_emits_no_grab_at_any_point() {
        let mut pointer = Pointer::default();
        let mut verbs = Vec::new();

        verbs.extend(pointer.update(true, true, at(100.0, 100.0), TICK));
        // A hand is not steady; a click drifts a pixel.
        verbs.extend(pointer.update(true, true, at(101.0, 100.0), TICK));
        verbs.extend(pointer.update(true, false, at(101.0, 100.0), TICK));

        assert_eq!(verbs, vec![Verb::Poke]);
    }

    #[test]
    fn dragging_past_the_threshold_grabs_the_sprite() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);

        assert_eq!(
            pointer.update(true, true, at(103.0, 100.0), TICK),
            vec![],
            "three points is still a click"
        );
        assert_eq!(
            pointer.update(true, true, at(110.0, 100.0), TICK),
            vec![Verb::Grab]
        );
        assert!(pointer.grabbing(), "so the hit-test is suspended");
    }

    /// Picking the sprite up without moving it yet.
    #[test]
    fn holding_still_long_enough_also_grabs() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);

        let held = (0..DRAG_DELAY_MS / TICK + 2)
            .map(|_| pointer.update(true, true, at(100.0, 100.0), TICK))
            .find(|verbs| !verbs.is_empty());

        assert_eq!(held, Some(vec![Verb::Grab]));
        assert!(pointer.grabbing());
    }

    /// Once held, the cursor is free to leave the art. That is the whole point:
    /// the sprite follows the hand rather than the hand being pinned to it.
    #[test]
    fn a_grab_survives_the_cursor_leaving_the_sprite() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);
        pointer.update(true, true, at(120.0, 100.0), TICK);
        assert!(pointer.grabbing());

        assert_eq!(
            pointer.update(false, true, at(400.0, 300.0), TICK),
            vec![Verb::Grab],
            "the drag outran the sprite and still holds it"
        );
        assert!(pointer.grabbing());
    }

    #[test]
    fn releasing_a_moving_grab_throws_the_sprite() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);
        pointer.update(true, true, at(120.0, 100.0), TICK);

        pointer.update(false, true, at(136.0, 100.0), TICK);
        pointer.update(false, true, at(152.0, 100.0), TICK);
        let thrown = pointer.update(false, false, at(152.0, 100.0), TICK);

        // The hand covered 52 points across five 16ms ticks, which is 650
        // points a second. The measurement is taken over the recent window
        // rather than the whole gesture, so it lands near that figure rather
        // than on it — but a throw that misses the speed of the hand by more
        // than a quarter is not the throw the user made.
        let hand = 52.0 / (5.0 * f64::from(TICK) / 1000.0);
        match thrown.as_slice() {
            [Verb::Throw { velocity }] => {
                assert!(
                    (velocity.x - hand).abs() < hand * 0.25,
                    "thrown at {:.0}, where the hand moved at {hand:.0} points a second",
                    velocity.x
                );
                assert_eq!(velocity.y, 0.0, "and not downward, which gravity does");
            }
            other => panic!("expected a Throw, got {other:?}"),
        }
        assert!(!pointer.grabbing(), "and the hit-test comes back");
    }

    /// Letting go of a sprite you were not moving puts it down rather than
    /// flinging it. The Engine drops any Grab that stops, so this emits nothing.
    #[test]
    fn releasing_a_still_grab_drops_the_sprite_rather_than_throwing_it() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);
        let grabbed = (0..DRAG_DELAY_MS / TICK + 2)
            .map(|_| pointer.update(true, true, at(100.0, 100.0), TICK))
            .any(|verbs| verbs.contains(&Verb::Grab));
        assert!(grabbed);

        // Long enough still that the velocity window holds only stillness.
        for _ in 0..8 {
            pointer.update(true, true, at(100.0, 100.0), TICK);
        }
        assert_eq!(pointer.update(true, false, at(100.0, 100.0), TICK), vec![]);
        assert!(!pointer.grabbing());
    }

    /// Nobody holds a mouse perfectly still. A release after a pause is putting
    /// the sprite down, and one twitch in the last moments must not turn that
    /// into a fling.
    #[test]
    fn a_twitch_at_the_moment_of_release_is_not_a_throw() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);
        pointer.update(true, true, at(120.0, 100.0), TICK);
        assert!(pointer.grabbing());

        // Settled, then one point of jitter on the last tick before letting go.
        for _ in 0..8 {
            pointer.update(true, true, at(300.0, 300.0), TICK);
        }
        pointer.update(true, true, at(301.0, 300.0), TICK);

        assert_eq!(
            pointer.update(true, false, at(301.0, 300.0), TICK),
            vec![],
            "put down, not thrown"
        );
    }

    /// A click that begins somewhere else belongs to whatever is under it. The
    /// overlay passes those through, and the sprite must not claim them.
    #[test]
    fn a_press_that_did_not_begin_on_the_sprite_is_ignored() {
        let mut pointer = Pointer::default();

        assert_eq!(pointer.update(false, true, at(500.0, 500.0), TICK), vec![]);
        assert_eq!(
            pointer.update(true, true, at(100.0, 100.0), TICK),
            vec![],
            "dragging a window under the sprite does not pick the sprite up"
        );
        assert_eq!(pointer.update(true, false, at(100.0, 100.0), TICK), vec![]);
        assert!(!pointer.grabbing());
    }

    /// A Grab is reported every tick it is held, not only on the tick it began:
    /// the Engine reads it as a state, and a missing one is a release.
    #[test]
    fn a_held_grab_is_reported_on_every_tick() {
        let mut pointer = Pointer::default();
        pointer.update(true, true, at(100.0, 100.0), TICK);
        pointer.update(true, true, at(120.0, 100.0), TICK);

        for step in 0..5 {
            let x = 130.0 + f64::from(step) * 10.0;
            assert_eq!(
                pointer.update(false, true, at(x, 100.0), TICK),
                vec![Verb::Grab]
            );
        }
    }

    /// Nothing under the cursor is nobody's press, which is what leaves a click
    /// on the desktop to the desktop.
    #[test]
    fn a_press_over_no_sprite_belongs_to_nobody() {
        assert_eq!(press_target(&[false, false], None), None);
        assert_eq!(press_target(&[], None), None);
    }

    #[test]
    fn a_press_over_one_sprite_belongs_to_it() {
        assert_eq!(press_target(&[false, true, false], None), Some(1));
    }

    /// Two sprites overlap and only the top one is the one the user can see
    /// under the cursor. Drawn in roster order, so the last is the top.
    #[test]
    fn a_press_where_two_sprites_overlap_belongs_to_the_top_one() {
        assert_eq!(press_target(&[true, true], None), Some(1));
        assert_eq!(press_target(&[true, true, true], None), Some(2));
    }

    /// The whole point of the holder: a drag that outruns the art leaves the
    /// cursor over no sprite at all, and the Instance being dragged must not
    /// change or be dropped because of it. #6 fixed this for one sprite; with
    /// several, it also must not pass to whichever sprite the cursor crossed.
    #[test]
    fn the_instance_being_held_keeps_the_press_wherever_the_cursor_goes() {
        assert_eq!(press_target(&[false, false], Some(0)), Some(0));
        assert_eq!(
            press_target(&[true, false], Some(1)),
            Some(1),
            "crossing another sprite does not hand it over"
        );
    }

    /// An Instance dismissed mid-drag leaves a holder that is no longer there.
    /// The press falls back to what is under the cursor rather than to an index
    /// that would panic or point at somebody else's Instance.
    #[test]
    fn a_holder_that_is_gone_gives_the_press_back_to_the_cursor() {
        assert_eq!(press_target(&[false, true], Some(5)), Some(1));
        assert_eq!(press_target(&[false, false], Some(5)), None);
    }
}
