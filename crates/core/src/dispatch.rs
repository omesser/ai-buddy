//! MCP tool dispatch.
//!
//! Maps tool names and JSON arguments onto the tool handlers from tools.rs.
//! Tested in-process without an MCP transport, so it can live in core beside
//! the handlers it wraps.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::memory::MemoryManifest;
use crate::tools::{self, DenyList, ExpressionHandle, InstanceInfo};
use crate::window_source::WindowSource;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownTool,
    InvalidArguments,
    ExecutionFailed,
}

pub struct DispatchContext<'a> {
    pub window_source: &'a dyn WindowSource,
    pub memory_path: PathBuf,
    pub denylist: DenyList,
    pub roster: &'a [InstanceInfo],
    pub expression: Option<&'a mut dyn ExpressionHandle>,
}

/// Reborrow a taken handle for one call. `as_deref_mut` on `Option<&mut dyn>`
/// is stuck at the inner lifetime, so the handle could not be stored back.
fn as_expression_handle<'short>(
    expression: &'short mut Option<&mut dyn ExpressionHandle>,
) -> Option<&'short mut dyn ExpressionHandle> {
    match expression {
        Some(handle) => Some(&mut **handle),
        None => None,
    }
}

fn parse_args<T: for<'de> Deserialize<'de>>(
    arguments: Value,
    tool_name: &str,
) -> Result<T, DispatchError> {
    serde_json::from_value(arguments).map_err(|e| DispatchError {
        code: ErrorCode::InvalidArguments,
        message: format!("Invalid arguments for {}: {}", tool_name, e),
    })
}

pub fn dispatch(
    tool_name: &str,
    arguments: Value,
    context: &mut DispatchContext,
) -> Result<Value, DispatchError> {
    match tool_name {
        "speak" => {
            #[derive(Deserialize)]
            struct Args {
                message: String,
                #[serde(default)]
                instance_id: Option<String>,
            }
            let args: Args = parse_args(arguments, tool_name)?;
            let roster = context.roster;
            // take() rather than as_deref_mut() on the field: Option<&mut dyn>
            // is invariant, and `&'a mut DispatchContext<'a>` would extend that
            // borrow past every caller. The local is assigned back so a reused
            // context keeps the handle.
            let mut expression = context.expression.take();
            let result = tools::speak(
                &args.message,
                args.instance_id.as_deref(),
                roster,
                as_expression_handle(&mut expression),
            );
            context.expression = expression;
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "play_behavior" => {
            #[derive(Deserialize)]
            struct Args {
                behavior: String,
                #[serde(default)]
                instance_id: Option<String>,
            }
            let args: Args = parse_args(arguments, tool_name)?;
            let roster = context.roster;
            let mut expression = context.expression.take();
            let result = tools::play_behavior(
                &args.behavior,
                args.instance_id.as_deref(),
                roster,
                as_expression_handle(&mut expression),
            );
            context.expression = expression;
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "list_windows" => {
            let result = tools::list_windows(context.window_source, &context.denylist);
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "describe_screen" => {
            let result = tools::describe_screen(context.window_source, &context.denylist);
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "recall" => {
            let memory = MemoryManifest::new(&context.memory_path);
            let result = tools::recall(&memory).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to recall: {}", e),
            })?;
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "remember" => {
            #[derive(Deserialize)]
            struct Args {
                heading: String,
                fact: String,
            }
            let args: Args = parse_args(arguments, tool_name)?;
            let memory = MemoryManifest::new(&context.memory_path);
            let result =
                tools::remember(&memory, &args.heading, &args.fact).map_err(|e| DispatchError {
                    code: ErrorCode::ExecutionFailed,
                    message: format!("Failed to remember: {}", e),
                })?;
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "list_instances" => {
            let result = tools::list_instances(context.roster);
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        _ => Err(DispatchError {
            code: ErrorCode::UnknownTool,
            message: format!("Unknown tool: {}", tool_name),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Returns exactly the seven tools from #15 / tools.rs, and none that post
/// input events.
pub fn list_tools() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "speak".to_string(),
            description: "Make the Character speak a line of dialogue".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to speak"
                    },
                    "instance_id": {
                        "type": "string",
                        "description": "Optional instance ID to target"
                    }
                },
                "required": ["message"]
            }),
        },
        ToolInfo {
            name: "play_behavior".to_string(),
            description: "Play a named Behavior".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "behavior": {
                        "type": "string",
                        "description": "The name of the Behavior to play"
                    },
                    "instance_id": {
                        "type": "string",
                        "description": "Optional instance ID to target"
                    }
                },
                "required": ["behavior"]
            }),
        },
        ToolInfo {
            name: "list_windows".to_string(),
            description: "List visible windows with bounds and owning application".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolInfo {
            name: "describe_screen".to_string(),
            description: "Describe what is on screen (v1: window metadata only)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolInfo {
            name: "recall".to_string(),
            description: "Recall everything Memory holds".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolInfo {
            name: "remember".to_string(),
            description: "Remember one fact under a heading".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "heading": {
                        "type": "string",
                        "description": "The heading under which to record the fact"
                    },
                    "fact": {
                        "type": "string",
                        "description": "The fact to remember"
                    }
                },
                "required": ["heading", "fact"]
            }),
        },
        ToolInfo {
            name: "list_instances".to_string(),
            description: "List Character Instances and their names".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_source::{Capabilities, FakeWindowSource, Rect, WindowRect, WorldGeometry};
    use serde_json::json;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "ai-buddy-dispatch-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("temp dir is creatable");
            Self(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn window(owner: &str, x: f64, y: f64, width: f64, height: f64) -> WindowRect {
        WindowRect {
            id: 0,
            bounds: Rect {
                x,
                y,
                width,
                height,
            },
            owner: owner.to_string(),
            layer: 0,
        }
    }

    fn test_context<'a>(
        temp: &TempDir,
        source: &'a FakeWindowSource,
        roster: &'a [InstanceInfo],
    ) -> DispatchContext<'a> {
        DispatchContext {
            window_source: source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster,
            expression: None,
        }
    }

    fn fake_source(windows: Vec<WindowRect>) -> FakeWindowSource {
        FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                }],
                windows,
                dock: None,
            },
        }
    }

    #[test]
    fn dispatch_speak_returns_speak_result() {
        let temp = TempDir::new("speak");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({"message": "Hello, world"});
        let result = dispatch("speak", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello, world");
    }

    #[test]
    fn dispatch_play_behavior_returns_play_behavior_result() {
        let temp = TempDir::new("play");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({"behavior": "wave"});
        let result = dispatch("play_behavior", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["behavior"], "wave");
    }

    #[test]
    fn dispatch_list_windows_returns_list_windows_result() {
        let temp = TempDir::new("list-windows");
        let source = fake_source(vec![window("Terminal", 10.0, 20.0, 800.0, 600.0)]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("list_windows", args, &mut context).expect("dispatch succeeds");

        let windows = result["windows"].as_array().expect("windows is array");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["owner"], "Terminal");
    }

    #[test]
    fn dispatch_describe_screen_returns_describe_screen_result() {
        let temp = TempDir::new("describe");
        let source = fake_source(vec![window("Safari", 30.0, 40.0, 1200.0, 800.0)]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("describe_screen", args, &mut context).expect("dispatch succeeds");

        let description = result["description"]
            .as_str()
            .expect("description is string");
        assert!(description.contains("1 visible window"));
        assert!(description.contains("Safari"));
    }

    #[test]
    fn dispatch_recall_returns_recall_result() {
        let temp = TempDir::new("recall");
        let memory = MemoryManifest::new(temp.join("memory.md"));
        memory
            .remember("Facts", "The user likes coffee")
            .expect("remembering writes");

        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("recall", args, &mut context).expect("dispatch succeeds");

        let content = result["content"].as_str().expect("content is string");
        assert!(content.contains("The user likes coffee"));
    }

    #[test]
    fn dispatch_remember_returns_remember_result() {
        let temp = TempDir::new("remember");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({"heading": "Facts", "fact": "The user's name is Oded"});
        let result = dispatch("remember", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["recorded"], "- The user's name is Oded");
    }

    #[test]
    fn dispatch_list_instances_returns_list_instances_result() {
        let temp = TempDir::new("list-instances");
        let roster = [
            InstanceInfo {
                id: "inst-1".to_string(),
                name: "Clippy".to_string(),
            },
            InstanceInfo {
                id: "inst-2".to_string(),
                name: "Ferris".to_string(),
            },
        ];
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &roster);

        let args = json!({});
        let result = dispatch("list_instances", args, &mut context).expect("dispatch succeeds");

        let instances = result["instances"].as_array().expect("instances is array");
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0]["id"], "inst-1");
        assert_eq!(instances[0]["name"], "Clippy");
    }

    #[test]
    fn dispatch_unknown_tool_returns_error() {
        let temp = TempDir::new("unknown");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("click_mouse", args, &mut context);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::UnknownTool);
        assert!(err.message.contains("click_mouse"));
    }

    #[test]
    fn dispatch_with_invalid_arguments_returns_error() {
        let temp = TempDir::new("bad-args");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({"wrong_field": 42});
        let result = dispatch("speak", args, &mut context);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidArguments);
    }

    #[test]
    fn list_tools_returns_exactly_seven_tools() {
        let tools = list_tools();

        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn list_tools_includes_all_handler_tools() {
        let tools = list_tools();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"speak"));
        assert!(names.contains(&"play_behavior"));
        assert!(names.contains(&"list_windows"));
        assert!(names.contains(&"describe_screen"));
        assert!(names.contains(&"recall"));
        assert!(names.contains(&"remember"));
        assert!(names.contains(&"list_instances"));
    }

    #[test]
    fn list_tools_includes_no_input_event_tools() {
        let tools = list_tools();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(!names.contains(&"click"));
        assert!(!names.contains(&"type"));
        assert!(!names.contains(&"press_key"));
        assert!(!names.contains(&"move_mouse"));
    }

    #[test]
    fn dispatch_list_windows_applies_denylist() {
        let temp = TempDir::new("denylist-list");
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };
        let source = fake_source(vec![
            window("Terminal", 10.0, 20.0, 800.0, 600.0),
            window("1Password", 30.0, 40.0, 400.0, 300.0),
        ]);
        let mut context = test_context(&temp, &source, &[]);
        context.denylist = denylist;

        let args = json!({});
        let result = dispatch("list_windows", args, &mut context).expect("dispatch succeeds");

        let windows = result["windows"].as_array().expect("windows is array");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["owner"], "Terminal");
    }

    #[test]
    fn dispatch_describe_screen_applies_denylist() {
        let temp = TempDir::new("denylist-describe");
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };
        let source = fake_source(vec![
            window("Terminal", 10.0, 20.0, 800.0, 600.0),
            window("1Password", 30.0, 40.0, 400.0, 300.0),
        ]);
        let mut context = test_context(&temp, &source, &[]);
        context.denylist = denylist;

        let args = json!({});
        let result = dispatch("describe_screen", args, &mut context).expect("dispatch succeeds");

        let description = result["description"]
            .as_str()
            .expect("description is string");
        assert!(description.contains("Terminal"));
        assert!(!description.contains("1Password"));
    }

    #[test]
    fn dispatch_list_instances_with_empty_roster_returns_empty_list() {
        let temp = TempDir::new("empty-roster");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("list_instances", args, &mut context).expect("dispatch succeeds");

        let instances = result["instances"].as_array().expect("instances is array");
        assert_eq!(instances.len(), 0);
    }

    /// Copy test helpers from roster.rs
    fn test_character(name: &str) -> crate::character::Character {
        use crate::character::{
            Animation, Behavior, Character, CursorReaction, Primitive, DEFAULT_MODEL_BASE,
            DEFAULT_MODEL_POWER,
        };
        use std::collections::BTreeMap;

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
            near_reaction: CursorReaction::default(),
            rush_reaction: CursorReaction::default(),
        }
    }

    fn test_snapshot() -> crate::engine::WorldSnapshot {
        use crate::engine::{Point, Rect, WorldSnapshot};

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

    /// Build roster with grounded instance - settle ~50 ticks so sprite is Grounded
    fn test_roster_with_grounded_instance(
        name: &str,
        memory_path: &std::path::Path,
    ) -> (crate::roster::Roster, String) {
        use crate::engine::Point;
        use crate::memory::MemoryManifest;

        let memory = MemoryManifest::new(memory_path.to_path_buf());
        let mut roster = crate::roster::Roster::new(memory);
        let character = test_character(name);
        let id = roster.spawn(&character, name.to_string(), Point { x: 100.0, y: 100.0 });

        // Settle ~50 ticks so the sprite is Grounded before proposing
        let mut snapshot = test_snapshot();
        for _ in 0..50 {
            if let Some(instance) = roster.get_mut(&id) {
                instance.tick(&snapshot);
            }
            snapshot.elapsed_ms = 16; // ~60fps
        }

        (roster, id)
    }

    #[test]
    fn reused_dispatch_context_keeps_the_expression_handle() {
        use std::cell::RefCell;
        use std::rc::Rc;

        struct SharedRoster(Rc<RefCell<crate::roster::Roster>>);

        impl crate::tools::ExpressionHandle for SharedRoster {
            fn enqueue(
                &mut self,
                instance_id: &str,
                proposal: crate::engine::BehaviorProposal,
            ) -> bool {
                crate::tools::ExpressionHandle::enqueue(
                    &mut *self.0.borrow_mut(),
                    instance_id,
                    proposal,
                )
            }
        }

        let temp = TempDir::new("reuse-handle");
        let source = fake_source(vec![]);
        let (roster, instance_id) =
            test_roster_with_grounded_instance("TestBuddy", &temp.join("expression.md"));

        let shared = Rc::new(RefCell::new(roster));
        let roster_info = shared
            .borrow()
            .list()
            .into_iter()
            .map(|(id, name)| InstanceInfo { id, name })
            .collect::<Vec<_>>();

        let mut expression = SharedRoster(Rc::clone(&shared));
        let mut context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &roster_info,
            expression: Some(&mut expression),
        };

        let first = dispatch("speak", json!({"message": "First line"}), &mut context)
            .expect("dispatch succeeds");
        assert_eq!(first["success"], true);

        let snapshot = test_snapshot();
        let first_frame = shared
            .borrow_mut()
            .get_mut(&instance_id)
            .expect("instance still in roster")
            .tick(&snapshot);
        assert_eq!(first_frame.dialogue.as_deref(), Some("First line"));

        let second = dispatch("speak", json!({"message": "Second line"}), &mut context)
            .expect("dispatch succeeds");
        assert_eq!(second["success"], true);

        let second_frame = shared
            .borrow_mut()
            .get_mut(&instance_id)
            .expect("instance still in roster")
            .tick(&snapshot);
        assert_eq!(second_frame.dialogue.as_deref(), Some("Second line"));
    }

    #[test]
    fn speak_with_one_instance_enqueues_dialogue_and_plays_talk() {
        let temp = TempDir::new("speak-expression");
        let source = fake_source(vec![]);
        let (mut roster, instance_id) =
            test_roster_with_grounded_instance("TestBuddy", &temp.join("expression.md"));

        // Build roster info from the roster
        let roster_info = roster
            .list()
            .into_iter()
            .map(|(id, name)| InstanceInfo { id, name })
            .collect::<Vec<_>>();

        let mut context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &roster_info,
            expression: Some(&mut roster),
        };

        let args = json!({"message": "Hello, world!"});
        let result = dispatch("speak", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello, world!");
        drop(context);

        // The Engine should have received the dialogue proposal and play talk
        let snapshot = test_snapshot();
        let instance = roster
            .get_mut(&instance_id)
            .expect("instance still in roster");
        let frame = instance.tick(&snapshot);
        assert!(frame.dialogue.is_some(), "Frame should carry dialogue");
        assert_eq!(frame.dialogue.as_ref().unwrap(), "Hello, world!");
        assert_eq!(frame.animation, "talk");
    }

    #[test]
    fn play_behavior_with_one_instance_delivers_the_proposal_to_the_engine() {
        let temp = TempDir::new("behavior-expression");
        let source = fake_source(vec![]);
        let (mut roster, instance_id) =
            test_roster_with_grounded_instance("TestBuddy", &temp.join("expression.md"));

        let roster_info = roster
            .list()
            .into_iter()
            .map(|(id, name)| InstanceInfo { id, name })
            .collect::<Vec<_>>();

        let mut context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &roster_info,
            expression: Some(&mut roster),
        };

        let args = json!({"behavior": "wave"});
        let result = dispatch("play_behavior", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["behavior"], "wave");
        drop(context);

        // wave's Primitive is React -> Frame.animation == "react" and Frame.behavior == Some("wave")
        let snapshot = test_snapshot();
        let instance = roster
            .get_mut(&instance_id)
            .expect("instance still in roster");
        let frame = instance.tick(&snapshot);
        assert_eq!(frame.animation, "react");
        assert_eq!(frame.behavior, Some("wave".to_string()));
    }

    #[test]
    fn omitted_instance_id_with_several_instances_is_failure() {
        let temp = TempDir::new("multi-instance");
        let source = fake_source(vec![]);
        let infos = [
            InstanceInfo {
                id: "instance-1".to_string(),
                name: "First".to_string(),
            },
            InstanceInfo {
                id: "instance-2".to_string(),
                name: "Second".to_string(),
            },
        ];
        let mut context = test_context(&temp, &source, &infos);

        let args = json!({"message": "Hello"});
        let result = dispatch("speak", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], false);
        assert_eq!(result["message"], "Hello");
    }

    #[test]
    fn unknown_instance_id_is_failure() {
        let temp = TempDir::new("unknown-instance");
        let source = fake_source(vec![]);
        let infos = [InstanceInfo {
            id: "known-instance".to_string(),
            name: "Known".to_string(),
        }];
        let mut context = test_context(&temp, &source, &infos);

        let args = json!({"message": "Hello", "instance_id": "unknown-instance"});
        let result = dispatch("speak", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], false);
        assert_eq!(result["message"], "Hello");
    }

    #[test]
    fn empty_roster_without_a_handle_keeps_the_stub_success_shape() {
        let temp = TempDir::new("empty-roster");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let args = json!({"message": "Hello, world"});
        let result = dispatch("speak", args, &mut context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello, world");
    }

    /// Break: empty message / empty behavior start reporting success, or become DispatchError.
    #[test]
    fn empty_message_and_empty_behavior_keep_the_existing_failure_shape() {
        let temp = TempDir::new("empty-inputs");
        let source = fake_source(vec![]);
        let mut context = test_context(&temp, &source, &[]);

        let speak_args = json!({"message": ""});
        let speak_result = dispatch("speak", speak_args, &mut context).expect("dispatch succeeds");
        assert_eq!(speak_result["success"], false);
        assert_eq!(speak_result["message"], "");

        let behavior_args = json!({"behavior": ""});
        let behavior_result =
            dispatch("play_behavior", behavior_args, &mut context).expect("dispatch succeeds");
        assert_eq!(behavior_result["success"], false);
        assert_eq!(behavior_result["behavior"], "");
    }

    #[test]
    fn an_undeclared_behavior_is_still_enqueued_and_the_engine_refuses() {
        let temp = TempDir::new("undeclared-behavior");
        let source = fake_source(vec![]);
        let (mut roster, instance_id) =
            test_roster_with_grounded_instance("TestBuddy", &temp.join("expression.md"));

        let roster_info = roster
            .list()
            .into_iter()
            .map(|(id, name)| InstanceInfo { id, name })
            .collect::<Vec<_>>();

        let mut context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &roster_info,
            expression: Some(&mut roster),
        };

        let args = json!({"behavior": "undeclared_behavior"});
        let result = dispatch("play_behavior", args, &mut context).expect("dispatch succeeds");

        // Tool should still report success and enqueue
        assert_eq!(result["success"], true);
        assert_eq!(result["behavior"], "undeclared_behavior");
        drop(context);

        // But Engine should refuse it - Frame.behavior is None
        let snapshot = test_snapshot();
        let instance = roster
            .get_mut(&instance_id)
            .expect("instance still in roster");
        let frame = instance.tick(&snapshot);
        assert_eq!(frame.behavior, None);
    }
}
