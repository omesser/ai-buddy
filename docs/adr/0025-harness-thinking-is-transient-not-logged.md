# A Harness's thinking is transient status, not a line anything keeps

## Context

Harnesses stream their reasoning over ACP as `agent_thought_chunk`, and every
TUI ai-buddy's users come from — Claude Code, Hermes, opencode — draws it while
the agent works. ai-buddy dropped it on the floor: the wire's update match ended
in a catch-all and nothing downstream was ever offered a thought. No decision
put it there. It was never wired (#483).

Three places could hold one, and each is already spoken for.

The reply the Director parses is a Behavior name and, optionally, a line to say
out loud. A thought joining it is either spoken by the buddy or read as a
Behavior name.

The Chat log is four kinds of line and "not a workbench" (ADR-0018, and
ADR-0010 before it, which drew the line for the next change that wants a fifth
thing in that window). Reasoning that scrolls, that a reader goes back through,
is the workbench arriving by another door.

The Action Log points at the Harness's own session dump rather than copying it.
Streamed reasoning is exactly that copy — of the one part the Harness itself
treats as disposable.

## Decision

A thought is transient status, drawn where nothing is kept: one line in the Chat
surface, above the composer and outside the log, saying what the Harness is
thinking right now. Each thought replaces the one before it. The end of the turn
clears it. Nothing keeps one — no line of the chat log, no line of the Action
Log, and no replay for a window that opens afterwards.

Thinking is never part of the answer. It leaves the wire on its own event and
never joins the text a turn returns.

## Consequences

The log's kinds of line are unchanged, so ADR-0018's refusal of the workbench
holds: a thought cannot be scrolled back to, filtered, or acted on.

Whoever wants reasoning kept has the Harness's own session dump, which is where
the Action Log already points.

A thought reaches every open Chat surface, like a forwarded permission request
and for the same reason: one session serves every Instance and the wire does not
say whose turn is on it. Two surfaces open during one turn both show it.

Nothing appears until a Harness is configured for visible thinking. Adapters
default to omitting it, so the common case is a strip that never opens — which
is also what an attached Harness that reasons in silence should look like.

Reversing this means deciding a thought is worth keeping, which is a widening of
what the log draws and a fifth kind of line in it.
