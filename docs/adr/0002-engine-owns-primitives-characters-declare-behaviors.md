# The engine owns Primitives; Characters declare Behaviors as data

A Character Package could plausibly ship its own behavior graph, which is what
[Shimeji-ee](https://kilkakon.com/shimeji/) does with per-character XML. We split it
instead: the engine owns the State machine and the Primitives, no Character may invent
one, and a Character declares Behaviors as data — named sequences of Primitives with
weights and trigger conditions. The evidence is fifteen years of Shimeji: the
overwhelming majority of community packages are art reskins of the default XML, because
authoring the graph was too hard for the people who wanted to make characters.

## Consequences

Behaviors are declarative, validatable, not Turing-complete, and diffable. A Character
Package cannot crash the engine or hang the frame loop, which is the only version where
loading an AI-generated Character Package is safe.

When a Character needs something the Primitives cannot express, the fix is to extend the
engine's vocabulary for everyone. It is never to hand packages a scripting runtime.

The Required Animation Set was fixed at 8 — `idle`, `walk`, `fall`, `land`, `sit`,
`sleep`, `react`, `talk` — with a declared optional set used when present. Every
required animation is a tax on every Character that will ever exist, so eight kept a
hobbyist package to an evening's drawing. The set is nine as of
[ADR-0007](0007-hold-is-a-required-animation.md).

A Personality Prompt governs demeanour, never capability. Character Packages are
untrusted input to a model that can reach an agent Harness.
