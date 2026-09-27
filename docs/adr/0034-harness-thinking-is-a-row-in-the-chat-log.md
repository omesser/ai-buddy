# A Harness's thinking is a row in the Chat log, kept like a reply

## Context

ADR-0025 made a thought transient status: a strip above the composer, replaced
by the next thought, cleared at the end of the turn, and kept by nothing. It
argued that reasoning a reader scrolls back through is the workbench arriving by
another door.

In use, the strip lost the one thing a reader wanted from it. The reasoning
behind an answer vanished as the answer landed, and it sat outside the timeline
it explained. Every harness ai-buddy's users come from keeps thinking in the
transcript, set apart from the reply. #697 called the single line a regression,
and ADR-0028 left superseding ADR-0025 as a decision of its own. The owner has
taken it (#1066).

## Decision

A turn's thinking is a Thinking row in the Chat log. It is stamped like any
other row and sits between the question and the answer it led to. It is drawn
apart from a reply: a small **Thinking** title and muted, framed text.

The row is open while the turn thinks and fills as the thought grows. When the
turn's answer lands, or the turn ends without one, the row collapses to its
title. Its title is the handle that opens and closes it again.

The row is kept exactly as a reply is kept. The Shell's session log holds it, so
a Chat surface opened or reopened later replays it, collapsed, above the reply.
A replaced session forgets it with the rest of the conversation, and nothing
keeps it across a restart. It is not written to the Action Log, because replies
are not either. The Action Log still points at the Harness's own session dump.

Thinking is never part of the answer. It leaves the wire on its own event and
never joins the text a turn returns. That half of ADR-0025 stands.

## Consequences

The Chat log has a Thinking row beside the user's line, the reply, the
permission ask and the note. ADR-0018 counts no kinds of line, so it needs no
amendment. ADR-0028's rule still bounds the row: it draws what the wire sent,
and it is not a reasoning pane or a trace inspector.

The wire does not say whose turn a thought belongs to. The live row reaches
every open Chat surface, as ADR-0025 had it. The session log holds the thought
until a turn is remembered for an Instance and files it there. A thought from
a turn that keeps nothing, such as a Behavior-only wake, is dropped when the
next one starts. On the HTTP lane two Instances can think at once, and one's
thinking can be kept above the other's reply. Naming the Instance on the event
fixes both.

A Harness that thinks on every proactive wake adds a collapsed row per wake that
speaks. The common case adds none, because adapters and servers omit reasoning
unless configured to send it.

## Supersedes

This decision supersedes [ADR-0025](./0025-harness-thinking-is-transient-not-logged.md).
