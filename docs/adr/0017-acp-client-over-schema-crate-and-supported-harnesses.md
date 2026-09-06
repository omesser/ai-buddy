# The ACP client is hand-rolled over stdio, and three Harnesses have a launch row

`src-tauri/src/harness.rs` speaks the Agent Client Protocol v1 to a Harness
it spawns: newline-delimited JSON-RPC 2.0 on the child's stdin and stdout, a
reader thread, and `serde_json::Value` for the eight messages we use
(`initialize`, `session/new`, `session/load`, `session/prompt`,
`session/cancel`, `session/update`, `session/request_permission`, and a
method-not-found reply for `fs/*` and `terminal/*`). No protocol crate.

`AI_BUDDY_HARNESS` picks the Harness: `claude`, `hermes`, `opencode`, or a
command line of the user's own. Unset leaves the HTTP Completer in `model.rs`
exactly as it was. The Harness is the Completer for every Instance, so
ADR-0008's one session holds across buddies as well as across wakes.

One session per app lifetime, and across restarts when the Harness allows
it: `{session_id, harness, agent}` goes to `harness-session.json` in the data
folder, and an `initialize` that reports `loadSession` gets a `session/load`
with that id before any `session/new`. Any failure there is a fresh session
and a rewritten file. Where "the turn finished" is read is one function,
`turn_finished`, because ACP v2 moves it.

## Considered Options

- **`acp-cli` as a library.** ADR-0010 named it for its `AcpBridge` and its
  launch table for seventeen agents. It is 0.3.1, pins `agent-client-protocol
  ^0.10` against a current 2.x, and brings clap, indicatif and tokio into a
  shell that chose ureq to stay off tokio. Rejected.
- **`agent-client-protocol` 2.x, implementing `Client`.** The reference
  crate, and an async runtime with it (`async-io`, `async-process`). Every
  blocking call in this shell is already a worker thread the frame loop
  polls (ADR-0004, ADR-0016); a second scheduler for one pipe buys nothing.
  Rejected.
- **`agent-client-protocol-schema` 1.7 for the types only.** The brief's
  first choice. Its `rust-version` is 1.88 against the workspace floor of
  1.77, and its request types are `#[non_exhaustive]` builders, which cost
  more lines to fill than the ten fields we read out of a `Value`. Rejected
  for now; the fake-agent tests pin the wire shape, so swapping the types in
  later changes no behaviour.

## Harnesses

| Name | Command | Standing |
|---|---|---|
| `claude` | `npx -y @agentclientprotocol/claude-agent-acp` | Zed's adapter over the Claude Agent SDK; no first-party ACP mode. Tested against the fake only. |
| `hermes` | `hermes acp` | First-party. Tested against the fake only. |
| `opencode` | `opencode acp` | First-party. **Untested**: not installed where this was written. |
| anything else | as typed, split on whitespace | The escape hatch for the next adapter. |

Deferred, and named so nobody reads their absence as a decision: Pi has no
first-party ACP and the research says it wants a second adapter, not a
compromise; Grok Build has no ACP documentation page; Codex, Gemini and
Copilot have Zed adapters but are not among ADR-0010's five.

## Consequences

The Harness authenticates itself (ADR-0010's eight rules). The child gets our
environment untouched — no provider key, no `CLAUDE_CONFIG_DIR`, no `--bare`
— and a test asserts it. `auth_required` on `session/new` becomes a command
the Chat surface names for the user's own terminal (`claude /login`, or the
Harness's own `authMethods` description), retried no more than once a minute
so a login mid-session is picked up without a restart.

A permission request is forwarded to every open Chat surface and answered
only by a click there. A turn that times out first sends the protocol's
`cancelled` outcome, which is a withdrawal, not an answer.

There is no loopback HTTP MCP server yet (#166). The session gets the
`ai-buddy-mcp` binary over stdio when it can be found beside the app or at
`AI_BUDDY_MCP_BIN`, and nothing otherwise; `mcpCapabilities.http` is recorded
so #166 can branch on it.

Reversing the crate decision is a contained change: the wire shape is fixed
by the protocol and by the tests, and only `harness.rs` reads JSON.
