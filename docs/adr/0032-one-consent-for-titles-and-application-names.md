# One consent gates window titles and application names

**Supersedes:** two clauses of [ADR-0031](./0031-drop-capture-tiers.md). Its
decision to drop the Capture tiers stands as written.

## Context

One walk over the window list reads three things about a window: where it is,
which application owns it, and what its title says. One consent gates the
title. The owning application's name is returned whether or not the user
consented to anything, and so is the frontmost application's name, which a
different platform call supplies. A user who declines still has a buddy that
knows Signal and Tor Browser are running. What they turned off is knowing which
window is which.

That split was never argued anywhere. It arrived with ADR-0005's Free tier,
ADR-0031 carried it forward, and `CONTEXT.md` and `docs/SPEC.md` restate it.
Three statements of a rule and no reasoning behind any of them, which is how a
contradiction survived inside one document: ADR-0031 defines free sensing as
needing no permissions while listing the frontmost application name in it, and
says Screen Recording is never requested while the consent catalog requests
exactly that grant in order to read titles.

## Decision

**One consent gates window titles and application names alike.** Without it the
buddy reports neither. Window geometry, stacking order, time, idle duration and
recent Behaviors stay free, as they have always been.

The case for the old split is content against category. A window title is free
text lifted out of whatever document is open, so it leaks what you are doing. An
application name comes from the bounded set of programs installed on the
machine, so it leaks only that one of them is running. On that reading the gate
belongs on titles, which is where it was put.

The counterargument decides it. A bounded set is exactly what makes a name
legible without interpretation. `Signal`, `Tor Browser`, a health app named
after the condition it tracks, a competitor's product open during an
acquisition. Knowing one of those is running needs no parsing to act on, and in
the worst case it says more than any title inside a text editor. The asymmetry
runs the other way too: titles are the half that makes desktop awareness
useful, and application names mostly are not, so the old split gated the useful
half and gave away the half that is dangerous when it is dangerous at all.

Both halves come out of the same walk, and a user has no way to reason about a
boundary drawn between them. One switch is a boundary they can hold in their
head.

This is a judgment about what a user would expect, not an observation about the
data, and the repository owner made it (#975).

**The gate is this project's, not the operating system's.** macOS hands out the
owning application name with no prompt at all. Declining to pass it on without
consent is a choice made here about a field the platform gives away.

### What this changes in ADR-0031

- Free sensing is OS metadata that never touches pixels. It is no longer
  defined as the tier that needs no permissions. Geometry, time, idle duration
  and recent Behaviors need none; titles and application names need this one
  consent.
- "Screen Recording permission is never requested" is withdrawn as written. It
  was already untrue when it was written, because enabling the titles consent on
  macOS requests that grant. What ADR-0031 meant, and what still stands, is that
  ai-buddy builds no Capture path: no screenshot, no pixel analysis, no OCR or
  vision model for desktop content.

## Consequences

- Without the consent the buddy perches on a rectangle it cannot name. The
  Director loses the frontmost application name that `docs/SPEC.md` names first
  among its inputs, the perch description falls back from "a Terminal window" to
  "a window", and `list_windows`, `describe_screen` and the windows resource
  report geometry alone. The buddy is duller until the user turns the row on,
  which is the row's whole purpose.
- On macOS the price of an application name rises to a Screen Recording grant
  the operating system would not otherwise have asked for. A user who wants
  names gives a grant scoped wider than the data it buys. That is what one
  switch costs, and the settings row has to say what is actually read.
- Excluded applications keep working. The denylist matches on the owning
  application name, so the gate belongs on what leaves the process rather than
  on what the walk reads. Gating the read would turn every excluded
  application's windows back into ordinary ones.
- The consent no longer describes what it controls, so it is renamed along with
  its settings row copy. A rename must not silently revoke a grant a user
  already gave. If the stored key changes, the user is asked again on purpose
  and told why.
- #965, the hint that explains the gated state to a user, has one boundary to
  describe instead of two.

## Alternatives Considered

- **Keep the split and defend it.** Content against category is a real
  distinction, and it is the reason the code looks the way it does. It fails on
  the bounded-set counterexample above, and it is the wrong way round on
  usefulness.
- **A second consent for application names.** Honest about the two grants macOS
  actually needs, since titles want Screen Recording and owner names want
  nothing, and it would let a user keep names while declining titles. Rejected
  because two rows for one walk is a pane a user clicks through instead of
  reading, and the permission surface stays legible only while it stays small
  (DESIGN.md decision 11).
- **Amend ADR-0031 in place.** `docs/agents/docs.md` makes an ADR immutable:
  supersede, do not rewrite. An amendment would leave no record that free
  sensing once meant "no permissions", and that claim is the reason this
  decision has to be argued rather than asserted.

## References

- ADR-0031: [Free sensing only; Capture tiers are dropped](./0031-drop-capture-tiers.md), two of whose clauses this supersedes
- ADR-0005: [Tiered sensing, consent for every Capture, a mandatory Local Gate, and the sprite as privacy indicator](./0005-sensing-posture.md), where the Free tier's "no permissions" definition began
- #975 (the decision and its argument), #965 (the hint blocked on it), #888 (the settings row copy this revisits)
