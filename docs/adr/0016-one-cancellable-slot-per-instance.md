# The Shell owns one cancellable slot per Instance, and a superseded reply is never applied

One `Slots` registry holds at most one session call per Character Instance, and
starting a call *is* the cancellation of that Instance's previous one. `wake` is
infallible: there is no busy to report, so there is no check a caller can
forget. The reply comes back through `take` as the `Wake` and the `Context` it
was computed for, together, and a reply from a superseded moment is dropped
inside `take` rather than compared against the present by whoever applies it.

The bug that argues for it is one the user can feel. Grab the sprite, a wake
goes out saying `what just happened: picked up`, throw it, it flies and lands —
and fifteen seconds later the buddy says "hey, put me down!" from the floor. The
Engine already refuses a Behavior that no longer fits, because `permitted`
requires `on_feet`; Speech had no such guard, and a Poke arriving a millisecond
after an ambient wake waited out the whole of `timeout_global` — 20s hosted,
120s on a local server — before its prompt was even sent.

## Considered Options

- **Keep the convention and check it at the call site.** One `ready()` term in a
  five-term condition, which is what shipped until #312. Correct by inspection
  of one caller, and correct only for as long as there is one caller: #16's
  Harness and #17's chat are the second and the third.
- **Queue the events instead of superseding them.** The buddy then works through
  a backlog of Pokes the user has forgotten making. Coalescing into a one-slot
  latch is the right shape for a mascot; a queue is the right shape for a job
  runner.
- **Compare the reply's `Context` against the present at the apply site.** The
  defensive shape this codebase avoids. A check the caller must remember is the
  thing being removed, not a smaller version of it.
- **Enforce it at the `Completer` seam.** `complete` is already inside the
  worker thread, so a refusal there arrives after the thread exists and the
  prompt is built. And `crates/core` does no I/O: admission control and
  cancellation are properties of a socket only the Shell holds.

## Per-Instance newest-wins, global concurrency cap

Two different questions, deliberately answered in different places. "Should this
buddy's old Poke be abandoned for its new Throw?" is always yes, and the slot
settles it with no dial. "Should buddy B wait because buddy A is mid-call?" is a
policy, and the registry is shared across Instances so that it has somewhere to
be expressed at all — N separate slots could not express it without a second,
outer mechanism. It starts at no cap, which is exactly what N independent
in-flight calls already did, and waits for #18 to have a panel to show the spend
on.

Only the slot is centralised. Sessions stay per-Instance inside each `Endpoint`,
so [ADR-0008](0008-one-harness-session.md) is untouched, and
[ADR-0004](0004-director-outside-frame-loop.md) is reinforced: every blocking
call is still on a worker thread and the frame loop still only polls.

## Consequences

Abandoning a call stops costing the endpoint rather than merely going unheard.
Superseding raises the flag #302 put behind the streaming reader, so the loser
closes its connection between SSE frames and the host stops generating.

Two calls can now overlap on one `Endpoint`, which the session bookkeeping had
been relying on not happening. A turn carries a number, because the position of
the last message no longer says whose question it is: a question nobody answered
is withdrawn by the next `open_turn`, and a turn some later one has replaced
closes without touching the session at all.

Reversing this means going back to a convention checked by hand at every call
site, and to a buddy that answers a question the world has moved past.
