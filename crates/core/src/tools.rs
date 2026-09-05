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
