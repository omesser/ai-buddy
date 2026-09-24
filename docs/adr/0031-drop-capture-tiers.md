# Free sensing only; Capture tiers are dropped

**Status:** Accepted, with two clauses superseded by
[ADR-0032](./0032-one-consent-for-titles-and-application-names.md): free sensing
is no longer defined as the tier needing no permissions, and the "Screen
Recording permission is never requested" line below is withdrawn as written.
Dropping the Capture tiers stands.

**Supersedes:** [ADR-0005](./0005-sensing-posture.md)

## Context

ADR-0005 defined three sensing tiers: Free (OS metadata without consent), On-Demand Capture (single screenshot with per-act consent), and Ambient Capture (periodic sampling under mandatory Local Gate with explicit consent). ADR-0005 deferred the two Capture tiers to post-v1, leaving Free sensing as the only tier shipped.

The "ask Harness for screenshot after consent" hook described in ADR-0005 was never wired. As of September 2026, research found the ACP attach has no standard desktop-control capability, so even a consented Capture would have no path to an attached harness unless the user connects a separate MCP server for it.

Computer use for agents — including desktop pixel access and control — belongs outside ai-buddy: in harness-native capabilities (Claude Code on macOS/Windows, Codex Computer Use plugin, Cursor Cloud Agents, Hermes computer_use toolset) or in multi-harness MCP drivers such as cua-driver. September 2026 research evaluated the multi-harness computer-use landscape and recommended cua-driver as the primary cross-platform option.

Dropping Capture means ai-buddy never embeds, ships, or owns screenshot capability. It does not mean agents the user runs can never see pixels — a user running a CU-capable harness or attaching a computer-use MCP server (like cua-driver) can still grant that agent desktop control, just not through ai-buddy's own code.

## Decision

**Free sensing ships; Ambient Capture, On-Demand Capture, and the Local Gate are dropped** — not deferred, not waiting. ai-buddy never takes screenshots, never analyzes screen pixels, and never embeds OCR or vision models for desktop content awareness.

Free sensing remains as defined in ADR-0005 and CONTEXT.md: OS metadata (frontmost app name, window geometry, time, idle duration, recent Behaviors) with no permissions required. The MCP tools `list_windows` and `describe_screen` query window metadata from the OS without touching pixels.

The "ask Harness for screenshot after consent" path in ADR-0005 is withdrawn. No code path for ai-buddy to request or process screenshots is built.

## Consequences

**For the product:**

- The pet keeps its surface interactions: perch, fade, window geometry sensing, idle detection, `list_windows`, and `describe_screen` all remain. The sprite still reacts to the desktop and obeys physics.
- No buddy content-awareness of pixels: ai-buddy never analyzes screen content, never takes screenshots itself, and never embeds OCR or vision models for desktop sensing.
- The Local Gate (ADR-0005) is unneeded because nothing asks for Capture.
- Agent CU is still available when a user runs a CU-capable harness (Cursor Cloud Agent, Codex with Computer Use plugin, Hermes with computer_use toolset, interactive Claude Code on macOS) or attaches an MCP server like cua-driver. The control comes from the harness or MCP server the user chose, not from ai-buddy.

**Tradeoff:** The buddy itself never "looks at the screen" in the content sense. A future where the pet visually reacts to what is on the desktop (noticing a specific app's content, reading notifications) is not this path. That would require Capture, which this decision drops.

**For permissions and consent:** ai-buddy owns consent for Free sensing only. Screen Recording permission is never requested. The sprite-as-privacy-indicator rule (ADR-0005: "Its eyes open and it turns toward the window exactly when it is looking") is unneeded because the buddy never looks at pixel content.

**For documentation:** SPEC.md, CONTEXT.md, and DESIGN.md are updated to reflect this drop. Any remaining references to Capture tiers "coming soon" or "deferred to v2" are removed or rephrased to point at this ADR.

**User-facing meaning:** Users who want agent desktop control attach cua-driver (or another CU MCP) to their harness of choice. Users who want only the animated sprite, sensing, and chat attach no MCP. The decision separates ai-buddy's role (spatial layer, personality, perch) from the harness's role (functional layer, execution).

## Alternatives Considered

- **Defer Capture to v2.** The ADR-0005 position. Leaving it as "coming soon" implies a commitment that this decision explicitly retracts. The code path was never wired, and the architectural role (pixels from desktop to agent) is better filled by harness-native CU or multi-harness MCPs.
- **Implement Capture for ai-buddy to proxy to harness.** Makes ai-buddy a screenshot relay and embeds vision/OCR capability it does not need. The harness or MCP already has this capability; duplicating it in ai-buddy adds complexity for no user-facing gain.
- **Keep Local Gate without Capture.** The Local Gate was designed to filter consented Capture frames. Without Capture, the Gate has no input and no purpose.

## References

- ADR-0005: [Tiered sensing, consent for every Capture, a mandatory Local Gate, and the sprite as privacy indicator](./0005-sensing-posture.md) (superseded by this ADR)
- ADR-0003: [ai-buddy ships no Executor; the Harness owns desktop control](./0003-no-executor-harness-owns-desktop-control.md)
- Research (September 2026): computer-use landscape evaluation and cua-driver recommendation
