# 0010 — Static Director

## Why

The buddy must have a life with no model configured, offline, and whenever the model-backed
Director errors or times out. This is the configurable fallback from
[DESIGN.md](../../DESIGN.md) decision 5.

## Scope

Weighted selection over the active Character's declared Behaviors, gated by their trigger
conditions. No model, no network.

Used when no model is configured, when the Director is disabled in settings, and as the
fallback on any Director error or timeout.

## Acceptance criteria

- Weighted selection is deterministic given a seeded source.
- Trigger conditions gate selection correctly.
- Recently played Behaviors are suppressed so the buddy does not visibly repeat itself.
- The buddy has idle life with no network connection and no model configured.

## Tests

Engine tests with a seeded source: distribution over many selections, trigger gating, and
repetition suppression.
