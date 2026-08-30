//! MCP tool dispatch.
//!
//! Maps tool names and JSON arguments onto the tool handlers from tools.rs.
//! Tested in-process without an MCP transport, so it can live in core beside
//! the handlers it wraps.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::memory::MemoryManifest;
use crate::tools::{self, DenyList};
use crate::window_source::WindowSource;

/// A successful tool dispatch result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResult {
    Speak(tools::SpeakResult),
    PlayBehavior(tools::PlayBehaviorResult),
    ListWindows(tools::ListWindowsResult),
    DescribeScreen(tools::DescribeScreenResult),
    Recall(tools::RecallResult),
    Remember(tools::RememberResult),
    ListInstances(tools::ListInstancesResult),
}

/// A tool dispatch error.
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

/// Dispatch context carrying dependencies the handlers need.
pub struct DispatchContext<'a> {
    pub window_source: &'a dyn WindowSource,
    pub memory_path: PathBuf,
    pub denylist: DenyList,
    pub roster: &'a [(String, String)],
}

/// Dispatch one tool call.
///
/// Returns the handler's result as JSON or a structured error.
pub fn dispatch(
    tool_name: &str,
    arguments: Value,
    context: &DispatchContext,
) -> Result<ToolResult, DispatchError> {
    match tool_name {
        "speak" => {
            #[derive(Deserialize)]
            struct Args {
                message: String,
            }
            let args: Args = serde_json::from_value(arguments).map_err(|e| DispatchError {
                code: ErrorCode::InvalidArguments,
                message: format!("Invalid arguments for speak: {}", e),
            })?;
            let result = tools::speak(&args.message);
            Ok(ToolResult::Speak(result))
        }
        "play_behavior" => {
            #[derive(Deserialize)]
            struct Args {
                behavior: String,
            }
            let args: Args = serde_json::from_value(arguments).map_err(|e| DispatchError {
                code: ErrorCode::InvalidArguments,
                message: format!("Invalid arguments for play_behavior: {}", e),
            })?;
            let result = tools::play_behavior(&args.behavior);
            Ok(ToolResult::PlayBehavior(result))
        }
        "list_windows" => {
            let result = tools::list_windows(context.window_source, &context.denylist);
            Ok(ToolResult::ListWindows(result))
        }
        "describe_screen" => {
            let result = tools::describe_screen(context.window_source, &context.denylist);
            Ok(ToolResult::DescribeScreen(result))
        }
        "recall" => {
            let memory = MemoryManifest::new(&context.memory_path);
            let result = tools::recall(&memory).map_err(|e| DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: format!("Failed to recall: {}", e),
            })?;
            Ok(ToolResult::Recall(result))
        }
        "remember" => {
            #[derive(Deserialize)]
            struct Args {
                heading: String,
                fact: String,
            }
            let args: Args = serde_json::from_value(arguments).map_err(|e| DispatchError {
                code: ErrorCode::InvalidArguments,
                message: format!("Invalid arguments for remember: {}", e),
            })?;
            let memory = MemoryManifest::new(&context.memory_path);
            let result =
                tools::remember(&memory, &args.heading, &args.fact).map_err(|e| DispatchError {
                    code: ErrorCode::ExecutionFailed,
                    message: format!("Failed to remember: {}", e),
                })?;
            Ok(ToolResult::Remember(result))
        }
        "list_instances" => {
            let result = tools::list_instances(context.roster);
            Ok(ToolResult::ListInstances(result))
        }
        _ => Err(DispatchError {
            code: ErrorCode::UnknownTool,
            message: format!("Unknown tool: {}", tool_name),
        }),
    }
}

/// List all available tools.
///
/// Returns exactly the seven tools from #15 / tools.rs, and none that post
/// input events.
pub fn list_tools() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "speak".to_string(),
            description: "Make the Character speak a line of dialogue.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message the Character should speak"
                    }
                },
                "required": ["message"]
            }),
        },
        ToolInfo {
            name: "play_behavior".to_string(),
            description: "Play a named Behavior.".to_string(),
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
            description: "List visible windows with bounds and owning application.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolInfo {
            name: "describe_screen".to_string(),
            description: "Describe what is on screen (v1: window metadata only).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolInfo {
            name: "recall".to_string(),
            description: "Recall everything Memory holds.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolInfo {
            name: "remember".to_string(),
            description: "Remember one fact under a heading.".to_string(),
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
            description: "List Character Instances and their names.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
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

    // Seam 1: dispatch returns documented success JSON for each tool

    #[test]
    fn dispatch_speak_returns_speak_result() {
        let temp = TempDir::new("speak");
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({"message": "Hello, world"});
        let result = dispatch("speak", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::Speak(r) => {
                assert!(r.success);
                assert_eq!(r.message, "Hello, world");
            }
            _ => panic!("expected SpeakResult"),
        }
    }

    #[test]
    fn dispatch_play_behavior_returns_play_behavior_result() {
        let temp = TempDir::new("play");
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({"behavior": "wave"});
        let result = dispatch("play_behavior", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::PlayBehavior(r) => {
                assert!(r.success);
                assert_eq!(r.behavior, "wave");
            }
            _ => panic!("expected PlayBehaviorResult"),
        }
    }

    #[test]
    fn dispatch_list_windows_returns_list_windows_result() {
        let temp = TempDir::new("list-windows");
        let source = FakeWindowSource {
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
                windows: vec![window("Terminal", 10.0, 20.0, 800.0, 600.0)],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({});
        let result = dispatch("list_windows", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::ListWindows(r) => {
                assert_eq!(r.windows.len(), 1);
                assert_eq!(r.windows[0].owner, "Terminal");
            }
            _ => panic!("expected ListWindowsResult"),
        }
    }

    #[test]
    fn dispatch_describe_screen_returns_describe_screen_result() {
        let temp = TempDir::new("describe");
        let source = FakeWindowSource {
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
                windows: vec![window("Safari", 30.0, 40.0, 1200.0, 800.0)],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({});
        let result = dispatch("describe_screen", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::DescribeScreen(r) => {
                assert!(r.description.contains("1 visible window"));
                assert!(r.description.contains("Safari"));
            }
            _ => panic!("expected DescribeScreenResult"),
        }
    }

    #[test]
    fn dispatch_recall_returns_recall_result() {
        let temp = TempDir::new("recall");
        let memory = MemoryManifest::new(temp.join("memory.md"));
        memory
            .remember("Facts", "The user likes coffee")
            .expect("remembering writes");

        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({});
        let result = dispatch("recall", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::Recall(r) => {
                assert!(r.content.contains("The user likes coffee"));
            }
            _ => panic!("expected RecallResult"),
        }
    }

    #[test]
    fn dispatch_remember_returns_remember_result() {
        let temp = TempDir::new("remember");
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({"heading": "Facts", "fact": "The user's name is Oded"});
        let result = dispatch("remember", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::Remember(r) => {
                assert_eq!(r.recorded, "- The user's name is Oded");
            }
            _ => panic!("expected RememberResult"),
        }
    }

    #[test]
    fn dispatch_list_instances_returns_list_instances_result() {
        let temp = TempDir::new("list-instances");
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let roster = [
            ("inst-1".to_string(), "Clippy".to_string()),
            ("inst-2".to_string(), "Ferris".to_string()),
        ];
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &roster,
        };

        let args = json!({});
        let result = dispatch("list_instances", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::ListInstances(r) => {
                assert_eq!(r.instances.len(), 2);
                assert_eq!(r.instances[0].id, "inst-1");
                assert_eq!(r.instances[0].name, "Clippy");
            }
            _ => panic!("expected ListInstancesResult"),
        }
    }

    #[test]
    fn dispatch_unknown_tool_returns_error() {
        let temp = TempDir::new("unknown");
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

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
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({"wrong_field": 42});
        let result = dispatch("speak", args, &context);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidArguments);
    }

    // Seam 2: list_tools names exactly the seven tools, no input event tools

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

    // Seam 3: denylist is applied through dispatch

    #[test]
    fn dispatch_list_windows_applies_denylist() {
        let temp = TempDir::new("denylist-list");
        let source = FakeWindowSource {
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
                windows: vec![
                    window("Terminal", 10.0, 20.0, 800.0, 600.0),
                    window("1Password", 30.0, 40.0, 400.0, 300.0),
                ],
            },
        };
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist,
            roster: &[],
        };

        let args = json!({});
        let result = dispatch("list_windows", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::ListWindows(r) => {
                assert_eq!(r.windows.len(), 1);
                assert_eq!(r.windows[0].owner, "Terminal");
            }
            _ => panic!("expected ListWindowsResult"),
        }
    }

    #[test]
    fn dispatch_describe_screen_applies_denylist() {
        let temp = TempDir::new("denylist-describe");
        let source = FakeWindowSource {
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
                windows: vec![
                    window("Terminal", 10.0, 20.0, 800.0, 600.0),
                    window("1Password", 30.0, 40.0, 400.0, 300.0),
                ],
            },
        };
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist,
            roster: &[],
        };

        let args = json!({});
        let result = dispatch("describe_screen", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::DescribeScreen(r) => {
                assert!(r.description.contains("Terminal"));
                assert!(!r.description.contains("1Password"));
            }
            _ => panic!("expected DescribeScreenResult"),
        }
    }

    // Seam 4: list_instances reflects the roster, including empty

    #[test]
    fn dispatch_list_instances_with_empty_roster_returns_empty_list() {
        let temp = TempDir::new("empty-roster");
        let source = FakeWindowSource {
            capabilities: Capabilities {
                window_geometry: true,
                absolute_positioning: true,
            },
            geometry: WorldGeometry {
                usable_frames: vec![],
                windows: vec![],
            },
        };
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };

        let args = json!({});
        let result = dispatch("list_instances", args, &context).expect("dispatch succeeds");

        match result {
            ToolResult::ListInstances(r) => {
                assert_eq!(r.instances.len(), 0);
            }
            _ => panic!("expected ListInstancesResult"),
        }
    }
}
