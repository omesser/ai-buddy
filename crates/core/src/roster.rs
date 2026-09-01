//! The roster of Character Instances: spawn, dismiss, list.
//!
//! One Engine per Instance. Instances differ in name, position, and current
//! Behavior, never in knowledge — Memory is shared.

use crate::character::Character;
use crate::engine::{Engine, Frame, Point, WorldSnapshot};
use crate::memory::MemoryManifest;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A stable identifier for one Character Instance.
///
/// Generated at spawn. A uuid v4 so Instance ids never collide across process
/// restarts (#13).
pub type InstanceId = String;

/// One Instance asked for at launch: which Character to run, and what to call
/// it.
///
/// A request rather than an Instance. The id and the Engine arrive at `spawn`,
/// and nothing here knows whether the Character named can actually be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceSpec {
    pub character: String,
    pub name: String,
}

/// Read a list of Instances to run out of one configuration string.
///
/// The grammar is `character:name`, comma separated, with the name optional.
/// It is small because it is temporary: naming an Instance belongs in #18's
/// menu, and this is what stands in until there is somewhere to type a name.
///
/// Naming nothing is not a failure. An unset variable, an empty one and a
/// stray comma are all the same request — run the default single Instance —
/// because refusing to start over punctuation is a worse answer than starting.
pub fn parse_specs(raw: &str) -> Result<Vec<InstanceSpec>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            // A colon the user typed is a name they meant to give, so an empty
            // half is a typo to report rather than a default to guess at. No
            // colon at all is the shorthand: the package name is the name.
            let (character, name) = match entry.split_once(':') {
                Some((character, name)) => (character.trim(), name.trim()),
                None => (entry, entry),
            };
            if character.is_empty() || name.is_empty() {
                return Err(format!(
                    "{entry:?} is not a Character and a name. Write character:name, \
                     or the Character alone to name it after its package"
                ));
            }
            Ok(InstanceSpec {
                character: character.to_string(),
                name: name.to_string(),
            })
        })
        .collect()
}

/// One spawned buddy: a Character plus a user-given name and a stable id.
pub struct Instance {
    pub id: InstanceId,
    pub name: String,
    character_name: String,
    engine: Engine,
}

impl Instance {
    /// Tick this Instance's Engine forward.
    pub fn tick(&mut self, snapshot: &WorldSnapshot) -> Frame {
        self.engine.tick(snapshot)
    }

    /// The Character this Instance is running.
    pub fn character_name(&self) -> &str {
        &self.character_name
    }

    /// Whether this Instance is in Do Not Disturb, and so refusing to start
    /// things of its own.
    ///
    /// Per Instance rather than per app: the mode says whether *this* buddy
    /// should sit quietly, and silencing every buddy because one was told to
    /// would make the setting mean something else. #84 owns what it does.
    pub fn do_not_disturb(&self) -> bool {
        self.engine.do_not_disturb()
    }
}

/// The roster of Character Instances.
pub struct Roster {
    instances: BTreeMap<InstanceId, Instance>,
    memory: Arc<MemoryManifest>,
}

impl Roster {
    /// Create a new empty Roster with the given Memory.
    pub fn new(memory: MemoryManifest) -> Self {
        Self {
            instances: BTreeMap::new(),
            memory: Arc::new(memory),
        }
    }

    /// Spawn a Character Instance with the given name at the given position.
    ///
    /// Returns the generated stable id.
    ///
    /// Borrowed rather than owned, because the art is the heaviest thing a
    /// Character carries and an Instance needs none of it: taking it by value
    /// would copy every frame of every Animation per buddy, which is the cost
    /// running several of one Character exists to avoid.
    pub fn spawn(&mut self, character: &Character, name: String, position: Point) -> InstanceId {
        let id = uuid::Uuid::new_v4().to_string();
        let engine = Engine::new(position).with_behaviors(character.behaviors.clone());
        let instance = Instance {
            id: id.clone(),
            name,
            character_name: character.name.clone(),
            engine,
        };
        self.instances.insert(id.clone(), instance);
        id
    }

    /// Dismiss the Instance with the given id.
    ///
    /// Memory is untouched.
    pub fn dismiss(&mut self, id: &str) -> Option<Instance> {
        self.instances.remove(id)
    }

    /// List all Instances: id and name.
    pub fn list(&self) -> Vec<(InstanceId, String)> {
        self.instances
            .iter()
            .map(|(id, instance)| (id.clone(), instance.name.clone()))
            .collect()
    }

    /// Get a mutable reference to an Instance by id.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Instance> {
        self.instances.get_mut(id)
    }

    /// Get a reference to an Instance by id.
    pub fn get(&self, id: &str) -> Option<&Instance> {
        self.instances.get(id)
    }

    /// The shared Memory.
    pub fn memory(&self) -> &Arc<MemoryManifest> {
        &self.memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{
        Animation, Behavior, Character, Primitive, DEFAULT_MODEL_BASE, DEFAULT_MODEL_POWER,
    };
    use crate::engine::{Point, Rect, Verb};
    use crate::memory::MemoryManifest;
    use std::collections::BTreeMap;

    /// A minimal test Character with the Required Animation Set.
    fn test_character(name: &str) -> Character {
        let mut animations = BTreeMap::new();
        let required = [
            "idle", "walk", "fall", "land", "sit", "sleep", "react", "talk", "hold",
        ];
        for anim in required {
            animations.insert(
                anim.to_string(),
                Animation {
                    frames: vec![format!("{anim}-0.png")],
                    frame_size: (32, 32),
                    fps: 8,
                    looping: true,
                    variants: Vec::new(),
                },
            );
        }

        let mut behaviors = BTreeMap::new();
        behaviors.insert(
            "wave".to_string(),
            Behavior {
                primitives: vec![Primitive::React],
                then: None,
                weight: 1,
                trigger: None,
            },
        );
        behaviors.insert(
            "sleep".to_string(),
            Behavior {
                primitives: vec![Primitive::Sleep],
                then: None,
                weight: 1,
                trigger: None,
            },
        );

        Character {
            name: name.to_string(),
            personality: format!("A test character named {name}"),
            animations,
            behaviors,
            art: BTreeMap::new(),
            smooth: false,
            scale: 1,
            model_base: DEFAULT_MODEL_BASE,
            model_power: DEFAULT_MODEL_POWER,
        }
    }

    fn test_snapshot() -> WorldSnapshot {
        WorldSnapshot {
            displays: vec![Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            }],
            windows: vec![],
            cursor: Point { x: 100.0, y: 100.0 },
            verbs: vec![],
            elapsed_ms: 16,
            proposal: None,
            poll_generation: 0,
        }
    }

    #[test]
    fn spawning_an_instance_records_the_given_name_and_a_stable_id() {
        let memory = MemoryManifest::new(std::env::temp_dir().join("test-spawn.md"));
        let mut roster = Roster::new(memory);
        let character = test_character("Blip");

        let id = roster.spawn(
            &character,
            "Buddy One".to_string(),
            Point { x: 100.0, y: 100.0 },
        );

        assert!(!id.is_empty(), "the id is not empty");
        assert_eq!(
            roster.list(),
            vec![(id.clone(), "Buddy One".to_string())],
            "the Instance appears in the roster with its name"
        );
    }

    #[test]
    fn two_instances_of_the_same_character_play_different_behaviors_independently() {
        let memory = MemoryManifest::new(std::env::temp_dir().join("test-independent.md"));
        let mut roster = Roster::new(memory);
        let character = test_character("Blip");

        let id_a = roster.spawn(
            &character,
            "Buddy A".to_string(),
            Point {
                x: 100.0,
                y: 1000.0,
            },
        );
        let id_b = roster.spawn(
            &character,
            "Buddy B".to_string(),
            Point {
                x: 500.0,
                y: 1000.0,
            },
        );

        let grounded_snapshot = test_snapshot();
        for _ in 0..50 {
            roster.get_mut(&id_a).unwrap().tick(&grounded_snapshot);
            roster.get_mut(&id_b).unwrap().tick(&grounded_snapshot);
        }

        let frame_a_check = roster.get_mut(&id_a).unwrap().tick(&grounded_snapshot);
        let frame_b_check = roster.get_mut(&id_b).unwrap().tick(&grounded_snapshot);

        assert!(
            frame_a_check.state == crate::engine::State::Grounded,
            "Instance A is grounded before behavior proposal, state: {:?}",
            frame_a_check.state
        );
        assert!(
            frame_b_check.state == crate::engine::State::Grounded,
            "Instance B is grounded before behavior proposal, state: {:?}",
            frame_b_check.state
        );

        let mut snapshot_wave = test_snapshot();
        snapshot_wave.proposal = Some(crate::engine::BehaviorProposal {
            behavior: "wave".to_string(),
            dialogue: None,
        });

        let mut snapshot_sleep = test_snapshot();
        snapshot_sleep.proposal = Some(crate::engine::BehaviorProposal {
            behavior: "sleep".to_string(),
            dialogue: None,
        });

        let frame_a = roster.get_mut(&id_a).unwrap().tick(&snapshot_wave);
        let frame_b = roster.get_mut(&id_b).unwrap().tick(&snapshot_sleep);

        assert_eq!(
            frame_a.animation, "react",
            "Instance A plays wave (which starts with react)"
        );
        assert_eq!(
            frame_b.animation, "sleep",
            "Instance B plays sleep independently"
        );
        assert_ne!(
            frame_a.animation, frame_b.animation,
            "the two Instances are playing different animations"
        );
    }

    #[test]
    fn two_instances_are_independently_positionable() {
        let memory = MemoryManifest::new(std::env::temp_dir().join("test-position.md"));
        let mut roster = Roster::new(memory);
        let character = test_character("Blip");

        let id_a = roster.spawn(
            &character,
            "Buddy A".to_string(),
            Point { x: 100.0, y: 200.0 },
        );
        let id_b = roster.spawn(
            &character,
            "Buddy B".to_string(),
            Point { x: 500.0, y: 600.0 },
        );

        let mut snapshot_a = test_snapshot();
        snapshot_a.verbs = vec![Verb::Grab];
        snapshot_a.cursor = Point { x: 120.0, y: 220.0 };

        let mut snapshot_b = test_snapshot();
        snapshot_b.verbs = vec![Verb::Grab];
        snapshot_b.cursor = Point { x: 520.0, y: 620.0 };

        let frame_a = roster.get_mut(&id_a).unwrap().tick(&snapshot_a);
        let frame_b = roster.get_mut(&id_b).unwrap().tick(&snapshot_b);

        assert_ne!(
            frame_a.position, frame_b.position,
            "the two Instances have different positions"
        );
        assert!(
            (frame_a.position.x - 120.0).abs() < 50.0,
            "Instance A is near its cursor position"
        );
        assert!(
            (frame_b.position.x - 520.0).abs() < 50.0,
            "Instance B is near its cursor position"
        );
    }

    #[test]
    fn listing_returns_both_instances_and_dismissing_one_leaves_the_other() {
        let memory = MemoryManifest::new(std::env::temp_dir().join("test-dismiss.md"));
        let mut roster = Roster::new(memory);
        let character = test_character("Blip");

        let id_a = roster.spawn(
            &character,
            "Buddy A".to_string(),
            Point { x: 100.0, y: 100.0 },
        );
        let id_b = roster.spawn(
            &character,
            "Buddy B".to_string(),
            Point { x: 500.0, y: 100.0 },
        );

        let list = roster.list();
        assert_eq!(list.len(), 2, "both Instances appear");
        assert!(
            list.iter().any(|(_, name)| name == "Buddy A"),
            "Buddy A is listed"
        );
        assert!(
            list.iter().any(|(_, name)| name == "Buddy B"),
            "Buddy B is listed"
        );

        let dismissed = roster.dismiss(&id_a);
        assert!(dismissed.is_some(), "dismissing returns the Instance");
        assert_eq!(dismissed.unwrap().name, "Buddy A");

        let list = roster.list();
        assert_eq!(list.len(), 1, "one Instance remains");
        assert_eq!(list[0], (id_b.clone(), "Buddy B".to_string()));

        roster.dismiss(&id_b);
        assert_eq!(
            roster.list().len(),
            0,
            "an empty list after dismissing both"
        );
    }

    #[test]
    fn memory_is_shared_remember_via_one_instance_recall_from_the_roster() {
        let temp_dir = std::env::temp_dir();
        let memory_path = temp_dir.join(format!("test-shared-{}.md", std::process::id()));
        let _ = std::fs::remove_file(&memory_path);

        let memory = MemoryManifest::new(&memory_path);
        let roster = Roster::new(memory);

        roster
            .memory()
            .remember("Facts", "Oded's cat is called Simba")
            .expect("remembering writes");

        let recalled = roster.memory().recall().expect("recall reads");
        assert!(
            recalled.contains("Simba"),
            "the fact is visible through the roster's Memory"
        );

        let _ = std::fs::remove_file(&memory_path);
    }

    #[test]
    fn dismissing_an_instance_does_not_delete_memory() {
        let temp_dir = std::env::temp_dir();
        let memory_path = temp_dir.join(format!("test-dismiss-memory-{}.md", std::process::id()));
        let _ = std::fs::remove_file(&memory_path);

        let memory = MemoryManifest::new(&memory_path);
        let mut roster = Roster::new(memory);
        let character = test_character("Blip");

        roster
            .memory()
            .remember("Facts", "Oded lives in Tel Aviv")
            .expect("remembering writes");

        let id = roster.spawn(
            &character,
            "Buddy".to_string(),
            Point { x: 100.0, y: 100.0 },
        );
        roster.dismiss(&id);

        let recalled = roster.memory().recall().expect("recall still works");
        assert!(
            recalled.contains("Tel Aviv"),
            "Memory survives dismissing the Instance"
        );

        let _ = std::fs::remove_file(&memory_path);
    }

    /// The launch configuration is the only way to name Instances until #18's
    /// menu exists, so what it accepts is the whole of what a user can ask for.
    #[test]
    fn one_spec_names_a_character_and_what_to_call_it() {
        assert_eq!(
            parse_specs("bmo:Blip"),
            Ok(vec![InstanceSpec {
                character: "bmo".to_string(),
                name: "Blip".to_string(),
            }])
        );
    }

    /// Whitespace around either half is a user typing a list, not a Character
    /// whose name begins with a space.
    #[test]
    fn several_specs_are_read_in_order_and_whitespace_is_trimmed() {
        assert_eq!(
            parse_specs(" bmo:Blip ,  jotaro:Jo "),
            Ok(vec![
                InstanceSpec {
                    character: "bmo".to_string(),
                    name: "Blip".to_string(),
                },
                InstanceSpec {
                    character: "jotaro".to_string(),
                    name: "Jo".to_string(),
                },
            ])
        );
    }

    /// A Character named alone is the common case — one of each, called what
    /// the package is called — and demanding `bmo:bmo` for it would be a tax on
    /// the shortest thing anyone will write.
    #[test]
    fn a_character_named_alone_is_called_after_its_package() {
        assert_eq!(
            parse_specs("bmo,jotaro:Jo"),
            Ok(vec![
                InstanceSpec {
                    character: "bmo".to_string(),
                    name: "bmo".to_string(),
                },
                InstanceSpec {
                    character: "jotaro".to_string(),
                    name: "Jo".to_string(),
                },
            ])
        );
    }

    /// Two Instances of one Character is the case #13 exists for, and they are
    /// told apart by their names rather than by their Characters.
    #[test]
    fn the_same_character_twice_is_two_specs() {
        let specs = parse_specs("bmo:One,bmo:Two").expect("both parse");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].character, specs[1].character);
        assert_ne!(specs[0].name, specs[1].name);
    }

    /// Nothing named is not an error. An unset variable and one set to a
    /// trailing comma are the same request — run the default single Instance —
    /// and refusing to start over a stray comma would be a poor trade.
    #[test]
    fn nothing_named_asks_for_no_instances_rather_than_failing() {
        assert_eq!(parse_specs(""), Ok(Vec::new()));
        assert_eq!(parse_specs("   "), Ok(Vec::new()));
        assert_eq!(parse_specs(",,"), Ok(Vec::new()));
        assert_eq!(
            parse_specs("bmo:Blip,"),
            Ok(vec![InstanceSpec {
                character: "bmo".to_string(),
                name: "Blip".to_string(),
            }])
        );
    }

    /// A colon the user typed is a name they meant to give, so an empty half is
    /// a typo to report rather than a default to guess at.
    #[test]
    fn a_colon_with_either_half_missing_is_reported() {
        assert!(parse_specs("bmo:").is_err(), "no name after the colon");
        assert!(parse_specs(":Blip").is_err(), "no Character before it");
        assert!(parse_specs("bmo:Blip,:Jo").is_err(), "the second is bad");
    }

    /// The message names the offending entry. A list of several is read back by
    /// finding which one is wrong, and "invalid" alone does not say.
    #[test]
    fn the_error_quotes_the_entry_that_could_not_be_read() {
        let why = parse_specs("bmo:Blip,nope:").expect_err("the second is bad");
        assert!(why.contains("nope:"), "the entry is quoted: {why}");
    }
}
