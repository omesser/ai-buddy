# The contact-sheet on-ramp is an agent's tool, and goes if agents cannot drive it

`--format frames` exists so an agent can bring in a Character from an
ecosystem no adapter reads. That is the whole reason it is worth its weight in
`scripts/import-pet.py`. The petscodex and shimeji adapters earn their code by
reading a convention; this mode reads nothing, so the only thing it can offer
is somebody with eyes categorizing frames off a contact sheet — and the
alternative to an agent doing that is a human hand-sorting forty-odd PNGs,
which is precisely the work the on-ramp was supposed to replace.

So its acceptance test is an agent completing the flow end to end and shipping
a Character, not a passing unit test. A unit test proves the two passes hand
data to each other. It cannot prove the mode is usable, because the hard step
is the one no code performs.

The mode is provisional until an agent has used it to ship a Character. If
agents cannot drive it, delete it. Do not accumulate workarounds around it —
a workaround is a human doing the categorizing after all, and at that price
the two adapters plus a hand-written Character Manifest is the cheaper repo.
`docs/agents/contact-sheet-import.md` is the flow an agent follows, and the
document is part of the feature: a step nobody can follow is the failure mode
this decision is watching for.

## Considered Options

- **Ship it as a normal feature, tested like one.** Rejected: the unit tests
  would pass on a flow no one can complete, which is the specific failure the
  ticket named. Nothing in the test suite reaches the step that matters.
- **Leave convention-less packs with no on-ramp.** Honest, and what #112
  shipped. Rejected because it makes every pack outside two ecosystems a
  manual port, and the promise in #112 was that the tool would sheet them.
- **Build a pose classifier into the importer.** Rejected: guessing a pose
  from art is the agent's job, and a model that guesses wrong silently is
  worse than a contact sheet that asks.
- **Delete it now.** Premature. The cost of finding out is one agent driving
  it once.

## Consequences

The on-ramp is on probation, and this ADR is the record of what ends it. A
Character shipped through `--format frames` closes the question and the mode
becomes ordinary. A drive that an agent cannot finish is grounds for removal,
and removal is a smaller repo rather than a regression.

Reviewing the mode means re-driving it, not re-reading the diff. Whoever
changes `write_skeleton`, `read_frames`, or the contact sheet runs the flow
again and fixes `docs/agents/contact-sheet-import.md` wherever reality
disagrees with it.
