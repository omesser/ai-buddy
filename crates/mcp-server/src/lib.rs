//! The stdio fallback: ai-buddy's tool dispatch, out of process and stubbed.
//!
//! **This is not the path that reaches the buddy on screen.** ADR-0023 makes
//! the running app serve MCP itself on loopback HTTP, because tools have to be
//! dispatched where the `Roster` is — the Instance list a target resolves
//! against, and the `ExpressionHandle` a resolved one is enqueued on.
//! `src-tauri/src/mcp_http.rs` is that server, and it is what `session/new`
//! hands any Harness that advertises `mcpCapabilities.http`.
//!
//! This is what the rest get, reached three ways and always the same code:
//! `AI_BUDDY_MCP_BIN`, an `ai-buddy-mcp` sidecar beside the app, or the app
//! binary re-executed as `ai-buddy --mcp-stdio` (#497). A Harness with no HTTP
//! MCP — `hermes` is one (ADR-0017) — and story 66's power user pointing their
//! own Harness at ai-buddy both land here, and both get only what a process
//! outside the app can honestly do:
//!
//! - `recall` and `remember` are real. Memory is a file in the data folder and
//!   this process can read and write it.
//! - `speak` and `play_behavior` reach no Instance. There is no roster here, and
//!   `tools::speak` reports an empty roster as success — so a Harness on this
//!   path is told the line was said and nothing appears on screen. #470 is
//!   closed for the HTTP path only. #501 is the shim that would dial the
//!   running app and close it here too; #502 is the success this lies with.
//! - `list_windows` and `describe_screen` see nothing: `StubWindowSource` below
//!   is the only window source a process outside the app can have, since the
//!   platform sensing lives behind the Shell's own consent (ADR-0005).
//! - `list_instances` reports none, for the same reason as `speak`.

mod settings;

use ai_buddy_core::dispatch::{dispatch, DispatchContext};
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
            denylist: settings::load_denylist_from_settings(
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

/// Serve the tools on stdio until the client hangs up.
///
/// `#[tokio::main]` lives here so a child process started as `ai-buddy
/// --mcp-stdio` gets the mcp-server crate's multi-thread runtime, not the
/// shell's current-thread ACP one.
#[tokio::main]
pub async fn run() {
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
