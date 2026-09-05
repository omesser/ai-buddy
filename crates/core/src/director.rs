//! Director: propose the next Behavior.
//!
//! `StaticDirector` picks from the Character's weights. Use it when no
//! Harness is attached, the Director is off, or a session call fails
//! (DESIGN.md decision 5, ADR-0008). `ModelDirector` sends a Character
//! Prompt through a `Completer` and parses the reply. The Completer is the
//! attached Harness once #16 lands; until then it is an HTTP stand-in. This
//! crate does not do I/O.
//!
//! The Shell decides when to call either one. Do not wait on the model in
//! the frame loop. Apply a finished proposal on the next tick, or drop it.
//! Static may wake often (`due`). A session wake is `session_due`: reactive
//! or backed-off, never while the display is asleep.
//!
//! `StaticDirector` tests pass a fixed seed so the same inputs pick the same
//! Behaviors on every run.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::character::{Behavior, Trigger, DEFAULT_MODEL_BASE, DEFAULT_MODEL_POWER};
use crate::engine::{BehaviorProposal, State};
use crate::sensing::Activity;

mod prompt;
pub use prompt::{character_prompt, follow_up};

/// How long the Static Director goes unwoken when nothing notable happens.
///
/// A tuning knob. Long enough that the sprite is not constantly interrupting
/// itself, short enough that a glance at the desktop usually catches it doing
/// something. This path costs nothing to wake. A session wake is `Pace`, not
/// this number.
pub const WAKE_EVERY: Duration = Duration::from_secs(20);

/// Idle duration that counts as the user leaving.
/// Wake once when idle crosses this. Do not wake again while it stays over.
pub const IDLE_OVER: Duration = Duration::from_secs(5 * 60);

/// Wake if the sprite has been in the same State this long.
pub const STATE_BOUND: Duration = Duration::from_secs(90);

/// How many Behaviors back the Director is asked to remember.
///
/// A tuning knob, and the reason suppression has to be able to give way: a
/// Character declaring fewer Behaviors than this would otherwise run out of
/// things it is allowed to do.
pub const REMEMBERED: usize = 3;

/// What the user (or the clock) just did, in one word for the follow-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Happened {
    Poke,
    Throw,
    Summon,
    /// Grab started this tick. Grab itself repeats every held tick.
    Grab,
    /// The sprite just became Perched — placed on a window edge.
    Perch,
    Ambient,
}

/// What the Director is told about the world on one wake.
///
/// The Free tier and the recent past, which is the whole of v1's context per
/// `docs/SPEC.md`. Recent Behavior identifiers are handed back rather than
/// remembered here, so the Director stays a function of what it is given.
#[derive(Clone, Debug)]
pub struct Context {
    pub activity: Activity,
    /// Behavior identifiers played recently, most recent first.
    pub recent: Vec<String>,
    /// The active Character's Personality Prompt. Empty when the package
    /// shipped none.
    pub personality: String,
    pub state: State,
    pub happened: Happened,
    /// What the feet are on: a window (owner name), the floor above the
    /// Dock, or a screen edge. Not a title — that needs Screen Recording.
    pub standing: String,
}

/// Whatever decides what the buddy does next.
pub trait Director {
    /// A Behavior to play, or nothing when this moment suits none.
    fn propose(&mut self, context: &Context) -> Option<BehaviorProposal>;
}

/// Completes a Character Prompt.
///
/// The attached Harness, once #16 lands. Until then, an HTTP stand-in in the
/// shell. Tests put a double here.
pub trait Completer {
    fn complete(&self, prompt: &str) -> Result<String, String>;
}

/// Result of one model call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wake {
    Proposed(BehaviorProposal),
    /// Completer error, timeout, or unparsable reply. Use `StaticDirector`.
    Failed,
}

/// Sends a Character Prompt through a `Completer` and parses the reply.
pub struct ModelDirector<C> {
    completer: C,
    behaviors: Vec<String>,
    /// The Character Prompt is the opening turn only. After a successful
    /// Completer hop, later wakes send `follow_up`.
    opened: AtomicBool,
}

impl<C> ModelDirector<C> {
    pub fn new(completer: C, behaviors: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            completer,
            behaviors: behaviors.into_iter().map(Into::into).collect(),
            opened: AtomicBool::new(false),
        }
    }
}

impl<C: Completer> ModelDirector<C> {
    /// The user turn for this wake. Settings shows this string.
    pub fn prompt(&self, context: &Context) -> String {
        if self.opened.load(Ordering::SeqCst) {
            follow_up(context)
        } else {
            character_prompt(context, self.behaviors.iter())
        }
    }

    pub fn wake(&self, context: &Context) -> Wake {
        self.wake_and_near_miss(context).0
    }

    /// The wake, and the Behavior name the reply proposed that this Character
    /// declares none of.
    ///
    /// A near miss (`prowll`, `Inspectt`) is a contract miss, and it arrives
    /// as speech exactly like a model that chose to talk — so without this it
    /// is invisible, trace flag or not. Reported, never corrected; guessing
    /// a correction is what #231 ruled out (#243).
    pub fn wake_and_near_miss(&self, context: &Context) -> (Wake, Option<String>) {
        match self.completer.complete(&self.prompt(context)) {
            Ok(reply) => {
                // The Completer has the opening; later turns stay short
                // even if this reply failed to parse.
                self.opened.store(true, Ordering::SeqCst);
                match parse_proposal(&reply) {
                    // The declared spelling, not the model's: a name written
                    // at the start of a line comes back capitalised, and the
                    // Engine looks a Behavior up by the name its Character
                    // declared (#231).
                    Ok(proposal) => match self.declared(&proposal.behavior) {
                        Some(behavior) => (
                            Wake::Proposed(BehaviorProposal {
                                behavior,
                                dialogue: proposal.dialogue,
                            }),
                            None,
                        ),
                        None if proposal.behavior.eq_ignore_ascii_case("say") => {
                            match proposal.dialogue {
                                Some(line) => (
                                    Wake::Proposed(BehaviorProposal {
                                        behavior: String::new(),
                                        dialogue: Some(line),
                                    }),
                                    None,
                                ),
                                None => (Wake::Failed, None),
                            }
                        }
                        // `parse_proposal` has already ruled the name a single
                        // token, so this is the near miss and not prose.
                        None => (spoken_or_failed(&reply), Some(proposal.behavior)),
                    },
                    Err(_) => (spoken_or_failed(&reply), None),
                }
            }
            Err(_) => (Wake::Failed, None),
        }
    }

    /// What this Character declared, for a Shell reporting a near miss
    /// against it.
    pub fn behaviors(&self) -> &[String] {
        &self.behaviors
    }

    /// The Character's own spelling of `name`, when it declared one.
    ///
    /// Compared without case because that is the only way the two ever
    /// differ in practice, and because `say` beside it is already matched
    /// that way. A name nobody declared stays unknown, so loosening the
    /// comparison never invents a Behavior (#231).
    fn declared(&self, name: &str) -> Option<String> {
        self.behaviors
            .iter()
            .find(|declared| declared.eq_ignore_ascii_case(name))
            .cloned()
    }
}

/// A reply that named no Behavior: speech when there are words, else a
/// failed turn for `StaticDirector` to take.
fn spoken_or_failed(reply: &str) -> Wake {
    match as_speech(reply) {
        Some(said) => Wake::Proposed(said),
        None => Wake::Failed,
    }
}

/// A reply that is not a declared Behavior. Empty name: the Engine plays
/// `talk` and speaks. #119 shows this in a bubble.
fn as_speech(reply: &str) -> Option<BehaviorProposal> {
    let text = reply.trim();
    let text = text
        .strip_prefix("say:")
        .or_else(|| text.strip_prefix("Say:"))
        .map(str::trim)
        .unwrap_or(text);
    (!text.is_empty()).then(|| BehaviorProposal {
        behavior: String::new(),
        dialogue: Some(text.to_string()),
    })
}

/// Return the model proposal, or ask `StaticDirector` if the call failed.
pub fn fallback(
    wake: Wake,
    static_director: &mut StaticDirector,
    context: &Context,
) -> Option<BehaviorProposal> {
    match wake {
        Wake::Proposed(proposal) => Some(proposal),
        Wake::Failed => static_director.propose(context),
    }
}

/// Record a Behavior as just played, forgetting whatever fell off the end.
///
/// The caller keeps the list because the Director is a function of what it is
/// handed: what has been played is the Shell's to know, since the Shell is what
/// plays it.
pub fn remember(recent: &mut Vec<String>, behavior: String) {
    recent.insert(0, behavior);
    recent.truncate(REMEMBERED);
}

/// Wait between proactive model calls. Grows by `model_base.pow(model_power)`
/// after each proactive call, resets when the user addresses the buddy.
/// The Character Manifest names those two. ADR-0015.
#[derive(Clone, Debug)]
pub struct Pace {
    first: Duration,
    wait: Duration,
    base: u32,
    power: u32,
}

impl Pace {
    /// First proactive wait, and the value a reactive wake resets to.
    pub const FIRST: Duration = Duration::from_secs(2 * 60);
    /// Ceiling after repeated proactive wakes with no one addressing the buddy.
    pub const CAP: Duration = Duration::from_secs(2 * 60 * 60);

    pub fn new() -> Self {
        Self::with_first(Self::FIRST)
    }

    pub fn with_first(first: Duration) -> Self {
        Self::with_growth(first, DEFAULT_MODEL_BASE, DEFAULT_MODEL_POWER)
    }

    /// `first` is the opening wait. After each proactive model call,
    /// the wait becomes `wait * base.pow(power)`, capped at `CAP`.
    pub fn with_growth(first: Duration, base: u32, power: u32) -> Self {
        let first = first.clamp(Duration::from_secs(1), Self::CAP);
        Self {
            first,
            wait: first,
            base: base.max(1),
            power,
        }
    }

    pub fn wait(&self) -> Duration {
        self.wait
    }

    pub fn after_ambient(&mut self) {
        let factor = self.base.saturating_pow(self.power).max(1);
        self.wait = self.wait.saturating_mul(factor).min(Self::CAP);
    }

    pub fn after_reactive(&mut self) {
        self.wait = self.first;
    }
}

impl Default for Pace {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether to wake the Static Director.
///
/// True on frontmost-app change, idle crossing `IDLE_OVER`, time in one State
/// reaching `STATE_BOUND`, or `since_wake >= every`. Free, so it may be chatty.
/// Quiet under Do Not Disturb: no Director wakes cost less than refused proposals.
pub fn due(
    since_wake: Duration,
    every: Duration,
    activity: &Activity,
    previous_idle: Duration,
    since_state: Duration,
    do_not_disturb: bool,
) -> bool {
    if do_not_disturb {
        return false;
    }
    activity.switched
        || (previous_idle < IDLE_OVER && activity.idle >= IDLE_OVER)
        || since_state >= STATE_BOUND
        || since_wake >= every
}

/// Whether to wake the session Director (Harness, or the HTTP stand-in).
///
/// Reactive when the user addressed the buddy. Proactive when `since_ambient`
/// has reached the current `Pace` and ambient wakes are allowed. Never while
/// the display is asleep. Quiet under Do Not Disturb so the Character stays
/// visible and Poke still works; displays-asleep would drop Poke too.
///
/// Ambient is its own switch: off keeps Poke and Summon on the session path
/// and leaves Static weights to fill the idle life. #18.
pub fn session_due(
    addressed: bool,
    since_ambient: Duration,
    pace: &Pace,
    displays_asleep: bool,
    do_not_disturb: bool,
    ambient_allowed: bool,
) -> bool {
    if do_not_disturb {
        return false;
    }
    if displays_asleep {
        return false;
    }
    addressed || (ambient_allowed && since_ambient >= pace.wait())
}

/// The reply was not a Behavior name. Fall back instead of guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

/// Parse a reply as a Behavior name on the first line and optional dialogue
/// after. Anything else is `ParseError`.
pub fn parse_proposal(reply: &str) -> Result<BehaviorProposal, ParseError> {
    let mut lines = reply.lines().map(str::trim).filter(|line| !line.is_empty());
    let first = lines.next().ok_or(ParseError)?;
    let (name, inline) = match first.split_once('|') {
        Some((name, line)) => (name.trim(), Some(line.trim())),
        None => (first, None),
    };
    let name = name.trim_end_matches(['.', ':']);
    if name.is_empty() || !identifier(name) {
        return Err(ParseError);
    }

    let dialogue = match inline {
        Some(line) if !line.is_empty() => Some(line.to_string()),
        _ => {
            let rest: Vec<&str> = lines.collect();
            (!rest.is_empty()).then(|| rest.join(" "))
        }
    };

    Ok(BehaviorProposal {
        behavior: name.to_string(),
        dialogue,
    })
}

/// True if `name` is a single token. The Engine still rejects unknown names.
fn identifier(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Weighted selection over a Character's declared Behaviors. No model, no
/// network, no clock.
pub struct StaticDirector {
    behaviors: BTreeMap<String, Behavior>,
    seeded: Seeded,
}

impl StaticDirector {
    /// The Behaviors to choose among, and the seed that decides which.
    pub fn new(behaviors: BTreeMap<String, Behavior>, seed: u64) -> Self {
        Self {
            behaviors,
            seeded: Seeded(seed),
        }
    }
}

impl Director for StaticDirector {
    /// Pick among the Behaviors this moment permits, by weight.
    ///
    /// Three filters and a draw. A Behavior of no weight is one the author took
    /// out of the running; a trigger that does not match is a Behavior this
    /// moment is not for; and a Behavior played recently is one the user has
    /// just seen.
    ///
    /// Suppression gives way when it would leave nothing, because a Character
    /// with two Behaviors and three of them remembered would otherwise go still
    /// for ever. Repeating is worse than pausing only while there is something
    /// else to do.
    fn propose(&mut self, context: &Context) -> Option<BehaviorProposal> {
        let suits = |(name, behavior): (&String, &Behavior)| {
            let triggered = match &behavior.trigger {
                None => true,
                Some(trigger) => triggered(trigger, &context.activity),
            };
            (behavior.weight > 0 && triggered).then(|| (name.clone(), behavior.weight))
        };

        let eligible: Vec<(String, u32)> = self.behaviors.iter().filter_map(suits).collect();
        let unseen: Vec<(String, u32)> = eligible
            .iter()
            .filter(|(name, _)| !context.recent.contains(name))
            .cloned()
            .collect();

        let choices = if unseen.is_empty() {
            &eligible
        } else {
            &unseen
        };
        let behavior = self.seeded.pick(choices)?;

        // A Static Director has nothing to say. Dialogue is the model-backed
        // Director's, and a canned line would be worse than silence.
        Some(BehaviorProposal {
            behavior,
            dialogue: None,
        })
    }
}

/// Whether the moment matches what a Behavior asked for.
fn triggered(trigger: &Trigger, activity: &Activity) -> bool {
    match trigger {
        Trigger::IdleOver(span) => activity.idle > *span,
        Trigger::IdleUnder(span) => activity.idle < *span,
        Trigger::Frontmost(application) => {
            activity.frontmost_application.as_deref() == Some(application.as_str())
        }
    }
}

/// A seeded source of randomness, so that "unpredictable to the user" and
/// "unpredictable to a test" are different things.
///
/// splitmix64: five lines, no dependency, no seed it degenerates on — which a
/// bare xorshift has at zero. `rand` would be a dependency and a trait object
/// for a coin toss the Engine performs once every twenty seconds.
///
/// Public because the Shell needs draws of its own — where each Instance's wake
/// clock starts — and a second generator there would be a second thing to
/// reason about at a second quality. One mixer, one set of properties.
pub struct Seeded(u64);

impl Seeded {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next draw. Well mixed even from adjacent seeds, which is what lets
    /// one launch seed a buddy apiece.
    pub fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// One name out of `choices`, each as likely as its weight says.
    ///
    /// The draw is taken over the running total, which is why the order matters
    /// and why a `BTreeMap` feeds it: the same seed and the same Character must
    /// pick the same Behavior on every machine and every run.
    ///
    /// Modulo bias is real and irrelevant here — the weights of one Character
    /// sum to something astronomically smaller than 2^64, so the bias is a part
    /// in billions of billions.
    fn pick(&mut self, choices: &[(String, u32)]) -> Option<String> {
        let total: u64 = choices.iter().map(|(_, weight)| u64::from(*weight)).sum();
        if total == 0 {
            return None;
        }

        let mut drawn = self.draw() % total;
        for (name, weight) in choices {
            match drawn.checked_sub(u64::from(*weight)) {
                Some(left) => drawn = left,
                None => return Some(name.clone()),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Primitive;
    use std::time::UNIX_EPOCH;

    /// A Behavior of one Primitive, which is all selection cares about.
    fn behavior(weight: u32, trigger: Option<Trigger>) -> Behavior {
        Behavior {
            primitives: vec![Primitive::Idle],
            then: None,
            weight,
            trigger,
        }
    }

    /// The Behaviors a Character declares, as `(name, weight, trigger)`.
    fn declaring(behaviors: &[(&str, u32, Option<Trigger>)]) -> BTreeMap<String, Behavior> {
        behaviors
            .iter()
            .map(|(name, weight, trigger)| (name.to_string(), behavior(*weight, trigger.clone())))
            .collect()
    }

    /// Someone at their machine, in Terminal, having just typed.
    fn working() -> Activity {
        Activity {
            frontmost_application: Some("Terminal".to_string()),
            switched: false,
            idle: Duration::ZERO,
            at: UNIX_EPOCH,
            hour: 0,
            minute: 0,
            displays_asleep: false,
        }
    }

    fn context(activity: Activity, recent: &[&str]) -> Context {
        Context {
            activity,
            recent: recent.iter().map(|name| name.to_string()).collect(),
            personality: "a shy robot.".to_string(),
            state: State::Grounded,
            happened: Happened::Poke,
            standing: String::new(),
        }
    }

    /// What the Director proposes over `wakes` wakes into the same moment.
    fn proposed(director: &mut StaticDirector, context: &Context, wakes: usize) -> Vec<String> {
        (0..wakes)
            .filter_map(|_| director.propose(context).map(|proposal| proposal.behavior))
            .collect()
    }

    #[test]
    fn the_same_seed_picks_the_same_behaviors_in_the_same_order() {
        let behaviors = declaring(&[("nap", 1, None), ("pace", 1, None), ("wave", 1, None)]);
        let moment = context(working(), &[]);

        let first = proposed(&mut StaticDirector::new(behaviors.clone(), 7), &moment, 20);
        let again = proposed(&mut StaticDirector::new(behaviors.clone(), 7), &moment, 20);
        let other = proposed(&mut StaticDirector::new(behaviors, 8), &moment, 20);

        assert_eq!(first.len(), 20, "every wake proposed something");
        assert_eq!(
            first, again,
            "the seed is the whole of the unpredictability"
        );
        assert_ne!(first, other, "and a different seed is a different life");
    }

    /// Two Instances of one Character, seeded a bit apart the way the Shell
    /// seeds them, and each keeping its own record of what it has played.
    ///
    /// #13 asks for Instances that play Behaviors independently, and a
    /// difference in seed alone does not buy it: suppression walks each
    /// Director through what it has not lately done, so with a Character
    /// declaring few enough Behaviors both are steered onto the same one and
    /// stay in step. This pins where the line actually falls.
    #[test]
    fn two_instances_of_one_character_do_not_pick_in_lockstep() {
        // BMO's own set, which is what the lockstep was first seen with.
        let behaviors = declaring(&[
            ("walk", 1, None),
            ("patrol", 3, None),
            ("fidget", 2, None),
            ("report", 3, None),
            ("greet", 4, None),
        ]);

        // Each Instance remembers only its own Behaviors, exactly as the frame
        // loop does with one `recent` per Instance.
        let played = |seed: u64| -> Vec<String> {
            let mut director = StaticDirector::new(behaviors.clone(), seed);
            let mut recent: Vec<String> = Vec::new();
            (0..8)
                .filter_map(|_| {
                    let moment = context(
                        working(),
                        &recent.iter().map(String::as_str).collect::<Vec<_>>(),
                    );
                    director.propose(&moment).map(|proposal| {
                        remember(&mut recent, proposal.behavior.clone());
                        proposal.behavior
                    })
                })
                .collect()
        };

        let one = played(0x5EED);
        let two = played(0x5EED ^ 1);

        assert_eq!(one.len(), 8, "both wake the same number of times");
        assert_ne!(
            one, two,
            "a Character with five Behaviors leaves suppression room to differ"
        );
    }

    #[test]
    fn a_static_director_proposes_no_dialogue_of_its_own() {
        let mut director = StaticDirector::new(declaring(&[("nap", 1, None)]), 1);

        let proposal = director
            .propose(&context(working(), &[]))
            .expect("a Behavior suits this moment");

        assert_eq!(proposal.behavior, "nap");
        assert_eq!(proposal.dialogue, None, "speaking is the model's");
    }

    /// The distribution, over a fixed seed rather than over a real source: the
    /// counts below are the same on every run and every machine.
    #[test]
    fn weight_decides_how_often_a_behavior_is_picked() {
        let mut director =
            StaticDirector::new(declaring(&[("often", 3, None), ("seldom", 1, None)]), 42);

        let picked = proposed(&mut director, &context(working(), &[]), 4000);
        let often = picked.iter().filter(|name| *name == "often").count();
        let seldom = picked.len() - often;

        assert!(
            (2.7..3.3).contains(&(often as f64 / seldom as f64)),
            "three to one, near enough: {often} against {seldom}"
        );
    }

    #[test]
    fn a_behavior_of_no_weight_is_out_of_the_running_entirely() {
        let mut director =
            StaticDirector::new(declaring(&[("never", 0, None), ("always", 1, None)]), 3);

        let picked = proposed(&mut director, &context(working(), &[]), 200);
        assert!(
            picked.iter().all(|name| name == "always"),
            "weight zero takes a Behavior out of the running: {picked:?}"
        );

        // Nor does it count as something else to do. A Behavior the author took
        // out of the running is no reason to stand still while one that is
        // merely fresh out could be repeated.
        let after = proposed(&mut director, &context(working(), &["always"]), 20);
        assert_eq!(after.len(), 20, "{after:?}");
    }

    #[test]
    fn nothing_is_proposed_when_the_character_declares_nothing_to_do() {
        let mut director = StaticDirector::new(BTreeMap::new(), 1);

        assert_eq!(director.propose(&context(working(), &[])), None);
    }

    #[test]
    fn an_idle_trigger_gates_on_how_long_the_user_has_been_away() {
        let nap = declaring(&[("nap", 1, Some(Trigger::IdleOver(Duration::from_secs(120))))]);
        let away = |idle| Activity { idle, ..working() };

        let mut director = StaticDirector::new(nap.clone(), 5);
        assert_eq!(
            director.propose(&context(away(Duration::from_secs(120)), &[])),
            None,
            "two minutes is not over two minutes"
        );

        let mut director = StaticDirector::new(nap, 5);
        assert_eq!(
            director
                .propose(&context(away(Duration::from_secs(121)), &[]))
                .map(|proposal| proposal.behavior),
            Some("nap".to_string())
        );
    }

    #[test]
    fn a_freshly_returned_user_is_a_different_moment_from_a_long_gone_one() {
        let behaviors = declaring(&[
            ("greet", 1, Some(Trigger::IdleUnder(Duration::from_secs(5)))),
            ("nap", 1, Some(Trigger::IdleOver(Duration::from_secs(120)))),
        ]);
        let at = |idle| {
            proposed(
                &mut StaticDirector::new(behaviors.clone(), 11),
                &context(Activity { idle, ..working() }, &[]),
                50,
            )
        };

        assert!(at(Duration::from_secs(1)).iter().all(|n| n == "greet"));
        assert!(at(Duration::from_secs(300)).iter().all(|n| n == "nap"));
        assert!(
            at(Duration::from_secs(60)).is_empty(),
            "a minute away suits neither, and the buddy simply carries on"
        );
    }

    #[test]
    fn an_application_trigger_gates_on_what_is_frontmost() {
        let behaviors = declaring(&[(
            "browse",
            1,
            Some(Trigger::Frontmost("Google Chrome".to_string())),
        )]);
        let in_application = |name: Option<&str>| {
            proposed(
                &mut StaticDirector::new(behaviors.clone(), 2),
                &context(
                    Activity {
                        frontmost_application: name.map(String::from),
                        ..working()
                    },
                    &[],
                ),
                20,
            )
        };

        assert_eq!(in_application(Some("Google Chrome")).len(), 20);
        assert!(in_application(Some("Chrome")).is_empty(), "not a prefix");
        assert!(in_application(None).is_empty(), "nor an empty desktop");
    }

    #[test]
    fn a_recently_played_behavior_is_not_proposed_again() {
        let mut director = StaticDirector::new(
            declaring(&[("nap", 8, None), ("pace", 1, None), ("wave", 1, None)]),
            13,
        );

        let picked = proposed(&mut director, &context(working(), &["nap"]), 200);

        assert!(
            !picked.iter().any(|name| name == "nap"),
            "the heaviest Behavior is still refused while it is fresh: {picked:?}"
        );
        assert!(picked.iter().any(|name| name == "pace"));
        assert!(picked.iter().any(|name| name == "wave"));
    }

    /// A Character with little to do would otherwise be silenced by its own
    /// history: two Behaviors, both remembered, nothing left to pick.
    #[test]
    fn suppression_gives_way_rather_than_leaving_the_buddy_still() {
        let mut director =
            StaticDirector::new(declaring(&[("nap", 1, None), ("pace", 1, None)]), 4);

        let picked = proposed(&mut director, &context(working(), &["nap", "pace"]), 20);

        assert_eq!(picked.len(), 20, "repeating beats standing there");
    }

    #[test]
    fn what_is_remembered_is_the_last_few_behaviors_newest_first() {
        let mut recent = Vec::new();
        for behavior in ["nap", "pace", "wave", "greet"] {
            remember(&mut recent, behavior.to_string());
        }

        assert_eq!(recent, ["greet", "wave", "pace"], "\"nap\" is forgotten");
        assert_eq!(recent.len(), REMEMBERED);
    }

    /// Timer and switch only: idle has not crossed and the State is fresh.
    fn on_timer(since_wake: Duration, activity: &Activity) -> bool {
        due(
            since_wake,
            WAKE_EVERY,
            activity,
            Duration::MAX,
            Duration::ZERO,
            false,
        )
    }

    #[test]
    fn the_director_wakes_on_a_switch_and_otherwise_on_the_timer() {
        let switched = Activity {
            switched: true,
            ..working()
        };

        assert!(
            on_timer(Duration::ZERO, &switched),
            "frontmost application changed"
        );
        assert!(
            !on_timer(Duration::ZERO, &working()),
            "nothing has happened"
        );
        assert!(!on_timer(WAKE_EVERY - Duration::from_millis(1), &working()));
        assert!(on_timer(WAKE_EVERY, &working()), "the timer comes due");
    }

    #[test]
    fn the_director_wakes_when_idle_crosses_the_threshold_and_not_while_past_it() {
        let away = Activity {
            idle: IDLE_OVER,
            ..working()
        };
        let still = Activity {
            idle: IDLE_OVER + Duration::from_secs(30),
            ..working()
        };

        assert!(
            due(
                Duration::ZERO,
                WAKE_EVERY,
                &away,
                Duration::ZERO,
                Duration::ZERO,
                false
            ),
            "idle crossed IDLE_OVER"
        );
        assert!(
            !due(
                Duration::ZERO,
                WAKE_EVERY,
                &still,
                IDLE_OVER,
                Duration::ZERO,
                false
            ),
            "staying away is not another event"
        );
        assert!(
            !due(
                Duration::ZERO,
                WAKE_EVERY,
                &working(),
                Duration::ZERO,
                Duration::ZERO,
                false
            ),
            "still at the machine is not a crossing"
        );
    }

    #[test]
    fn the_director_wakes_when_one_state_outlasts_its_bound() {
        assert!(
            due(
                Duration::ZERO,
                WAKE_EVERY,
                &working(),
                Duration::MAX,
                STATE_BOUND,
                false
            ),
            "since_state reached STATE_BOUND"
        );
        assert!(!due(
            Duration::ZERO,
            WAKE_EVERY,
            &working(),
            Duration::MAX,
            STATE_BOUND - Duration::from_millis(1),
            false
        ));
    }

    #[test]
    fn wake_frequency_is_the_interval_the_caller_hands_in() {
        let longer = Duration::from_secs(180);
        assert!(!due(
            WAKE_EVERY,
            longer,
            &working(),
            Duration::MAX,
            Duration::ZERO,
            false
        ));
        assert!(due(
            longer,
            longer,
            &working(),
            Duration::MAX,
            Duration::ZERO,
            false
        ));
    }

    #[test]
    fn a_session_wake_is_reactive_or_backed_off_and_silent_while_asleep() {
        let pace = Pace::new();

        assert!(
            session_due(true, Duration::ZERO, &pace, false, false, true),
            "the user addressed the buddy"
        );
        assert!(
            !session_due(false, Duration::ZERO, &pace, false, false, true),
            "nothing happened and the wait has not elapsed"
        );
        assert!(
            session_due(false, Pace::FIRST, &pace, false, false, true),
            "the first ambient wait has elapsed"
        );
        assert!(
            !session_due(true, Duration::ZERO, &pace, true, false, true),
            "asleep: not even a Poke spends tokens"
        );
        assert!(
            !session_due(false, Pace::FIRST, &pace, true, false, true),
            "asleep: ambient stays quiet"
        );
    }

    #[test]
    fn ambient_session_waits_double_and_a_reactive_wake_resets_them() {
        let mut pace = Pace::new();
        assert_eq!(pace.wait(), Pace::FIRST);

        pace.after_ambient();
        assert_eq!(pace.wait(), Pace::FIRST * 2);
        pace.after_ambient();
        assert_eq!(pace.wait(), Pace::FIRST * 4);

        pace.after_reactive();
        assert_eq!(pace.wait(), Pace::FIRST, "addressed: start the wait over");
    }

    #[test]
    fn ambient_session_waits_do_not_grow_past_the_cap() {
        let mut pace = Pace::with_first(Pace::CAP);
        pace.after_ambient();
        assert_eq!(pace.wait(), Pace::CAP);
    }

    #[test]
    fn the_first_ambient_wait_is_two_minutes() {
        assert_eq!(Pace::FIRST, Duration::from_secs(2 * 60));
    }

    #[test]
    fn a_character_sets_how_ambient_session_waits_grow() {
        let mut pace = Pace::with_growth(Duration::from_secs(60), 3, 1);
        assert_eq!(pace.wait(), Duration::from_secs(60));
        pace.after_ambient();
        assert_eq!(pace.wait(), Duration::from_secs(180), "60 * 3^1");
        pace.after_ambient();
        assert_eq!(pace.wait(), Duration::from_secs(540), "180 * 3^1");

        let mut steep = Pace::with_growth(Duration::from_secs(60), 2, 2);
        steep.after_ambient();
        assert_eq!(steep.wait(), Duration::from_secs(240), "60 * 2^2");
    }

    #[test]
    fn a_clean_reply_is_a_behavior_and_optional_dialogue() {
        let spoken = parse_proposal("stroll\nhey there").expect("a named Behavior");
        assert_eq!(spoken.behavior, "stroll");
        assert_eq!(spoken.dialogue.as_deref(), Some("hey there"));

        let quiet = parse_proposal("wave").expect("dialogue is optional");
        assert_eq!(quiet.behavior, "wave");
        assert_eq!(quiet.dialogue, None);

        let inline = parse_proposal("nap | sleepy").expect("one-line form");
        assert_eq!(inline.behavior, "nap");
        assert_eq!(inline.dialogue.as_deref(), Some("sleepy"));
    }

    /// Completer that returns a fixed reply and records the prompt it received.
    struct Scripted {
        reply: Result<String, String>,
        seen: std::sync::Mutex<Option<String>>,
    }

    impl Scripted {
        fn says(reply: &str) -> Self {
            Self {
                reply: Ok(reply.to_string()),
                seen: std::sync::Mutex::new(None),
            }
        }

        fn fails() -> Self {
            Self {
                reply: Err("timeout".to_string()),
                seen: std::sync::Mutex::new(None),
            }
        }
    }

    impl Completer for Scripted {
        fn complete(&self, prompt: &str) -> Result<String, String> {
            *self.seen.lock().expect("the lock is not poisoned") = Some(prompt.to_string());
            self.reply.clone()
        }
    }

    #[test]
    fn a_model_director_proposes_what_the_completer_replies() {
        let director = ModelDirector::new(Scripted::says("stroll\nhey"), ["stroll", "wave"]);
        let moment = context(working(), &[]);

        match director.wake(&moment) {
            Wake::Proposed(proposal) => {
                assert_eq!(proposal.behavior, "stroll");
                assert_eq!(proposal.dialogue.as_deref(), Some("hey"));
            }
            other => panic!("the reply was a proposal, not {other:?}"),
        }
    }

    /// #231: a model writes the Behavior name at the start of a line, so it
    /// capitalises it — every local model measured in #175 answered `Prowl`
    /// where the manifest declares `prowl`. Matching exactly threw those
    /// replies away and spoke them instead, so the buddy talked and never
    /// acted. `say` two arms below was already compared case-insensitively;
    /// this is the same rule for the name beside it.
    #[test]
    fn a_declared_behavior_is_known_however_the_model_capitalises_it() {
        let director = ModelDirector::new(Scripted::says("Prowl\nMine now."), ["prowl"]);
        let moment = context(working(), &[]);

        match director.wake(&moment) {
            Wake::Proposed(proposal) => {
                assert_eq!(
                    proposal.behavior, "prowl",
                    "the declared spelling is what the Engine plays"
                );
                assert_eq!(proposal.dialogue.as_deref(), Some("Mine now."));
            }
            other => panic!("a capitalised name is still the Behavior, not {other:?}"),
        }
    }

    /// The other half: a name nobody declared is still prose, not a Behavior
    /// invented by loosening the comparison.
    #[test]
    fn a_name_no_character_declares_is_still_speech() {
        let director = ModelDirector::new(Scripted::says("Check\nSomething moved."), ["prowl"]);
        let moment = context(working(), &[]);

        match director.wake(&moment) {
            Wake::Proposed(proposal) => {
                assert!(
                    proposal.behavior.is_empty(),
                    "no Behavior was played: {proposal:?}"
                );
            }
            other => panic!("unknown names become speech, not {other:?}"),
        }
    }

    /// #243: `prowll` and a model that simply chose to talk both arrive as
    /// speech, so a contract miss is invisible in a trace. The name is handed
    /// back rather than corrected — the Shell is what prints it.
    #[test]
    fn an_undeclared_name_is_handed_back_as_a_near_miss() {
        let director = ModelDirector::new(Scripted::says("prowll\nMine now."), ["prowl", "wave"]);

        let (wake, near_miss) = director.wake_and_near_miss(&context(working(), &[]));

        assert_eq!(near_miss.as_deref(), Some("prowll"));
        match wake {
            Wake::Proposed(proposal) => assert!(
                proposal.behavior.is_empty(),
                "a near miss still becomes speech: {proposal:?}"
            ),
            other => panic!("a near miss still becomes speech, not {other:?}"),
        }
    }

    #[test]
    fn a_declared_name_is_no_near_miss() {
        for reply in ["prowl", "Prowl.", "PROWL:", "Prowl | hunting"] {
            let director = ModelDirector::new(Scripted::says(reply), ["prowl", "wave"]);

            let (_, near_miss) = director.wake_and_near_miss(&context(working(), &[]));

            assert_eq!(near_miss, None, "{reply:?} names something declared");
        }
    }

    /// `say` is the keyword every Character gets, not one it declares, so it
    /// takes its own arm above and must not be reported as a miss.
    #[test]
    fn the_say_keyword_is_no_near_miss() {
        let director = ModelDirector::new(Scripted::says("say | hello"), ["prowl", "wave"]);

        let (_, near_miss) = director.wake_and_near_miss(&context(working(), &[]));

        assert_eq!(near_miss, None);
    }

    #[test]
    fn the_completer_is_sent_the_character_prompt() {
        let director = ModelDirector::new(Scripted::says("wave"), ["wave"]);
        let moment = context(working(), &["nap"]);
        let expected = character_prompt(&moment, ["wave"]);

        director.wake(&moment);

        assert_eq!(
            director
                .completer
                .seen
                .lock()
                .expect("the lock is not poisoned")
                .as_deref(),
            Some(expected.as_str()),
            "Completer must receive character_prompt's output"
        );
    }

    #[test]
    fn a_director_error_falls_back_to_the_static_director() {
        let model = ModelDirector::new(Scripted::fails(), ["nap"]);
        let mut static_director = StaticDirector::new(declaring(&[("nap", 1, None)]), 1);
        let moment = context(working(), &[]);

        let proposal = fallback(model.wake(&moment), &mut static_director, &moment)
            .expect("StaticDirector proposed");

        assert_eq!(proposal.behavior, "nap");
        assert_eq!(proposal.dialogue, None, "the fallback does not speak");
    }

    #[test]
    fn a_valid_model_proposal_is_kept_and_the_static_director_is_not_asked() {
        let model = ModelDirector::new(Scripted::says("wave"), ["wave", "nap"]);
        let mut static_director = StaticDirector::new(declaring(&[("nap", 1, None)]), 1);
        let moment = context(working(), &[]);

        let proposal =
            fallback(model.wake(&moment), &mut static_director, &moment).expect("model proposed");

        assert_eq!(
            proposal.behavior, "wave",
            "a Behavior the Static Director does not even declare"
        );
    }

    #[test]
    fn a_garbled_reply_is_an_error_not_a_guess() {
        assert!(parse_proposal("").is_err(), "empty reply");
        assert!(
            parse_proposal("Sure, a stroll would be nice!").is_err(),
            "prose is not an identifier"
        );
        assert!(
            parse_proposal("***").is_err(),
            "punctuation is not an identifier"
        );
    }

    #[test]
    fn a_reply_that_is_not_a_behavior_is_said() {
        let director = ModelDirector::new(
            Scripted::says("It's 23:59! Almost a brand new day!"),
            ["wave", "report"],
        );
        match director.wake(&context(working(), &[])) {
            Wake::Proposed(said) => {
                assert!(said.behavior.is_empty(), "speaking is not a Behavior");
                assert_eq!(
                    said.dialogue.as_deref(),
                    Some("It's 23:59! Almost a brand new day!")
                );
            }
            other => panic!("prose should be said, not {other:?}"),
        }
    }

    #[test]
    fn a_say_prefix_is_stripped_and_the_rest_is_spoken() {
        let director = ModelDirector::new(Scripted::says("say: hey"), ["wave"]);
        match director.wake(&context(working(), &[])) {
            Wake::Proposed(said) => {
                assert!(said.behavior.is_empty());
                assert_eq!(said.dialogue.as_deref(), Some("hey"));
            }
            other => panic!("expected speech, got {other:?}"),
        }

        let piped = ModelDirector::new(Scripted::says("say | hey"), ["wave"]);
        match piped.wake(&context(working(), &[])) {
            Wake::Proposed(said) => {
                assert!(said.behavior.is_empty());
                assert_eq!(said.dialogue.as_deref(), Some("hey"));
            }
            other => panic!("expected speech, got {other:?}"),
        }
    }

    #[test]
    fn an_undeclared_identifier_is_said_not_played() {
        let director = ModelDirector::new(Scripted::says("cartwheel"), ["wave"]);
        match director.wake(&context(working(), &[])) {
            Wake::Proposed(said) => {
                assert!(said.behavior.is_empty());
                assert_eq!(said.dialogue.as_deref(), Some("cartwheel"));
            }
            other => panic!("expected speech, got {other:?}"),
        }
    }

    /// The prompt must include every Free-tier field. Settings shows this string.
    #[test]
    fn the_character_prompt_is_the_payload_the_model_is_sent() {
        let moment = Context {
            activity: Activity {
                frontmost_application: Some("Terminal".to_string()),
                switched: false,
                idle: Duration::from_secs(12),
                at: UNIX_EPOCH,
                hour: 22,
                minute: 15,
                displays_asleep: false,
            },
            recent: vec!["stroll".to_string(), "nap".to_string()],
            personality: "Blip is cheerful.".to_string(),
            state: State::Grounded,
            happened: Happened::Poke,
            standing: "the display floor, above the Dock".to_string(),
        };

        let payload = character_prompt(&moment, ["greet", "stroll", "wave"]);

        assert!(
            payload.contains("Blip is cheerful."),
            "personality: {payload}"
        );
        assert!(
            payload.contains("Terminal is the frontmost window"),
            "frontmost: {payload}"
        );
        assert!(
            payload.contains("22:15"),
            "local time of day, not UTC from `at` (00:00): {payload}"
        );
        assert!(
            !payload.contains("00:00"),
            "UNIX_EPOCH as UTC must not appear: {payload}"
        );
        assert!(
            payload.contains("stroll") && payload.contains("nap"),
            "recent Behavior identifiers: {payload}"
        );
        assert!(
            payload.contains("greet") && payload.contains("wave"),
            "declared Behaviors: {payload}"
        );
        assert!(
            payload.contains("what just happened: poked") && payload.contains("state: idle"),
            "this moment: {payload}"
        );
        assert!(
            payload.contains("standing on: the display floor, above the Dock"),
            "standing: {payload}"
        );
    }

    /// Each rule the opening turn must carry, and that later wakes do not
    /// repeat them.
    #[test]
    fn the_character_prompt_carries_the_voice_rules_once() {
        let moment = context(working(), &["nap"]);
        let payload = character_prompt(&moment, ["wave"]);

        assert!(
            payload.contains("always in character"),
            "no character breaks: {payload}"
        );
        assert!(
            payload.contains("model or an assistant"),
            "no model mentions: {payload}"
        );
        assert!(
            payload.contains("five short sentences"),
            "a line fits the bubble: {payload}"
        );
        assert!(payload.contains("Vary"), "no repeated lines: {payload}");
        assert!(
            payload.contains("never promise"),
            "demeanour, not capability: {payload}"
        );
        assert!(
            payload.contains("React to this moment when there is something worth remarking on"),
            "the wake facts are material to play off, not background: {payload}"
        );
        assert!(
            !follow_up(&moment).contains("always in character"),
            "the rules ride the opening only; later wakes stay cheap"
        );
        assert!(
            !follow_up(&moment).contains("React to this moment"),
            "the nudge is a rule too, and rides the opening with the rest"
        );
    }

    #[test]
    fn a_later_wake_sends_only_the_follow_up() {
        let director = ModelDirector::new(Scripted::says("wave"), ["wave", "greet"]);
        let first = context(working(), &["nap"]);
        director.wake(&first);

        let later = Context {
            happened: Happened::Throw,
            state: State::Falling,
            ..first
        };
        director.wake(&later);

        let sent = director
            .completer
            .seen
            .lock()
            .expect("the lock is not poisoned")
            .clone()
            .expect("a follow-up was sent");
        assert_eq!(sent, follow_up(&later));
        assert!(
            !sent.contains("a shy robot."),
            "personality is the opening only: {sent}"
        );
        assert!(
            !sent.contains("You may propose"),
            "the roster is the opening only: {sent}"
        );
        assert!(
            sent.contains("what just happened: thrown") && sent.contains("state: falling"),
            "{sent}"
        );
    }

    /// A switch is a new ModelDirector. The next wake has to be this
    /// Character's opening, not a follow-up in the previous conversation.
    #[test]
    fn a_new_director_opens_again() {
        let first = ModelDirector::new(Scripted::says("wave"), ["wave"]);
        let moment = context(working(), &["nap"]);
        first.wake(&moment);

        let next = ModelDirector::new(Scripted::says("wave"), ["stroll"]);
        let payload = next.prompt(&moment);
        assert!(
            payload.contains("You may propose"),
            "switch is a new opening: {payload}"
        );
        assert!(
            payload.contains("stroll"),
            "the new roster, not the old: {payload}"
        );
    }

    #[test]
    fn pick_up_and_perch_are_named_in_the_follow_up() {
        let picked = context(working(), &[]);
        let picked = Context {
            happened: Happened::Grab,
            state: State::Dragged,
            ..picked
        };
        assert!(follow_up(&picked).contains("what just happened: picked up"));

        let placed = Context {
            happened: Happened::Perch,
            state: State::Perched,
            standing: "a Cursor window".to_string(),
            ..picked
        };
        let sent = follow_up(&placed);
        assert!(sent.contains("what just happened: placed on a perch"));
        assert!(sent.contains("standing on: a Cursor window"), "{sent}");
    }

    #[test]
    fn session_due_is_false_under_do_not_disturb_even_when_addressed() {
        let pace = Pace::new();

        assert!(
            !session_due(true, Duration::ZERO, &pace, false, true, true),
            "addressed but Do Not Disturb is on"
        );
        assert!(
            !session_due(false, Pace::FIRST, &pace, false, true, true),
            "ambient wait elapsed but Do Not Disturb is on"
        );
    }

    #[test]
    fn session_due_unchanged_when_do_not_disturb_is_off() {
        let pace = Pace::new();

        assert!(
            session_due(true, Duration::ZERO, &pace, false, false, true),
            "addressed and Do Not Disturb is off"
        );
        assert!(
            session_due(false, Pace::FIRST, &pace, false, false, true),
            "ambient wait elapsed and Do Not Disturb is off"
        );
        assert!(
            !session_due(false, Duration::ZERO, &pace, false, false, true),
            "nothing happened and wait not elapsed"
        );
        assert!(
            !session_due(true, Duration::ZERO, &pace, true, false, true),
            "asleep silences even when Do Not Disturb is off"
        );

        // Ambient off is not Director off: a Poke still spends a session turn,
        // and an elapsed idle wait does not. Static weights keep the life.
        assert!(
            session_due(true, Duration::ZERO, &pace, false, false, false),
            "a Poke still wakes the Director when ambient is off"
        );
        assert!(
            !session_due(false, Pace::FIRST, &pace, false, false, false),
            "an elapsed ambient wait does not wake when ambient is off"
        );
    }

    #[test]
    fn due_is_false_under_do_not_disturb_even_when_timer_fires() {
        assert!(
            !due(
                WAKE_EVERY,
                WAKE_EVERY,
                &working(),
                Duration::MAX,
                Duration::ZERO,
                true
            ),
            "timer elapsed but Do Not Disturb is on"
        );

        let switched = Activity {
            switched: true,
            ..working()
        };
        assert!(
            !due(
                Duration::ZERO,
                WAKE_EVERY,
                &switched,
                Duration::MAX,
                Duration::ZERO,
                true
            ),
            "frontmost switched but Do Not Disturb is on"
        );
    }

    #[test]
    fn due_unchanged_when_do_not_disturb_is_off() {
        assert!(
            due(
                WAKE_EVERY,
                WAKE_EVERY,
                &working(),
                Duration::MAX,
                Duration::ZERO,
                false
            ),
            "timer elapsed and Do Not Disturb is off"
        );

        let switched = Activity {
            switched: true,
            ..working()
        };
        assert!(
            due(
                Duration::ZERO,
                WAKE_EVERY,
                &switched,
                Duration::MAX,
                Duration::ZERO,
                false
            ),
            "frontmost switched and Do Not Disturb is off"
        );
        assert!(
            !due(
                Duration::ZERO,
                WAKE_EVERY,
                &working(),
                Duration::MAX,
                Duration::ZERO,
                false
            ),
            "nothing happened and timer not elapsed"
        );
    }

    #[test]
    fn a_trailing_full_stop_or_colon_still_names_the_behavior() {
        let director = ModelDirector::new(Scripted::says("Prowl."), ["prowl"]);
        match director.wake(&context(working(), &[])) {
            Wake::Proposed(proposal) => {
                assert_eq!(proposal.behavior, "prowl");
            }
            other => panic!("trailing full stop should not prevent match: {other:?}"),
        }

        let colon = ModelDirector::new(Scripted::says("prowl:"), ["prowl"]);
        match colon.wake(&context(working(), &[])) {
            Wake::Proposed(proposal) => {
                assert_eq!(proposal.behavior, "prowl");
            }
            other => panic!("trailing colon should not prevent match: {other:?}"),
        }

        let with_dialogue =
            ModelDirector::new(Scripted::says("PROWL. | hunting"), ["prowl", "wave"]);
        match with_dialogue.wake(&context(working(), &[])) {
            Wake::Proposed(proposal) => {
                assert_eq!(proposal.behavior, "prowl");
                assert_eq!(proposal.dialogue.as_deref(), Some("hunting"));
            }
            other => panic!("trailing punctuation with dialogue: {other:?}"),
        }
    }

    #[test]
    fn a_full_stop_or_colon_after_the_name_is_not_part_of_it() {
        let nap_dot = parse_proposal("nap.").expect("trailing full stop");
        assert_eq!(nap_dot.behavior, "nap");

        let nap_colon = parse_proposal("nap:").expect("trailing colon");
        assert_eq!(nap_colon.behavior, "nap");

        let with_dialogue = parse_proposal("nap: | so sleepy...").expect("colon with dialogue");
        assert_eq!(with_dialogue.behavior, "nap");
        assert_eq!(with_dialogue.dialogue.as_deref(), Some("so sleepy..."));

        let upper = parse_proposal("Nap.").expect("case preserved before declared match");
        assert_eq!(upper.behavior, "Nap");
    }

    #[test]
    fn say_with_nothing_to_say_falls_back_rather_than_saying_say() {
        let director = ModelDirector::new(Scripted::says("say"), ["wave"]);
        match director.wake(&context(working(), &[])) {
            Wake::Failed => {}
            other => panic!("bare say should fail, not {other:?}"),
        }

        let colon = ModelDirector::new(Scripted::says("say:"), ["wave"]);
        match colon.wake(&context(working(), &[])) {
            Wake::Failed => {}
            other => panic!("say: with no dialogue should fail, not {other:?}"),
        }

        let upper = ModelDirector::new(Scripted::says("Say."), ["wave"]);
        match upper.wake(&context(working(), &[])) {
            Wake::Failed => {}
            other => panic!("Say. should fail, not {other:?}"),
        }
    }
}
