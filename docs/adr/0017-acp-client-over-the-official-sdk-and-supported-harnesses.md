# The ACP client is the official SDK on one thread, and three Harnesses have a launch row

ai-buddy is an Agent Client Protocol client built on `agent-client-protocol`
2.x, the SDK Zed ships and the protocol's own repository maintains. It lives
in `src-tauri/src/acp_wire.rs` and nowhere else: one thread per spawned
Harness runs a current-thread tokio runtime that drives the SDK's connection
future, registers the two callbacks a client needs (`session/update`
notifications and `session/request_permission` requests), and hands the rest
of the shell plain values over a channel. `fs/*` and `terminal/*` are never
advertised, and the SDK answers them with method-not-found on its own. No
SDK type crosses out of that file, so reversing the crate choice rewrites it
and nothing beside it.

`src-tauri/src/harness.rs` owns the policy around the wire: the launch table,
when to spawn and respawn and how long to back off, the `Completer`
implementation, the authentication gate, the session file, and what reaches
the Action Log and the Chat surface. `Completer::complete` stays a blocking
call on a `Slots` worker (ADR-0016) that waits on the wire with the same
timeout → `session/cancel` semantics the HTTP Completer has. The frame loop
never sees a future (ADR-0004).

`AI_BUDDY_HARNESS` picks the Harness: `claude`, `hermes`, `opencode`, or a
command line of the user's own. Unset leaves the HTTP Completer in `model.rs`
exactly as it was. The Harness is the Completer for every Instance, so
ADR-0008's one session holds across buddies as well as across wakes.

One session per app lifetime, and across restarts when the Harness allows
it: `{session_id, harness, agent}` goes to `harness-session.json` in the data
folder, and an `initialize` that reports `loadSession` gets a `session/load`
with that id before any `session/new`. Any failure there is a fresh session
and a rewritten file. `session/new` and `session/load` go out as raw requests
rather than through the SDK's session builders, which tear the connection
down when the Harness refuses — and `auth_required` is a refusal we recover
from. Where "the turn finished" is read is one function, `acp_wire::turn`,
because ACP v2 moves it.

## Considered Options

- **`acp-cli` as a library.** ADR-0010 named it for its `AcpBridge`. It is a
  pre-1.0 CLI packaged as a library, with clap and indicatif as hard
  dependencies, and it pins `agent-client-protocol ^0.10` while the current
  line is 2.x. The product policy it would have absorbed — when to spawn,
  what a stop reason means, who answers a permission request — is the part
  this repository has to own anyway. Rejected.
- **Hand-rolled newline JSON-RPC over `std::process::Child`.** What the first
  revision of this pull request shipped: no protocol crate, no executor, one
  reader thread, `serde_json::Value` for seven messages. It worked against a
  fake and against `hermes acp`'s handshake, and it would have been ours to
  keep in step with a protocol that is still adding messages. Replaced by the
  SDK before merge.
- **`agent-client-protocol-schema` alone, for the types.** Moot: the full
  crate carries it, and its builders are the reason `serde_json::Value` was
  tempting. Skipped as a temporary measure, not as a strategy.
- **Any of the above to keep tokio out.** Not a reason. ureq is a choice the
  HTTP Completer made for itself; tokio is already in the tree through Tauri,
  and here it is confined to the wire thread with the `rt`, `sync` and
  `macros` features and no runtime anywhere else.

The SDK declares `rust-version = 1.88`, as does the schema crate it pins. The
workspace floor moves from 1.77 to 1.88 with it; the old floor was set when
the core crate was split out (#33) and nothing since has leaned on it. CI
runs stable. Two lints that the floor unlocks (`as_chunks`, `is_multiple_of`)
are taken as clippy asks.

## Harnesses

The launch table is ours: three named rows and an escape hatch, not a
seventeen-agent list inherited from a CLI.

| Name | Command | Standing |
|---|---|---|
| `claude` | `npx -y @agentclientprotocol/claude-agent-acp` | Zed's adapter over the Claude Agent SDK; no first-party ACP mode. **Verified 2026-09-07**: `end_turn` on a fresh session and again on a resumed one. |
| `hermes` | `hermes acp` | First-party. **Verified 2026-09-07** on a fresh session; a resumed one is #448, and it is ours. |
| `opencode` | `opencode acp` | First-party. **Unverified**: not installed on the machine either #433 or #434 was written on. |
| anything else | as typed, split on whitespace | The escape hatch for every row below, and the next adapter. |

`scripts/probe-harness.sh` is what "verified" means: it attaches the
configured Harness with no overlay, runs one fixed prompt, and prints the
handshake, the stop reason, the reply and whether the reply parsed as a
proposal. A row moves in this table when that command exits zero, and the
handshake it printed is what the next paragraph records.

What the two installed Harnesses advertise in `initialize` differs enough to
matter. `hermes` 0.18.2 offers `loadSession` and two `authMethods` — `custom
runtime credentials` and `Configure Hermes provider`, the second a `terminal`
method — and no `mcpCapabilities.http`, so #166 has to keep the stdio path for
it. Claude Code's adapter offers `loadSession` and `mcpCapabilities.http` and,
signed in, an empty `authMethods`: the list is what is *available*, not what is
outstanding, so it is no test of whether a login is needed. Only `session/new`
answering `-32000` is that, which is why the gate hangs off the error and not
the handshake. `hermes` also advertises `sessionCapabilities` (`fork`, `list`,
`resume`) and `promptCapabilities.image`, neither of which anything here reads
yet.

The one thing a real turn contradicted is the fallback above: `session/load`
refusing is not always an error. `hermes` answers a session it cannot restore
with a success result and logs the reason to its stderr, which leaves the id
cached and every later turn a `refusal`. #448 has the fix, and the fake agent's
error-shaped refusal is why the tests missed it.

Protocol-compatible and not yet named, all first-party ACP on stdio and all
reachable today through the custom value: Grok Build (`grok agent stdio`),
GitHub Copilot CLI (`copilot --acp --stdio`, public preview) and Gemini CLI
(`gemini --acp`). They earn a named row once a turn has been smoked. None of
their always-approve, yolo, or `setSessionMode` auto-approve flags is ever
passed by default — the Chat surface owns permissions. A vendor extension
method we do not know (`x.ai/*` and the like) gets method-not-found, which
the protocol allows. Gemini has open issues about stdout pollution corrupting
its NDJSON; smoke it carefully.

Pi stays deferred: it has no first-party ACP and the research says it wants
a second adapter, not a compromise.

## Consequences

The Harness authenticates itself (ADR-0010's eight rules). The child gets our
environment untouched — no provider key, no `CLAUDE_CONFIG_DIR`, no `--bare`
— and a test asserts it. `auth_required` on `session/new` becomes a command
the Chat surface names for the user's own terminal (`claude /login`, or the
Harness's own `authMethods` description), retried no more than once a minute
so a login mid-session is picked up without a restart. A new child resets
that gate.

The child is spawned by us, not by the SDK's `AcpAgent`, because that helper
has no working-directory knob and ADR-0010 puts the Harness in the data
folder. The SDK reads the pipes we hand it; we kill the child when the wire
thread ends, on every exit path.

A permission request is forwarded to every open Chat surface and answered
only by a click there. A turn that times out first sends the protocol's
`cancelled` outcome, which is a withdrawal, not an answer.

`AI_BUDDY_HARNESS` is env-only: no settings-window row, because the row would
cost more than the variable it names until a second Harness setting joins it.

The Action Log's `prompt` event carries the session id and the prompt's
length, not which Instance woke or whether the wake was reactive: the
`Completer` seam hands over the prompt text and nothing else. Whether the
reply parsed as a proposal is not logged anywhere — `crates/core` parses it
and does no I/O, and the shell's near-miss line is a trace, not a record.

There is no loopback HTTP MCP server yet (#166). The session gets the
`ai-buddy-mcp` binary over stdio when it can be found beside the app or at
`AI_BUDDY_MCP_BIN`, and nothing otherwise; `mcpCapabilities.http` is recorded
so #166 can branch on it.
