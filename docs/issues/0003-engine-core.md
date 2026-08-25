# 0003 — Engine core: snapshot, frame, State machine

## Why

The Engine is the single seam for the whole Spatial Layer. Getting its contract right
makes everything downstream testable without a windowing system, a model, or waiting.

## Scope

A pure, synchronous core with no I/O and no clock.

- `WorldSnapshot` — display frames, visible window rectangles in descending z-order,
  cursor position, pending interaction verbs, elapsed milliseconds since the previous
  tick, and any Behavior proposal delivered since the last tick.
- `Frame` — sprite position and velocity, current State, current animation identifier and
  frame index, optional dialogue.
- The State machine over `grounded`, `falling`, `dragged`, `perched`, `climbing`,
  `asleep`.

Time enters only as elapsed milliseconds on the snapshot. The Engine reads no clock, holds
no timers, and performs no I/O.

## Acceptance criteria

- Every State is reachable and no State is a dead end.
- Feeding an identical snapshot sequence twice produces identical frames.
- The Engine compiles and tests without any platform, model, or network dependency.

## Tests

Engine tests construct snapshot sequences and assert on frames. No sleeping, no polling,
no wall-clock time. Assert on output, never on internal calls.
