# The Chat surface renders what ACP sends and invents nothing

## Context

Attaching a Harness turned a desktop pet into something people use like an
agent client, and the Chat surface was not designed for that. A turn arrives as
plans, tool calls with their content and locations, thought chunks, stop
reasons and usage; most of it is discarded and the remains are drawn as a line
of text. The gap is real and it pulls in one direction: every missing piece has
an obvious fix in a coding-agent IDE, and shipping a few of them is how a
mascot becomes a second Zed.

Nothing off the shelf settles it either. Zed's renderer is Rust and GPUI,
VS Code's chat UI is welded to its workbench, and the ACP ecosystem publishes a
protocol and adapters but no client UI. The scope cannot be delegated to a
dependency, so it needs an edge written down instead of argued per feature.

ADR-0018 already refused to host the Harness's own interface, and ADR-0025
refused to keep its reasoning. Neither says what the surface may draw of what
the wire hands it, which is the question every one of these features asks.

## Decision

The Chat surface renders what ACP already sends. It invents nothing.

A change to the surface is in scope when it can be phrased as "the protocol
sent this field and we discarded it". A change that adds a capability the
protocol does not carry is out, and so is a change that reads a protocol field
and builds an interface around it rather than drawing it.

ai-buddy is not becoming a Harness. No agent panel, no diff viewer, no embedded
terminal, no plan editor, no tool-call inspector, no re-implementation of what
a vendor CLI does. A proposal that starts to look like a coding-agent IDE is
refused by this paragraph.

The bar for the surface is narrower than "looks like a coding agent": a user
can tell what the buddy is doing and read what it said without opening a
terminal. Depth beyond that belongs to the Harness's own session dump, which
the Action Log already points at.

A drop is either rendered or recorded as deliberate at the code that drops it.
Silence is the failure this ADR exists to stop — the next reader must be able
to tell "not supported" from "thrown away by accident".

## Consequences

Each protocol field is its own argument, and each is small: one dropped field,
one change, justified by the wire. No feature gets in on the strength of what
another client draws.

How a field is drawn stays open. Whether a diff is a diff or a path and a line
count, whether a terminal block is named and skipped, whether a long thought
gets a scroll region, and whether a long transcript belongs on a 420-point
window at all are decisions this ADR does not make. It bounds them; it does not
settle them.

The surface will keep drawing less than the Harness's own TUI, permanently.
That is the trade this ADR takes, and a complaint that it draws less is not
grounds to reopen it.

Reversing this means accepting a second agent client as the product, at which
point ADR-0018's refusal to host a Harness interface has to fall with it.
