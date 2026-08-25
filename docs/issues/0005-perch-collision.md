# 0005 — Perch collision on window top edges

## Why

Perching on windows is what makes the buddy inhabit the desktop rather than float over it.
It is also the reason window geometry is polled at all.

## Scope

Each visible window's top edge is a **Perch**: a one-way platform.

- The sprite lands on it, walks along it, and falls off either end.
- The sprite passes *upward* through an edge from below and is never blocked.
- Window sides and bottoms are ignored entirely.

Ignoring sides and bottoms is deliberate. It avoids the sprite being trapped inside an
occluded window and avoids jitter where windows overlap. See [DESIGN.md](../../DESIGN.md)
decision 7.

## Acceptance criteria

- The sprite lands on and walks along a window's top edge.
- The sprite falls off either end of a Perch.
- The sprite passes upward through an edge from below.
- The sprite falls when its Perch moves out from under it.
- The sprite falls when its Perch disappears.
- The sprite is never placed inside a window rectangle.
- Overlapping windows do not produce jitter.

## Tests

Engine tests with constructed window rectangles, including the moved-window,
closed-window, and overlapping-window cases.
