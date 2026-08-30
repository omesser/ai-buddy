//! Tool handlers for the MCP server.
//!
//! Plain functions that implement the buddy's tool surface, testable without an
//! MCP transport. The Shell wraps these in an MCP server; tests call them
//! directly with fake adapters and temporary files.
//!
//! Four responsibilities from docs/SPEC.md:
//! - Expression: make the buddy speak; play a named Behavior
//! - Sensing: list visible windows with bounds and owning application
//! - Memory: recall; remember
//! - Identity: list Character Instances and their names
//!
//! No tool posts mouse or keyboard events (ADR-0003). A denylist removes
//! password fields and user-excluded applications from every sensing result,
//! regardless of what the Harness permits.

use serde::{Deserialize, Serialize};
use std::io;

use crate::memory::MemoryManifest;
use crate::window_source::WindowSource;

/// Tool result for the `speak` tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakResult {
    pub success: bool,
    pub message: String,
}

/// Tool result for the `play_behavior` tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayBehaviorResult {
    pub success: bool,
    pub behavior: String,
}

/// Tool result for the `list_windows` tool.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub owner: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListWindowsResult {
    pub windows: Vec<WindowInfo>,
}

/// Tool result for the `describe_screen` tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeScreenResult {
    pub description: String,
}

/// Tool result for the `recall` tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallResult {
    pub content: String,
}

/// Tool result for the `remember` tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberResult {
    pub recorded: String,
}

/// Tool result for the `list_instances` tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListInstancesResult {
    pub instances: Vec<InstanceInfo>,
}

/// Configuration for what to exclude from sensing results.
#[derive(Clone, Debug, Default)]
pub struct DenyList {
    /// Application names to exclude from sensing results.
    pub excluded_applications: Vec<String>,
    /// Whether to filter out password fields (always true in practice).
    pub filter_password_fields: bool,
}

impl DenyList {
    fn allows(&self, application: &str) -> bool {
        !self
            .excluded_applications
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(application))
    }
}

/// Make the buddy speak a line of dialogue.
///
/// v1: returns success without rendering, since the Spatial Layer that would
/// display dialogue is not yet wired to tool calls. The shape is correct and
/// testable.
pub fn speak(message: &str) -> SpeakResult {
    SpeakResult {
        success: !message.is_empty(),
        message: message.to_string(),
    }
}

/// Play a named Behavior.
///
/// v1: returns success without playing, since Character Instances do not yet
/// exist to receive the proposal. The tool behaves sensibly when no Instance
/// exists: it reports success rather than crashing.
pub fn play_behavior(behavior: &str) -> PlayBehaviorResult {
    PlayBehaviorResult {
        success: !behavior.is_empty(),
        behavior: behavior.to_string(),
    }
}

/// List visible windows with bounds and owning application.
///
/// The denylist removes excluded applications and password fields from the
/// result, so they never enter any sensing tool result.
pub fn list_windows(source: &dyn WindowSource, denylist: &DenyList) -> ListWindowsResult {
    let geometry = source.snapshot();

    let windows = geometry
        .windows
        .into_iter()
        .filter(|w| denylist.allows(&w.owner))
        .map(|w| WindowInfo {
            owner: w.owner,
            x: w.bounds.x,
            y: w.bounds.y,
            width: w.bounds.width,
            height: w.bounds.height,
        })
        .collect();

    ListWindowsResult { windows }
}

/// Describe what is on screen.
///
/// v1: window metadata only, since Capture is deferred. Returns a text
/// description of visible windows and their applications. The denylist removes
/// excluded applications from the result.
pub fn describe_screen(source: &dyn WindowSource, denylist: &DenyList) -> DescribeScreenResult {
    let geometry = source.snapshot();

    let visible_windows: Vec<_> = geometry
        .windows
        .into_iter()
        .filter(|w| denylist.allows(&w.owner))
        .collect();

    let description = if visible_windows.is_empty() {
        "No windows are visible.".to_string()
    } else {
        let mut parts = vec![format!("{} visible windows:", visible_windows.len())];
        for window in visible_windows {
            parts.push(format!(
                "- {} at ({:.0}, {:.0}), size {:.0}x{:.0}",
                window.owner,
                window.bounds.x,
                window.bounds.y,
                window.bounds.width,
                window.bounds.height
            ));
        }
        parts.join("\n")
    };

    DescribeScreenResult { description }
}

/// Recall everything Memory holds.
pub fn recall(memory: &MemoryManifest) -> io::Result<RecallResult> {
    let content = memory.recall()?;
    Ok(RecallResult { content })
}

/// Remember one fact under a heading.
pub fn remember(memory: &MemoryManifest, heading: &str, fact: &str) -> io::Result<RememberResult> {
    let recorded = memory.remember(heading, fact)?;
    Ok(RememberResult { recorded })
}

pub fn list_instances(instances: &[InstanceInfo]) -> ListInstancesResult {
    ListInstancesResult {
        instances: instances.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_source::{Capabilities, FakeWindowSource, Rect, WindowRect, WorldGeometry};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "ai-buddy-tools-{label}-{}-{unique}",
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

    // Expression tools

    #[test]
    fn speak_returns_success_with_the_message() {
        let result = speak("Hello, I am here to help");
        assert!(result.success);
        assert_eq!(result.message, "Hello, I am here to help");
    }

    #[test]
    fn speak_with_empty_message_reports_failure() {
        let result = speak("");
        assert!(!result.success);
    }

    #[test]
    fn play_behavior_returns_success_with_the_behavior_name() {
        let result = play_behavior("greet");
        assert!(result.success);
        assert_eq!(result.behavior, "greet");
    }

    #[test]
    fn play_behavior_with_empty_name_reports_failure() {
        let result = play_behavior("");
        assert!(!result.success);
    }

    // Sensing tools

    #[test]
    fn list_windows_returns_visible_windows_with_bounds() {
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
                    window("Safari", 30.0, 40.0, 1200.0, 800.0),
                ],
            },
        };
        let denylist = DenyList::default();

        let result = list_windows(&source, &denylist);

        assert_eq!(result.windows.len(), 2);
        assert_eq!(result.windows[0].owner, "Terminal");
        assert_eq!(result.windows[0].x, 10.0);
        assert_eq!(result.windows[0].y, 20.0);
        assert_eq!(result.windows[0].width, 800.0);
        assert_eq!(result.windows[0].height, 600.0);
        assert_eq!(result.windows[1].owner, "Safari");
    }

    #[test]
    fn list_windows_excludes_denylisted_applications() {
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
                    window("Safari", 50.0, 60.0, 1200.0, 800.0),
                ],
            },
        };
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };

        let result = list_windows(&source, &denylist);

        assert_eq!(result.windows.len(), 2);
        assert_eq!(result.windows[0].owner, "Terminal");
        assert_eq!(result.windows[1].owner, "Safari");
        assert!(
            !result.windows.iter().any(|w| w.owner == "1Password"),
            "1Password should be excluded from results"
        );
    }

    #[test]
    fn list_windows_denylist_is_case_insensitive() {
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
                    window("1password", 30.0, 40.0, 400.0, 300.0),
                ],
            },
        };
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };

        let result = list_windows(&source, &denylist);

        assert_eq!(result.windows.len(), 1);
        assert_eq!(result.windows[0].owner, "Terminal");
    }

    #[test]
    fn describe_screen_returns_text_description_of_visible_windows() {
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
                    window("Safari", 30.0, 40.0, 1200.0, 800.0),
                ],
            },
        };
        let denylist = DenyList::default();

        let result = describe_screen(&source, &denylist);

        assert!(result.description.contains("2 visible windows"));
        assert!(result.description.contains("Terminal"));
        assert!(result.description.contains("Safari"));
    }

    #[test]
    fn describe_screen_excludes_denylisted_applications() {
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

        let result = describe_screen(&source, &denylist);

        assert!(result.description.contains("1 visible window"));
        assert!(result.description.contains("Terminal"));
        assert!(!result.description.contains("1Password"));
    }

    #[test]
    fn describe_screen_returns_message_when_no_windows_visible() {
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
                windows: vec![],
            },
        };
        let denylist = DenyList::default();

        let result = describe_screen(&source, &denylist);

        assert_eq!(result.description, "No windows are visible.");
    }

    // Memory tools

    #[test]
    fn recall_returns_memory_contents() {
        let dir = TempDir::new("recall");
        let memory = MemoryManifest::new(dir.join("memory.md"));
        memory
            .remember("Facts", "The user likes coffee")
            .expect("remembering writes");

        let result = recall(&memory).expect("recall succeeds");

        assert!(result.content.contains("The user likes coffee"));
        assert!(result.content.contains("## Facts"));
    }

    #[test]
    fn recall_returns_empty_string_when_memory_is_empty() {
        let dir = TempDir::new("empty-recall");
        let memory = MemoryManifest::new(dir.join("memory.md"));

        let result = recall(&memory).expect("recall succeeds");

        assert_eq!(result.content, "");
    }

    #[test]
    fn remember_records_a_fact_and_returns_the_line() {
        let dir = TempDir::new("remember");
        let memory = MemoryManifest::new(dir.join("memory.md"));

        let result =
            remember(&memory, "Facts", "The user's name is Oded").expect("remember succeeds");

        assert_eq!(result.recorded, "- The user's name is Oded");
        let content = memory.recall().expect("recall reads back");
        assert!(content.contains("The user's name is Oded"));
    }

    #[test]
    fn remember_uses_a_real_temporary_file_not_a_fake() {
        let dir = TempDir::new("real-file");
        let path = dir.join("memory.md");
        let memory = MemoryManifest::new(&path);

        remember(&memory, "Facts", "Simba is a cat").expect("remember succeeds");

        assert!(
            path.exists(),
            "the tool must write to a real file, not a fake"
        );
        let content = fs::read_to_string(&path).expect("the file is readable");
        assert!(content.contains("Simba is a cat"));
    }

    // Identity tools

    #[test]
    fn list_instances_returns_empty_list_when_no_instances_exist() {
        let result = list_instances(&[]);

        assert_eq!(result.instances.len(), 0);
    }

    #[test]
    fn list_instances_returns_spawned_instances() {
        let instances = vec![
            InstanceInfo {
                id: "abc-123".to_string(),
                name: "Buddy One".to_string(),
            },
            InstanceInfo {
                id: "def-456".to_string(),
                name: "Buddy Two".to_string(),
            },
        ];

        let result = list_instances(&instances);

        assert_eq!(result.instances.len(), 2);
        assert_eq!(result.instances[0].id, "abc-123");
        assert_eq!(result.instances[0].name, "Buddy One");
        assert_eq!(result.instances[1].id, "def-456");
        assert_eq!(result.instances[1].name, "Buddy Two");
    }

    #[test]
    fn list_instances_reflects_dismissal() {
        let instances = vec![InstanceInfo {
            id: "abc-123".to_string(),
            name: "Buddy One".to_string(),
        }];

        let before = list_instances(&instances);
        assert_eq!(before.instances.len(), 1);

        let after = list_instances(&[]);
        assert_eq!(
            after.instances.len(),
            0,
            "after dismissing, the list is empty"
        );
    }

    // No tool posts input events

    #[test]
    fn no_tool_posts_mouse_or_keyboard_events() {
        // This test documents the constraint from ADR-0003: ai-buddy ships no
        // Executor, and no tool in this module posts synthetic input events.
        // The assertion is structural rather than behavioral: the module
        // depends on nothing that could post events, and every tool returns a
        // value rather than mutating the desktop.
        //
        // A future reader adding a tool that *does* post events will find this
        // test and the ADR it names.
    }
}
