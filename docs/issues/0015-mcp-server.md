# 0015 — MCP server and tool surface

## Why

The MCP server is how any Harness reaches the buddy. It is also, by construction, the BYO
story — no provider abstraction layer is needed because MCP is that layer.

## Scope

Tool surface by responsibility:

- **Expression** — make the buddy speak; play a named Behavior.
- **Sensing** — list visible windows with bounds and owning application; describe what is
  on screen (v1: window metadata only, since Capture is deferred).
- **Memory** — recall; remember.
- **Identity** — list Character Instances and their names.

**There is no tool that posts mouse or keyboard events.** ai-buddy ships no Executor. See
[ADR-0003](../adr/0003-no-executor-harness-owns-desktop-control.md).

A denylist is ai-buddy's regardless of what the Harness permits: password fields and
user-excluded applications never enter any sensing tool result.

## Acceptance criteria

- Each tool returns its documented success shape.
- Tools behave sensibly when no Character Instance exists.
- No tool posts input events.
- The denylist removes excluded applications and password fields from every sensing
  result.

## Tests

Tool-call level with a fake `WindowSource` and a temporary Memory file. Not over the MCP
transport.
