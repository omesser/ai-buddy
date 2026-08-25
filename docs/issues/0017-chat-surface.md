# 0017 — Chat surface

## Why

Summoning should feel like talking to the buddy, not opening a text box that happens to be
nearby.

## Scope

A webview window owned by the Character Instance that was Summoned. Messages route to the
attached Harness.

While a Harness turn is in flight, the Spatial Layer continues to run normally. The buddy's
visible reaction comes from Behaviors the Harness plays through the expression tools and
from the Engine's own idle life — **never from blocking on the turn**.

With no Harness attached, the surface explains how to connect one rather than failing.

## Acceptance criteria

- Summon opens the chat surface for the Instance that was summoned.
- The buddy keeps moving and reacting throughout a Harness turn.
- Answers can arrive through the character as speech and Behaviors, not only as chat text.
- With no Harness attached, the surface explains how to connect one.

## Tests

Manual, against a fake Harness with an artificially slow turn to confirm the buddy never
freezes.
