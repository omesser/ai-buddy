# A call waiting on the user is not superseded

**Supersedes:** one clause of
[ADR-0016](./0016-one-cancellable-slot-per-instance.md). Starting a call no
longer always cancels the Instance's previous one.

## Context

The Harness asks the user questions through `session/request_permission` and
`elicitation/create`. Since #1001 the turn waits for the answer with no
timeout.

`Slots::wake` replaced a call whenever a newer wake arrived. `Session::supersede`
refused to give up a reactive turn to an ambient tick. When the two disagreed,
the Harness kept the turn and answered it, and `take` dropped that answer
because the slot had already moved its epoch. That lost the answer after a
permission ask (#1037). A Poke during an ask cancelled the turn the user was
answering (#1038).

## Decision

`Slots::wake` alone decides whether a new wake replaces the call on the wire.
`director::claim` puts every `Happened` in one of four classes with an
exhaustive match, so a new event does not compile until someone classes it.
Poke, Throw, Grab, and Perch are Interactions. Summon is an Opener. Chat is a
Line. An ambient tick is Ambient.

- While the call waits on the user's answer, every new wake is dropped.
- While a reactive call is still generating, an Opener or Ambient wake is
  dropped. An Interaction or a Line replaces the call.
- Otherwise the newest wake wins, as ADR-0016 says.

A dropped wake is not queued. `Completer::awaiting_user` reports the wait. It
defaults to false, because an HTTP endpoint cannot ask.

The ambient rule is new on the HTTP lane. There a tick used to cancel the
reactive call, and the user got the tick's reply instead of the answer to
their Poke.

## Considered Options

- **Exempt only a call waiting on the user.** That fixes #1038 but not #1037,
  because an ambient tick would still replace a slow reactive turn.
- **Queue the dropped wake.** ADR-0016 rejected queues, and its reason holds.
  The user would get replies to gestures made a minute earlier.
- **Let a Summon replace a reply still generating.** The operator rejected
  this. A Summon opens Chat to read that reply, so cancelling it defeats the
  Summon.
- **Put a time limit on the wait.** Rejected as the kind of setting ADR-0016
  avoids. `PendingAsks` shows an open question in any Chat window, and opens
  one if none is showing.
- **Check before each call to `wake`.** ADR-0016 removed that shape, because
  every new caller has to remember the check.

## Consequences

An unanswered question holds the slot. Every new wake for that Instance is
dropped until the user answers or rejects it. A dropped Poke, Throw, Grab, or
Perch shows a bubble that reads "Question for you in the chat", and "chat"
opens Chat, where `PendingAsks` shows the question. An ambient tick shows
nothing, and neither does a dropped Summon or typed line, whose Chat is open.

A dropped Summon still opens Chat, because the slot does not open windows.

`wake` returns `Woke::Started`, `Woke::Dropped`, or `Woke::AwaitingUser`,
which is a drop because the user owes an answer. The Shell updates the caret,
`chat_turn`, and `happened_last` only on `Started`, so a dropped wake leaves
the running turn's caret in place.

An ambient tick during a reactive turn is skipped. The pace still advances, so
the ambient cadence does not change.

## References

- [ADR-0016](./0016-one-cancellable-slot-per-instance.md), the clause narrowed here
- [ADR-0008](./0008-one-harness-session.md), why an open question belongs to one Instance
- #1037, #1038, #1001
