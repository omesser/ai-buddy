//! MCP server stub.
//!
//! Placeholder binary for the MCP server. The actual rmcp integration is
//! deferred: the in-process dispatch in core is the testable seam, and this
//! binary will wrap it when rmcp API questions are resolved.
//!
//! For now, this compiles and demonstrates the architecture: a thin bin crate
//! depending only on ai-buddy-core, no tauri, no GTK, passes Linux CI.

fn main() {
    eprintln!("ai-buddy-mcp: MCP server placeholder");
    eprintln!("Dispatch is testable in core; rmcp wrapper deferred to #16");
    std::process::exit(0);
}
