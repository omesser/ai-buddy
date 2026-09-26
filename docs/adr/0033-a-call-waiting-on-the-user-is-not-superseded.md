# A call waiting on the user is not superseded

**Supersedes:** one clause of
[ADR-0016](./0016-one-cancellable-slot-per-instance.md). Per-Instance
newest-wins stands as written for a touch of the sprite and for a line the
user typed. The moments carved out of it here are the ones where the wake
already on the wire is the truer one.

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
three facts: whether the call on the wire is waiting on the user, whether it
is reactive, and what kind of moment the new wake is. The kind is
`director::claim`, one exhaustive classification over `Happened` with no
wildcard arm, so a new event cannot compile until someone says which it is:
an **Interaction** with the sprite (Poke, Throw, Grab, Perch), an **Opener**
of a surface (Summon), a typed **Line** (Chat), or **Ambient**. Newest-wins
still holds, except that:

- **A call blocked on the user's own answer is not superseded, by anything.**
  Newest-wins abandons a moment the world has moved past. The user mid-answer
  is not that. The buddy asked, and the answer is owed in Chat. A Poke, a
  Throw, a Grab, and a Summon are the same interruption here, and each is
  dropped. The user answers, and that same turn continues.
- **An ambient tick does not supersede a reactive call.** The Harness already
  refuses to give the turn up for one, so a slot that superseded anyway would
  throw away an answer that is still on its way. This is the rule the two
  layers were disagreeing about, stated once. The slot applies it to the HTTP
  lane as well, where it is new rather than a reconciliation: there the tick
  used to cancel the reactive call outright, and the user got the muse's answer
  instead of the one to their Poke.
- **A Summon does not supersede a reactive call that is still generating.**
  A Poke, a Throw, and a Grab are the user touching the sprite, and
  newest-wins still lets them take that turn. A Summon opens Chat. The
  reply already on the wire is the one the user is about to read, so
  cancelling it to give the Summon a turn of its own throws that answer
  away. That split holds only while the reply is still generating.

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
- **Rank a Summon above a Poke.** A Summon is the user asking for the
  buddy, so it would outrank a Poke, and a Poke would give way wherever a
  Summon does. Rejected. They are different acts. A Poke, like a Throw or
  a Grab, is the user touching the sprite, and it may take a turn that is
  only generating. A Summon opens Chat and must not cancel the reply
  already on its way. While the user is mid-answer, a Poke, a Throw, a Grab,
  and a Summon are all dropped.
- **Bound the carve-out with a grace period.** It would stop an unanswered
  ask holding the slot forever. Rejected as a dial ADR-0016 exists to avoid,
  and unnecessary: `PendingAsks` replays an outstanding ask into any Chat
  window that opens and opens one when none is on screen, so the user always
  has a row to answer or reject.
- **Have the Shell check before calling `wake`.** The shape ADR-0016 removed.
  A rule the caller must remember is correct only while there is one caller.

## Consequences

An unanswered ask holds the Instance's slot. A Poke, a Throw, a Grab, and a
Summon do nothing to that turn until the user answers or rejects in Chat, and
nothing on screen says so. That is the trade: a mascot that ignores a gesture
for a moment against a mascot that hangs up on its own question.

A Summon that arrives while a reactive reply is still generating is dropped
too. The Chat window still opens, because the slot does not open it. The
reply on the wire is the one that lands. A Poke, a Throw, or a Grab in that
same stretch still takes the turn.

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
