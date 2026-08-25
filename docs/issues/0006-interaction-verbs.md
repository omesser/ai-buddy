# 0006 — The five interaction verbs

## Why

The verb set is fixed at five because every verb is a tax on every Character that will
ever exist. Fixing it now prevents the required animation set from growing later.

## Scope

| Input | Verb | Result |
|---|---|---|
| press and move | **Grab** | sprite follows the cursor, State `dragged` |
| release with velocity | **Throw** | ballistic arc until it lands |
| click | **Poke** | reaction animation, optionally dialogue |
| right-click | **Menu** | character switching, settings, quit |
| double-click | **Summon** | opens the chat surface |

Verbs enter the Engine on the snapshot. Additions wait for v2.

## Acceptance criteria

- Each verb produces its expected State or output.
- Grab overrides any State.
- Releasing a Grab with velocity enters Throw; without velocity, it falls.
- Verbs arriving in the same tick resolve deterministically.
- A Director proposal arriving during a Grab is deferred or dropped, never yanking the
  sprite.

## Tests

Engine tests per verb, plus the same-tick collision case and the proposal-during-Grab
case.
