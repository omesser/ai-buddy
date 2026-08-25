# 0011 — Model-backed Director

## Why

This is the riskiest unproven part of the product. A Director that proposes dull or
repetitive Behaviors makes ai-buddy feel worse than static weights, which is a thesis
failure rather than a bug. Measure it early against 0010 with the same two Characters.

## Scope

A `Director` implementation that proposes a Behavior identifier plus optional dialogue.
The Shell wakes it on a timer and on notable events: frontmost application changed, idle
duration crossed a threshold, or the buddy has been in one State beyond a bound.

**v1 context is the free sensing tier only** — frontmost application name, idle duration,
time of day, recent Behavior identifiers, and the active Character's Personality Prompt.
No window titles, no screen capture, no clipboard, no input contents.

The exact payload is inspectable in settings.

A proposal is advisory. The Engine may refuse it. The Director is never awaited on the
render path; a pending proposal is applied on the next tick or discarded.

## Acceptance criteria

- A valid proposal is applied on the next tick.
- An unknown Behavior identifier is refused without disrupting current play.
- A Director error or timeout falls back to the static Director with no visible stall.
- The buddy keeps moving and reacting while a proposal is in flight.
- Settings shows the exact payload sent.
- Wake frequency is user-configurable, and the Director can be turned off entirely.

## Tests

Engine tests with a fake `Director`: valid proposal, unknown identifier, proposal during
Grab, and error fallback. Quality is assessed by hand against the static Director.
