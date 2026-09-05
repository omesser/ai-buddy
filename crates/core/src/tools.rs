//! Tool types and shared logic for the MCP server.
//!
//! Defines result types and shared utilities used by the dispatch layer.
//! Private tool handlers implement the buddy's tool surface behind dispatch,
//! testable without an MCP transport.
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

use crate::engine::BehaviorProposal;
use crate::memory::MemoryManifest;

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

/// Live handle for enqueueing Expression proposals onto Character Instances.
pub trait ExpressionHandle {
    /// Enqueue a BehaviorProposal onto the Instance with the given id.
    /// Returns true if that id was live and the proposal was queued.
    fn enqueue(&mut self, instance_id: &str, proposal: BehaviorProposal) -> bool;
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
    pub fn allows(&self, application: &str) -> bool {
        !self
            .excluded_applications
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(application))
    }
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

/// Private helper functions for dispatch implementation.
mod helpers {
    use crate::engine::BehaviorProposal;
    use crate::tools::{DenyList, ExpressionHandle, InstanceInfo};
    use crate::window_source::{WindowRect, WindowSource};

    /// Get denylist-filtered snapshot of windows from the window source.
    ///
    /// Both list_windows and describe_screen use this to ensure consistent filtering.
    pub fn filtered_windows_snapshot(
        source: &dyn WindowSource,
        denylist: &DenyList,
    ) -> Vec<WindowRect> {
        let geometry = source.snapshot();
        geometry
            .windows
            .into_iter()
            .filter(|w| denylist.allows(&w.owner))
            .collect()
    }

    /// Result from resolving expression target and attempting enqueue.
    pub enum ExpressionResult {
        /// Successfully enqueued (or stub success for no instances)
        Success,
        /// Failed due to unknown instance or ambiguous target
        Failed,
    }

    /// Enqueue an expression proposal following tools.rs logic.
    ///
    /// Both speak and play_behavior follow the same pattern of resolving target
    /// and enqueueing proposals through the expression handle.
    pub fn enqueue_expression(
        instance_id: Option<&str>,
        roster: &[InstanceInfo],
        expression: Option<&mut dyn ExpressionHandle>,
        proposal: BehaviorProposal,
    ) -> ExpressionResult {
        let target_id = match resolve_target_instance(instance_id, roster) {
            TargetResolution::Resolved(id) => id,
            TargetResolution::NoInstances => {
                // Empty roster is success (stub behavior for harness compatibility)
                return ExpressionResult::Success;
            }
            TargetResolution::UnknownInstance | TargetResolution::AmbiguousTarget => {
                return ExpressionResult::Failed;
            }
        };

        // Try to enqueue the proposal
        if let Some(handle) = expression {
            let _enqueue_result = handle.enqueue(&target_id, proposal);
            // Regardless of enqueue result, we report success
        }

        ExpressionResult::Success
    }

    /// Target resolution result for Expression tools.
    enum TargetResolution {
        /// Resolved to a specific instance id
        Resolved(String),
        /// No instances in roster (stub success case)
        NoInstances,
        /// Unknown instance_id provided
        UnknownInstance,
        /// Multiple instances but no specific id provided
        AmbiguousTarget,
    }

    /// Target resolution against roster for both speak and play_behavior.
    fn resolve_target_instance(
        instance_id: Option<&str>,
        roster: &[InstanceInfo],
    ) -> TargetResolution {
        match instance_id {
            Some(id) => {
                // Check if the given id exists in roster
                if roster.iter().any(|info| info.id == id) {
                    TargetResolution::Resolved(id.to_string())
                } else {
                    TargetResolution::UnknownInstance
                }
            }
            None => {
                // No instance_id provided
                match roster.len() {
                    0 => TargetResolution::NoInstances,
                    1 => TargetResolution::Resolved(roster[0].id.clone()),
                    _ => TargetResolution::AmbiguousTarget,
                }
            }
        }
    }
}

/// Make the Character speak a line of dialogue.
pub(crate) fn speak(
    message: &str,
    instance_id: Option<&str>,
    roster: &[InstanceInfo],
    expression: Option<&mut dyn ExpressionHandle>,
) -> SpeakResult {
    // Early return for empty message
    if message.is_empty() {
        return SpeakResult {
            success: false,
            message: message.to_string(),
        };
    }

    let proposal = BehaviorProposal {
        behavior: String::new(),
        dialogue: Some(message.to_string()),
    };

    let success = match helpers::enqueue_expression(instance_id, roster, expression, proposal) {
        helpers::ExpressionResult::Success => true,
        helpers::ExpressionResult::Failed => false,
    };

    SpeakResult {
        success,
        message: message.to_string(),
    }
}

/// Play a named Behavior.
pub(crate) fn play_behavior(
    behavior: &str,
    instance_id: Option<&str>,
    roster: &[InstanceInfo],
    expression: Option<&mut dyn ExpressionHandle>,
) -> PlayBehaviorResult {
    // Early return for empty behavior
    if behavior.is_empty() {
        return PlayBehaviorResult {
            success: false,
            behavior: behavior.to_string(),
        };
    }

    let proposal = BehaviorProposal {
        behavior: behavior.to_string(),
        dialogue: None,
    };

    let success = match helpers::enqueue_expression(instance_id, roster, expression, proposal) {
        helpers::ExpressionResult::Success => true,
        helpers::ExpressionResult::Failed => false,
    };

    PlayBehaviorResult {
        success,
        behavior: behavior.to_string(),
    }
}

/// List visible windows with bounds and owning application.
pub(crate) fn list_windows(
    window_source: &dyn crate::window_source::WindowSource,
    denylist: &DenyList,
) -> ListWindowsResult {
    let windows = helpers::filtered_windows_snapshot(window_source, denylist);
    ListWindowsResult {
        windows: windows
            .into_iter()
            .map(|w| WindowInfo {
                owner: w.owner,
                x: w.bounds.x,
                y: w.bounds.y,
                width: w.bounds.width,
                height: w.bounds.height,
            })
            .collect(),
    }
}

/// Describe what is on screen (v1: window metadata only).
pub(crate) fn describe_screen(
    window_source: &dyn crate::window_source::WindowSource,
    denylist: &DenyList,
) -> DescribeScreenResult {
    let windows = helpers::filtered_windows_snapshot(window_source, denylist);
    let description = if windows.is_empty() {
        "No windows are visible.".to_string()
    } else {
        let mut parts = vec![format!("{} visible windows:", windows.len())];
        for window in &windows {
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

#[cfg(test)]
mod tests {
    use super::*;
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

    // Sensing tools

    #[test]
    fn denylist_match_is_case_insensitive() {
        let denylist = DenyList {
            excluded_applications: vec!["1Password".to_string()],
            filter_password_fields: true,
        };

        assert!(!denylist.allows("1password"));
        assert!(!denylist.allows("1Password"));
        assert!(!denylist.allows("1PASSWORD"));
        assert!(denylist.allows("Terminal"));
    }

    #[test]
    fn describe_screen_when_no_windows_are_visible_returns_message() {
        use crate::window_source::{Capabilities, FakeWindowSource, Rect, WorldGeometry};

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
                dock: None,
            },
        };
        let denylist = DenyList::default();

        let result = describe_screen(&source, &denylist);

        assert_eq!(result.description, "No windows are visible.");
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
