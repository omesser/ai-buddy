# 0008 — Primitives and declarative Behaviors

## Why

The engine owns the vocabulary; Characters compose it. Shimeji's per-character behavior
graphs went unused for fifteen years because authoring them was too hard. See
[ADR-0002](../adr/0002-engine-owns-primitives-characters-declare-behaviors.md).

## Scope

- **Primitives** — engine-owned units of motion and expression. A Character composes them
  and can never define one.
- **Behavior** — a named sequence of Primitives with weights and trigger conditions,
  declared as data in a Character Package.

Behaviors are declarative, validatable, not Turing-complete, and diffable. Validation
rejects unknown Primitives, Primitives not permitted in the current State, and sequences
that cannot terminate.

When a Character needs something the Primitives cannot express, extend the engine's
vocabulary for everyone. Never hand packages a scripting runtime.

## Acceptance criteria

- A Behavior plays its Primitives in order.
- A Behavior that becomes invalid mid-play is abandoned cleanly.
- A Behavior whose Primitives are not permitted in the current State is refused.
- Unknown Primitives are rejected by name at load time.
- A Behavior that cannot terminate is rejected at load time.

## Tests

Engine tests for playback, mid-play invalidation, State-gated refusal, and each load-time
rejection.
