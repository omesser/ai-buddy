# The ACP client uses the official SDK and supported Harnesses are named

## Context

Agent Client Protocol (ACP) is how ai-buddy talks to external agent harnesses.
We need to choose an ACP client implementation and decide which harnesses to
support by name versus requiring users to configure command lines themselves.

Three approaches exist:
1. The official `agent-client-protocol` 2.x SDK maintained by the protocol's
   own repository
2. The `acp-cli` library (pre-1.0, pins older protocol versions, carries CLI
   dependencies)
3. Hand-rolled JSON-RPC (no protocol crate, manually tracking protocol changes)

For harness selection, we could enumerate seventeen agents inherited from CLI
tools, or maintain a focused list of verified harnesses with an escape hatch.

## Decision

ai-buddy is an ACP client built on `agent-client-protocol` 2.x, the SDK Zed
ships and the protocol's own repository maintains.

The product layer owns the policy around the wire: launch table, spawn/respawn
logic and backoff, authentication gate, session file, and what reaches the
Action Log and Chat surface.

The wire is isolated so the frame loop never sees futures (ADR-0004).

The launch table names four harnesses (`claude`, `codex`, `hermes`, `opencode`)
and provides an escape hatch for any custom command. Harnesses earn verified
standing once a turn has been smoked against them; `codex` is unverified until
smoked.

One session per app lifetime, persisted across restarts when the Harness
supports `loadSession`.

## Consequences

The Harness is the Completer for every Instance, so ADR-0008's one session
holds across buddies and across wakes.

A permission request is forwarded to every open Chat surface and answered only
by a click there. A turn that times out sends `cancelled`, which is a
withdrawal, not an answer.

The client hands the session its own tool endpoint, and which endpoint that is
belongs to [ADR-0023](./0023-app-dispatches-its-own-tools.md): a Harness that
can use the app's own server reaches the live Instances, and one that cannot is
answered by a stub.

Protocol-compatible harnesses not yet named (Grok Build, GitHub Copilot CLI,
Gemini CLI) are reachable through the custom command and earn a named row once
verified. Their always-approve or auto-approve flags are never passed by
default — the Chat surface owns permissions.

## Supersedes

This decision supersedes [ADR-0017](./0017-acp-client-over-the-official-sdk-and-supported-harnesses.md),
which recorded the same choice alongside implementation detail that belongs
elsewhere.
