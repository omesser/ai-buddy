//! The Static Director: the buddy's life with no model and no network.
//!
//! What wakes it and what it is told are the Shell's; what it picks is here,
//! and it is a pure function of the context and a seed. DESIGN.md decision 5
//! keeps it as the configurable fallback for exactly the moments a model-backed
//! Director cannot serve: none configured, turned off in settings, offline, or
//! answering too slowly to wait for.
//!
//! Determinism is the point. A pet that surprises its owner is not a pet that
//! surprises its tests, so the randomness arrives as a seed rather than from a
//! clock or an operating system. Every test below fixes the seed, including the
//! one about distribution: it counts what a known seed actually drew, so it
//! cannot flake however tight the band around it is drawn.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::character::{Behavior, Trigger};
use crate::engine::BehaviorProposal;
use crate::sensing::Activity;

/// How long the Director goes unwoken when nothing notable happens.
///
/// A tuning knob. Long enough that the sprite is not constantly interrupting
/// itself, short enough that a glance at the desktop usually catches it doing
/// something. The model-backed Director will want its own, far longer, number:
/// this one costs nothing to wake.
pub const WAKE_EVERY: Duration = Duration::from_secs(20);

/// How many Behaviors back the Director is asked to remember.
///
/// A tuning knob, and the reason suppression has to be able to give way: a
/// Character declaring fewer Behaviors than this would otherwise run out of
/// things it is allowed to do.
pub const REMEMBERED: usize = 3;

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
}

/// Whatever decides what the buddy does next.
pub trait Director {
    /// A Behavior to play, or nothing when this moment suits none.
    fn propose(&mut self, context: &Context) -> Option<BehaviorProposal>;
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

/// Whether this read of the Free tier is worth waking the Director for.
///
/// A switch of application is the one thing the user will notice being ignored:
/// they moved to another window and the buddy carried on as though nothing
/// happened. Everything else is the timer.
pub fn due(since_wake: Duration, activity: &Activity) -> bool {
    activity.switched || since_wake >= WAKE_EVERY
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
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
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

        let mut drawn = self.next() % total;
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
        }
    }

    fn context(activity: Activity, recent: &[&str]) -> Context {
        Context {
            activity,
            recent: recent.iter().map(|name| name.to_string()).collect(),
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

    #[test]
    fn the_director_wakes_on_a_switch_and_otherwise_on_the_timer() {
        let switched = Activity {
            switched: true,
            ..working()
        };

        assert!(due(Duration::ZERO, &switched), "a new application is news");
        assert!(!due(Duration::ZERO, &working()), "nothing has happened");
        assert!(!due(WAKE_EVERY - Duration::from_millis(1), &working()));
        assert!(due(WAKE_EVERY, &working()), "the timer comes due");
    }
}
