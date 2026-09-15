# The Chat surface renders what ACP sends and invents nothing

## Context

Attaching a Harness turned a desktop pet into something people use like an
agent client, and the Chat surface was not designed for that. A turn arrives as
plans, tool calls with their content and locations, thought chunks, stop
reasons and usage; most of it is discarded and the remains are drawn as a line
of text. The gap is real and it pulls in two directions at once: the surface
has to become more legible than it is — closer to what a harness shows — while
every missing piece also has an obvious fix that is somebody else's IDE.

Nothing off the shelf settles it either. Zed is the closest thing to a reference
renderer, and it fails twice. Its `acp_thread` and `agent_ui` crates are
GPL-3.0-or-later against this repository's MIT, and even with the licence
granted, `acp_thread` is not a protocol-to-model layer that could be lifted: it
pulls `gpui`, `project`, `multi_buffer`, `language`, `language_model` and
`buffer_diff`. VS Code's chat UI is welded to its workbench.

Four permissively licensed web chat libraries come closer and still do not
settle it: assistant-ui, CopilotKit, LangGraph's SDK and Vercel's `ai`. Each
assumes React and a bundler, which `src/` does not have. Whether it should is
#723. None of them models ACP either, so every one would still need the adapter
that holds the decisions this ADR is about. The ACP ecosystem itself publishes
a protocol and adapters but no client UI. The scope cannot be delegated to a
dependency, so it needs an edge written down instead of argued per feature.

ADR-0018 already refused to host the Harness's own interface, and ADR-0025
refused to keep its reasoning and fixed the strip that shows it. Neither says
what the surface may draw of the rest of what the wire hands it, which is the
question every one of these features asks.

## Decision

The Chat surface renders what ACP already sends. It invents nothing.

A change to the surface is in scope when it can be phrased as "the protocol
sent this field and we discarded it". A change that adds a capability the
protocol does not carry is out, and so is a change that reads a protocol field
and builds an interface around it rather than drawing it.

ai-buddy is not rebuilding a vendor's IDE. No agent panel, no diff viewer, no
embedded terminal, no plan editor, no tool-call inspector — not as product
surfaces, not inside a 420-point window. A proposal that starts to look like a
coding-agent IDE is refused by this paragraph.

That refusal is about what gets built, not about how much gets drawn. The
surface needs to read more like a harness than it does today, and the line
between the two is fine enough that it has to be walked per field rather than
settled by drawing as little as possible.

The floor for the surface is that a user can tell what the buddy is doing and
read what it said without opening a terminal. It is a floor and not a ceiling.
Depth past what the wire carries belongs to the Harness's own session dump,
which the Action Log already points at.

A drop is a gap to close. Every field the wire sends is meant to reach the
reader in some form; until it does, the drop is recorded at the code that drops
it, so the next reader can tell "not drawn yet" from "thrown away by accident".
A recorded drop is a waypoint, not a resting place, and a comment that calls one
deliberate is wrong.

## Consequences

Each protocol field is its own argument, and each is small: one dropped field,
one change, justified by the wire. No feature gets in on the strength of what
another client draws.

How a field is drawn stays open. Whether a diff is a diff or a path and a line
count, whether a terminal block is drawn or named and skipped, and whether a
long transcript belongs on a 420-point window at all are decisions this ADR does
not make. It bounds them; it does not settle them. None of them decides whether
a field is read — that is settled, and reading does not wait on a rendering
decision.

Simplifying how something is drawn is allowed. Omitting it is not. An image, a
resource link or an embedded resource may reach the reader as a name and a path
rather than as the thing itself, but a turn that quietly says nothing is the
failure this ADR exists to stop. Resource links in particular are a gap to
close: ai-buddy is meant for computer use and tool use, so rich media arrives on
this wire as a matter of course.

The thought trace is contested, and this ADR does not settle it. ADR-0025
decided a thought is one transient line above the composer, replaced by the
next, cleared at the end of the turn, kept by nothing, and named reasoning a
reader scrolls back through as the workbench arriving by another door. The owner
has since called that single line a degraded experience to reverse. Giving a
thought more room than a strip means superseding or amending ADR-0025, which is
a decision of its own and not this ADR's to take; #606 is where the thought
strip is tracked and where that ADR gets written. Until then ADR-0025 stands as
written and is known to be contested — this ADR is neither grounds to ignore it
nor a reason to leave one clipped line alone.

The surface draws less than the Harness's own TUI, and how much less is a live
question rather than a settled one. What this ADR fixes is the reason: the
surface draws what the wire sends, which is a smaller set than a vendor's own
client can reach.

Reversing this means accepting a second agent client as the product, at which
point ADR-0018's refusal to host a Harness interface has to fall with it.
