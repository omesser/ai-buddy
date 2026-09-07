# The Harness runs headless and ai-buddy draws the chat surface

## Context

Desktop agent harnesses ship with their own TUIs: Claude Code, Hermes, opencode,
Grok Build, and Pi all draw their own interfaces. We need to decide whether to
use their UI or build our own, and whether to run the harness in the foreground
or spawn it headless.

Most harnesses support Agent Client Protocol (ACP), where the agent runs as a
subprocess speaking JSON-RPC on stdio and renders nothing. opencode additionally
supports attach mode (`opencode serve` + `opencode attach --session`), letting
two clients share one session store. Other harnesses lack this: Claude Code,
Hermes, Pi, and Grok Build have no equivalent.

A mascot that opens a terminal window when double-clicked undermines the
presence the spatial layer establishes.

## Decision

The attached Harness runs headless via ACP (`opencode acp`, `hermes acp`, etc.)
and ai-buddy draws the chat surface in its own Tauri webview window. We never
launch, embed, or wrap the Harness's TUI.

ACP mode and interactive mode are mutually exclusive in one process. Running
both costs a second process, which is a second conversation — the split brain
ADR-0008 forbids. A capability four of five Harnesses lack cannot be the
product shape.

## Consequences

We own the chat surface and the permission prompt. Forwarding the Harness's
`session/request_permission` is not a second confirmation — ADR-0003's rule is
that ai-buddy never *answers* it, and never adds one of its own.

The Harness authenticates itself and ai-buddy holds no credential for it.
Harnesses keep their credential in a home-relative file or the OS keychain.
A CLI the user has already logged in to hands its login to a subprocess we
spawn as the same user.

Attachment is opt-in. ai-buddy spawns and holds a full agent process for as
long as the app runs. Static weights and the HTTP Completer stay the path for
everyone who declines.

Reversing this means writing a terminal host and giving up the turn-completion
signal, or keeping ACP and accepting two conversations.

## Supersedes

This decision supersedes [ADR-0010](./0010-headless-harness-our-chat-surface.md),
which recorded the same choice alongside implementation detail that belongs
elsewhere.
