//! MCP tool dispatch.
//!
//! Maps tool names and JSON arguments onto the tool handlers from tools.rs.
//! Tested in-process without an MCP transport, so it can live in core beside
//! the handlers it wraps.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::memory::MemoryManifest;
use crate::tools::{self, DenyList, InstanceInfo};
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
    context: &DispatchContext,
) -> Result<Value, DispatchError> {
    match tool_name {
        "speak" => {
            #[derive(Deserialize)]
            struct Args {
                message: String,
            }
            let args: Args = parse_args(arguments, tool_name)?;
            let result = tools::speak(&args.message);
            serde_json::to_value(&result).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to serialize result: {}", e),
            })
        }
        "play_behavior" => {
            #[derive(Deserialize)]
            struct Args {
                behavior: String,
            }
            let args: Args = parse_args(arguments, tool_name)?;
            let result = tools::play_behavior(&args.behavior);
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
            // Any id: these tests read geometry and owner, never identity.
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
            },
        }
    }

    #[test]
    fn dispatch_speak_returns_speak_result() {
        let temp = TempDir::new("speak");
        let source = fake_source(vec![]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({"message": "Hello, world"});
        let result = dispatch("speak", args, &context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello, world");
    }

    #[test]
    fn dispatch_play_behavior_returns_play_behavior_result() {
        let temp = TempDir::new("play");
        let source = fake_source(vec![]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({"behavior": "wave"});
        let result = dispatch("play_behavior", args, &context).expect("dispatch succeeds");

        assert_eq!(result["success"], true);
        assert_eq!(result["behavior"], "wave");
    }

    #[test]
    fn dispatch_list_windows_returns_list_windows_result() {
        let temp = TempDir::new("list-windows");
        let source = fake_source(vec![window("Terminal", 10.0, 20.0, 800.0, 600.0)]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("list_windows", args, &context).expect("dispatch succeeds");

        let windows = result["windows"].as_array().expect("windows is array");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["owner"], "Terminal");
    }

    #[test]
    fn dispatch_describe_screen_returns_describe_screen_result() {
        let temp = TempDir::new("describe");
        let source = fake_source(vec![window("Safari", 30.0, 40.0, 1200.0, 800.0)]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("describe_screen", args, &context).expect("dispatch succeeds");

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
        let context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("recall", args, &context).expect("dispatch succeeds");

        let content = result["content"].as_str().expect("content is string");
        assert!(content.contains("The user likes coffee"));
    }

    #[test]
    fn dispatch_remember_returns_remember_result() {
        let temp = TempDir::new("remember");
        let source = fake_source(vec![]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({"heading": "Facts", "fact": "The user's name is Oded"});
        let result = dispatch("remember", args, &context).expect("dispatch succeeds");

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
        let context = test_context(&temp, &source, &roster);

        let args = json!({});
        let result = dispatch("list_instances", args, &context).expect("dispatch succeeds");

        let instances = result["instances"].as_array().expect("instances is array");
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0]["id"], "inst-1");
        assert_eq!(instances[0]["name"], "Clippy");
    }

    #[test]
    fn dispatch_unknown_tool_returns_error() {
        let temp = TempDir::new("unknown");
        let source = fake_source(vec![]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("click_mouse", args, &context);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::UnknownTool);
        assert!(err.message.contains("click_mouse"));
    }

    #[test]
    fn dispatch_with_invalid_arguments_returns_error() {
        let temp = TempDir::new("bad-args");
        let source = fake_source(vec![]);
        let context = test_context(&temp, &source, &[]);

        let args = json!({"wrong_field": 42});
        let result = dispatch("speak", args, &context);

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
        let result = dispatch("list_windows", args, &context).expect("dispatch succeeds");

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
        let result = dispatch("describe_screen", args, &context).expect("dispatch succeeds");

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
        let context = test_context(&temp, &source, &[]);

        let args = json!({});
        let result = dispatch("list_instances", args, &context).expect("dispatch succeeds");

        let instances = result["instances"].as_array().expect("instances is array");
        assert_eq!(instances.len(), 0);
    }
}
