# 0016 — Harness attachment and action log

## Why

The Harness reasons and acts. ai-buddy owns the character. Desktop control belongs to the
Harness because it already has an executor; ai-buddy writing one is redundant work with a
large permission surface. See [ADR-0003](../adr/0003-no-executor-harness-owns-desktop-control.md).

## Scope

- Attach a Harness by user configuration.
- One first-party adapter ships, so the out-of-box path is not "install a harness first."
- Any MCP-capable harness can attach directly.
- An action log surfaces what the Harness did.

**ai-buddy adds no confirmation of its own for acting** and owns consent only for sensing.
Two dialogs for one click teaches users to click through both.

No provider abstraction layer.

## Acceptance criteria

- A configured Harness can drive the buddy through the MCP tools.
- With no Harness attached, everything else keeps working and the user is told how to
  connect one.
- The action log records what the Harness did and is reviewable after the fact.
- ai-buddy never shows a second confirmation over the Harness's own.

## Tests

Adapter tested against a fake Harness. Attachment flow is manual.

## Known constraint

Harness-native computer use is a research preview gated behind a Pro or Max subscription,
and is not portable across vendors. A Harness without an executor can still chat and
sense. This limits who can use the Functional Layer at all — it is a recorded risk, not a
defect.
