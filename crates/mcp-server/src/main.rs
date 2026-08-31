//! MCP server exposing ai-buddy tool dispatch over stdio.
//!
//! A Harness can spawn this binary and call the seven tools from #15.
//! v1 uses injected stubs: no real WindowSource, empty roster, default denylist,
//! temp Memory.

use ai_buddy_core::dispatch::{dispatch, DispatchContext};
use ai_buddy_core::tools::DenyList;
use ai_buddy_core::window_source::{Capabilities, WindowSource, WorldGeometry};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

struct StubWindowSource;

impl WindowSource for StubWindowSource {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            window_geometry: false,
            absolute_positioning: false,
        }
    }

    fn read(&self) -> WorldGeometry {
        WorldGeometry {
            usable_frames: vec![],
            windows: vec![],
        }
    }
}

#[derive(Clone)]
struct AiBuddyServer {
    tool_router: ToolRouter<Self>,
}

impl AiBuddyServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct SpeakArgs {
    message: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct PlayBehaviorArgs {
    behavior: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct RememberArgs {
    heading: String,
    fact: String,
}

#[tool_router]
impl AiBuddyServer {
    #[tool(description = "Make the Character speak a line of dialogue")]
    async fn speak(&self, Parameters(args): Parameters<SpeakArgs>) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result = dispatch("speak", serde_json::to_value(&args).unwrap(), &context)
            .map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }

    #[tool(description = "Play a named Behavior")]
    async fn play_behavior(
        &self,
        Parameters(args): Parameters<PlayBehaviorArgs>,
    ) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result = dispatch(
            "play_behavior",
            serde_json::to_value(&args).unwrap(),
            &context,
        )
        .map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }

    #[tool(description = "List visible windows with bounds and owning application")]
    async fn list_windows(&self) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result = dispatch("list_windows", serde_json::json!({}), &context)
            .map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }

    #[tool(description = "Describe what is on screen (v1: window metadata only)")]
    async fn describe_screen(&self) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result = dispatch("describe_screen", serde_json::json!({}), &context)
            .map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }

    #[tool(description = "Recall everything Memory holds")]
    async fn recall(&self) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result =
            dispatch("recall", serde_json::json!({}), &context).map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }

    #[tool(description = "Remember one fact under a heading")]
    async fn remember(
        &self,
        Parameters(args): Parameters<RememberArgs>,
    ) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result = dispatch("remember", serde_json::to_value(&args).unwrap(), &context)
            .map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }

    #[tool(description = "List Character Instances and their names")]
    async fn list_instances(&self) -> Result<String, String> {
        let temp_dir = std::env::temp_dir().join("ai-buddy-mcp");
        std::fs::create_dir_all(&temp_dir).ok();
        let source = StubWindowSource;
        let context = DispatchContext {
            window_source: &source,
            memory_path: temp_dir.join("memory.md"),
            denylist: DenyList::default(),
            roster: &[],
        };
        let result = dispatch("list_instances", serde_json::json!({}), &context)
            .map_err(|e| e.message)?;
        Ok(serde_json::to_string(&result).unwrap())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AiBuddyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}

#[tokio::main]
async fn main() {
    let handler = AiBuddyServer::new();
    let transport = rmcp::transport::stdio();

    match rmcp::service::serve_server(handler, transport).await {
        Ok(_running) => {}
        Err(e) => {
            eprintln!("Server error: {}", e);
            std::process::exit(1);
        }
    }
}
