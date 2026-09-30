# Fidget declares its runtime dependencies and fails loudly when one is missing

## Context

Fidget is one binary, but several things it can use are not in that binary.
The Claude, Codex and Pi presets run Zed's adapters through `npx`, so they need
Node.js. Every other named preset runs a Harness CLI from `PATH`. Chat and
Settings need the platform WebView. On Linux, cue audio plays through GStreamer
and the AppImage needs FUSE. [The runtime dependency census](../research/runtime-dependencies.md)
(#735) lists each one.

Each of these could be shipped inside Fidget instead. That is not free. A Node
runtime with three adapter trees adds 50 to 100 MB or more, and its licenses and
release cadence become ours. A Harness is a separate product with its own
installer, updates and login, and the ACP registry lists dozens of them. The
bug that started this (#726) was not the missing dependency. It was that Fidget
said nothing: a missing `npx` left a dead chat.

## Decision

**Declare, and fail loudly, unless bundling is cheap.** A dependency the user
can install in one step stays outside Fidget. Fidget names it where the user
picks the thing that needs it, detects it at the moment it is used, and says
what is missing and where to get it.

**A missing required dependency is a sticky, copyable error.** It names the
missing command and its install page, in text the user can select and paste into
a search or another assistant. It stays up until a retry succeeds, the user
picks something else, or the user dismisses it. It never clears on a timer, and
it is never only a toast. Every surface that reports the state says the same
thing until it changes.

**A launcher that dies at startup shows what it printed.** Added 2026-09-30
(#1183). #1070 named the exit status and left the child's stderr in the
terminal, because piping it meant teeing it for the child's whole life on every
platform. The owner overrode that: a user who started Fidget from the Dock has
no terminal, and the dyld line is the diagnosis. The wire now pipes stderr and
copies every byte through to Fidget's own stderr, and keeps the last 4 KiB of
it, with the child's stdout up to `initialize`, for the landing's Error output
box. After `initialize`, stdout is the ACP stream and is not kept.

**An optional dependency degrades in silence.** When a missing piece only costs
flavor, Fidget carries on without it and shows nothing.

The V1 calls, taken by the owner on 2026-09-23:

| Dependency | Needed by | Call |
|---|---|---|
| Node.js and `npx` | The `claude`, `codex` and `pi` presets | Declared. Not bundled for V1; #928 is closed not planned. |
| A Harness CLI on `PATH` | The preset that names it | Declared per preset, with its login. |
| Platform WebView | Chat and Settings | Tauri's choice stands: system WebKit on macOS, the WebView2 bootstrapper on Windows, declared on Linux. |
| GStreamer | Linux cue audio | Optional. Silent skip. |
| FUSE | The Linux AppImage only | Declared. A future AppImage prefers type 2 with the runtime bundled. |
| X11 or Wayland | Linux overlay features | The user's environment, not a dependency. |
| The stdio MCP shim | A Harness that cannot take MCP over HTTP | Nothing extra ships. The app runs itself as the shim ([ADR-0026](./0026-stdio-mcp-is-a-shim-dialled-from-the-environment.md)). |

A new dependency is weighed on five questions. How large is it once bundled?
Whose licenses would Fidget carry? Whose release cadence would Fidget have to
follow? Do the people who want the feature already have it? Is it required for
what the user picked, or is it only flavor? A dependency that is small, freely
licensed, stable and absent from most machines is a bundling candidate. Anything
else is declared.

## Consequences

A user who picks `claude` without Node.js is told to install Node.js, and the
preset works after they do. Fidget stays one binary with no second release train
to follow.

The declare path is only as good as its failure. Each preset that needs a
command carries that command's install page, and a new preset is not done until
it does. A failure that is caught but reported only in a log breaks this
decision the same way #726 did.

Presets that need no Node.js are the cheaper path for a user who has none. The
Chat landing and Settings may steer toward them (#929), but that is presentation
and does not change this decision.

Reopening Node bundling in V1 means superseding this ADR. The likely trigger is
a first-party ACP binary for Claude or Codex, which would drop the `npx`
dependency without bundling anything. That is a V2 question.
