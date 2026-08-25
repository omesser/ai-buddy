# Engine owns the Primitives; Characters declare Behaviors as data

The engine owns the State machine and the Primitive vocabulary — walk, fall,
climb, perch, sit, sleep, react. A Character Package cannot invent a Primitive.
It declares its Behaviors as *data*: named sequences of Primitives with weights
and trigger conditions. Declarative, validatable, non-Turing-complete, diffable.

## Considered Options

- **Engine-owned graph.** One fixed state machine ships in the app; Characters
  supply only art plus weights. Trivial to author, perfectly consistent — but
  every Character is a costume, and distinctiveness is most of why someone picks
  one.
- **Character-owned graph.** Each package declares its own behavior graph. This is
  exactly what Shimeji-ee did, in per-character XML. Fifteen years of evidence:
  the overwhelming majority of community Shimeji are art reskins of the default
  XML, because the authoring burden was too high for anyone to actually use the
  flexibility. It also means owning, versioning, and debugging a small
  programming language running inside your frame loop.

## Consequences

Characters can be as lazy as weights-only or as elaborate as thirty authored
Behaviors, and neither can hang the frame loop or crash the engine. If a
Character needs something inexpressible, that is a signal to extend the
*engine's* vocabulary for everyone — never to hand packages a scripting runtime.

This is the only version where an AI-generated Character Package is safe to
load, which is where the character library is eventually going.

Retrofitting this boundary later means rewriting every shipped Character. The
manifest schema is therefore an early commitment, kept internal and undocumented
for v1 until it has survived contact with real art.
