# The app dispatches its own tools, and the transport follows

## Context

A Harness reaches ai-buddy's tools over MCP. Making one speak or play a
Behavior means resolving a target against the live Instances and enqueueing a
proposal on the layer that owns them. That layer is owned by the frame loop and
has no representation outside the process.

The published MCP binary was a separate process, so it could not reach any of
that. It answered from a stubbed context: no window sensing, no Instances, no
expression handle. Tool calls returned success and nothing happened on screen,
which is indistinguishable from working.

Two ways to close that gap were available. The app could serve MCP itself, or
the published binary could shrink to a shim that forwards to the running app
over a local channel.

## Decision

**Tools are dispatched inside the running app.** The app serves MCP on loopback
behind a per-run bearer token, and hands that endpoint to any Harness whose
handshake says it can use one.

A Harness that cannot is given the stdio server instead, on the precedence that
already existed. That path still answers from a stubbed context, so it reports
success and changes nothing; the shim that would fix it is tracked as work, not
as a decision.

The shim was rejected as the first move for the same reason the stub failed:
either way the dispatch has to happen inside the app, so serving directly is
the same decision with one less process in it.

## Consequences

Where a tool is dispatched is now an ownership seam rather than an accident of
packaging. Anything that needs the live Instances belongs inside the app, and a
second process can only ask.

The endpoint is reachable only from this machine, authorised per run, and
refused to anything that presents a browser origin or a method other than the
one it answers. A token that could be read from a log or a file would hand a
local process the ability to move the buddy, so it is held in memory and passed
in a header.

Two Harnesses now behave differently through no fault of the user: one reaches
the real Instances and one talks to stubs. Until the shim lands, an interface
that claims a Harness has tools is claiming too much.

Reversing this means accepting that tool calls cannot reach the Instances, which
is the state this replaced.
