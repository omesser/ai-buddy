# One chat UI ships in v1 and the switcher waits for v2

## Context

A **Chat UI** is a named, swappable visual design for the chat surface: palette,
type scale, and shape. We need to decide whether v1 ships with multiple designs
and a switcher, or ships one design with the seam that makes switching possible
but no way to switch yet.

Three mockups exist (modern minimal, terminal log, pane of glass) and all three
run off one stylesheet with design-specific token blocks. The seam works: one
class swap repaints the surface.

## Decision

v1 ships one chat UI — modern minimal — and the seam that makes it swappable,
but no switcher. v2 adds the switcher over the three designs.

The seam is one class (`chat-ui-minimal`) on the root element and one token
file (`src/chat-ui.css`). No part of the chat surface names a color, font
family, or radius literal outside a `.chat-ui-*` block.

The switcher is an application setting (one chat UI for the whole app), not
per-Character or per-Instance. Per-Instance chrome would make one product look
like two on one screen to distinguish buddies that are already visually
distinct.

## Consequences

`src/chat-ui.css` and the surface it dresses arrive together. A grep test
(`tests/chat-ui-tokens.test.js`) keeps literals out: no hex color,
`font-family`, or `border-radius` outside a `.chat-ui-*` block. A design that
cannot recolor the surface is a surface that stopped reading tokens.

The unshipped two designs survive as a Dated page (`docs/design/chat-mockups.html`,
#339 · 2026-09-05), which is what carries them to v2.

The overlay inherits from the default chat UI once, by hand, as literals in
`src/main.css`. The switcher repaints the chat surface and nothing else. The
bubble stays separate: changing the chat UI does not change the Character's
expression.

CONTEXT.md gains **Chat UI** beside **Chat surface**, so the design has a word
of its own.

Reversing this means shipping the chat surface with its colors inline and
revisiting modularity once a second design is wanted.

## Supersedes

This decision supersedes [ADR-0013](./0013-one-chat-ui-in-v1-switcher-in-v2.md),
which recorded the same choice alongside CSS tokens, bubble styling amendments,
and dated-page details that belong elsewhere.
