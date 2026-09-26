# A call waiting on the user is not superseded

**Supersedes:** one clause of
[ADR-0016](./0016-one-cancellable-slot-per-instance.md). Per-Instance
newest-wins stands as written for every call the buddy makes on its own
account. Two moments are carved out of it here.

## Context

ADR-0016 made starting a call the cancellation of that Instance's previous
one, and named the outcome in its own consequences: "a question nobody
answered is withdrawn by the next `open_turn`". That sentence was written
about a question the buddy asked the model. The Harness asks the user
questions too, as a `session/request_permission` or an `elicitation/create`,
and the turn then sits on the wire for as long as a person takes to read it.
#1001 removed the timeout on that wait, so the window is unbounded. The Poke
is the likeliest thing to arrive inside it: the ask is on screen and the
sprite is right there.

The slot cannot see any of this. `Slots::wake` supersedes on the strength of
one fact, that a newer wake exists. `Session::supersede` decides the same
question over on the Harness side from a different fact, `serving_reactive`,
and already refuses one case the slot does not: an ambient tick may not take
a turn that answers something the user did.

Those two rules disagreeing is a bug on its own, with no ask involved. The
Harness keeps the chat turn and answers it; the slot has already moved its
epoch, so `take` drops that answer as a superseded moment. The turn is paid
for, the reply is generated, and nobody is told. Add an ask and the window
grows from milliseconds to however long the user reads, which is why the
defect shows up as "the Harness's answer after a permission ask never reaches
Chat" (#1037) and as "a Poke kills the turn I was answering" (#1038). One
cause, two symptoms.

## Decision

Whether a wake may take a slot is decided in one place, `Slots::wake`, from
three facts. Newest-wins still holds, except that:

- **A call blocked on the user's own answer is not superseded, by anything.**
  Newest-wins abandons a moment the world has moved past. The user mid-answer
  is not that. The buddy asked; the answer is owed.
- **An ambient tick does not supersede a reactive call.** The Harness already
  refuses to give the turn up for one, so a slot that superseded anyway would
  throw away an answer that is still on its way. This is the rule the two
  layers were disagreeing about, stated once.

The losing wake is **dropped**, not queued. ADR-0016 rejected a queue for a
mascot and the reason holds here: a buddy working through a backlog of Pokes
the user has forgotten making is worse than a buddy that missed one.

The Completer answers the first fact, because it is the only layer that can
see a permission request. `Completer::awaiting_user` defaults to false, which
is the truth for the HTTP lane: an endpoint has no way to ask.

## Considered Options

- **A carve-out for the ask alone.** It fixes #1038 and leaves #1037 half
  fixed, because the slot would still take a turn the Harness keeps whenever
  an ambient tick lands inside a merely slow turn. Two fixes sharing one
  premise is the premise asking to be looked at.
- **Queue the dropped wake and run it when the ask settles.** The user
  answers a question and is then poked by their own gesture from a minute
  ago. ADR-0016's reason for refusing a queue does not weaken here.
- **Let a Summon interrupt where a Poke may not.** A Summon is the user
  asking for the buddy, so it has the better claim of the two. Rejected
  because the buddy is already mid-question *to that same user*, and a
  Summon that silently discards the question the user was answering is the
  defect wearing a different verb. Uniform beats a dial, per ADR-0016.
- **Bound the carve-out with a grace period.** It would stop an unanswered
  ask holding the slot forever. Rejected as a dial ADR-0016 exists to avoid,
  and unnecessary: `PendingAsks` replays an outstanding ask into any Chat
  window that opens and opens one when none is on screen, so the user always
  has a row to answer or reject.
- **Have the Shell check before calling `wake`.** The shape ADR-0016 removed.
  A rule the caller must remember is correct only while there is one caller.

## Consequences

An unanswered ask holds the Instance's slot. Pokes and Summons at the sprite
do nothing at all until the user answers or rejects, and nothing on screen
says so. That is the trade: a mascot that ignores a poke for a moment against
a mascot that hangs up on its own question.

`wake` now says whether it started a call. The Shell applies the turn
bookkeeping — the cancelled-caret note, `chat_turn`, `happened_last` — only
when it did, so a dropped wake leaves the caret of the turn still running
where it is. That is why the superseded-ask caret #1038 described no longer
appears: the ask is not superseded.

An ambient tick that lands inside a reactive turn is now spent rather than
sent. The pace still advances, so the buddy's ambient cadence is unchanged;
one muse is skipped when the buddy was already busy answering the user.

## References

- ADR-0016: [The Shell owns one cancellable slot per Instance](./0016-one-cancellable-slot-per-instance.md), the clause narrowed here
- ADR-0008: [One Harness session](./0008-one-harness-session.md), why one child's outstanding ask belongs to one Instance
- #1037 (the answer that never reached Chat), #1038 (the Poke that hung up), #1001 (the turn clock during a user ask)
