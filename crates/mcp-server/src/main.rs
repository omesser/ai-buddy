//! MCP server exposing ai-buddy tool dispatch over stdio.
//!
//! A Harness can spawn this binary and call the seven tools from #15.
//! v1 uses injected stubs: StubWindowSource (no real window sensing), empty
//! roster, denylist from settings.json beside Memory. A Harness attach in
//! #16 will provide real dependencies per instance.

use ai_buddy_core::dispatch::{dispatch, DenyList, DispatchContext};
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
            dock: None,
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

    fn make_context(&self) -> DispatchContext<'static> {
        static SOURCE: StubWindowSource = StubWindowSource;
        let memory_path = ai_buddy_core::memory::shared_path();
        if let Some(dir) = memory_path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        DispatchContext {
            window_source: &SOURCE,
            memory_path,
            denylist: DenyList::from_settings_file(
                &ai_buddy_core::memory::data_dir().join("settings.json"),
            ),
            roster: &[],
            expression: None,
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct SpeakArgs {
    message: String,
    #[serde(default)]
    instance_id: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct PlayBehaviorArgs {
    behavior: String,
    #[serde(default)]
    instance_id: Option<String>,
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
        let mut context = self.make_context();
        let args_json = serde_json::to_value(&args).map_err(|e| e.to_string())?;
        let result = dispatch("speak", args_json, &mut context).map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(description = "Play a named Behavior")]
    async fn play_behavior(
        &self,
        Parameters(args): Parameters<PlayBehaviorArgs>,
    ) -> Result<String, String> {
        let mut context = self.make_context();
        let args_json = serde_json::to_value(&args).map_err(|e| e.to_string())?;
        let result = dispatch("play_behavior", args_json, &mut context).map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(description = "List visible windows with bounds and owning application")]
    async fn list_windows(&self) -> Result<String, String> {
        let mut context = self.make_context();
        let result =
            dispatch("list_windows", serde_json::json!({}), &mut context).map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(description = "Describe what is on screen (v1: window metadata only)")]
    async fn describe_screen(&self) -> Result<String, String> {
        let mut context = self.make_context();
        let result = dispatch("describe_screen", serde_json::json!({}), &mut context)
            .map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(description = "Recall everything Memory holds")]
    async fn recall(&self) -> Result<String, String> {
        let mut context = self.make_context();
        let result =
            dispatch("recall", serde_json::json!({}), &mut context).map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(description = "Remember one fact under a heading")]
    async fn remember(&self, Parameters(args): Parameters<RememberArgs>) -> Result<String, String> {
        let mut context = self.make_context();
        let args_json = serde_json::to_value(&args).map_err(|e| e.to_string())?;
        let result = dispatch("remember", args_json, &mut context).map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(description = "List Character Instances and their names")]
    async fn list_instances(&self) -> Result<String, String> {
        let mut context = self.make_context();
        let result = dispatch("list_instances", serde_json::json!({}), &mut context)
            .map_err(|e| e.message)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
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

    let running = match rmcp::service::serve_server(handler, transport).await {
        Ok(running) => running,
        Err(e) => {
            eprintln!("Server initialization error: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = running.waiting().await {
        eprintln!("Server runtime error: {}", e);
        std::process::exit(1);
    }
}
