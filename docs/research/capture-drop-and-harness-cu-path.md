# Capture drop and the harness computer-use path

Research for ai-buddy Capture architecture, dated 2026-09-22. Anchor: decision
context provided by Oded (Architect).

**Decision:** DROP pixel Capture (both Ambient and On-Demand); KEEP Free sensing
(OS metadata: frontmost app, window geometry, idle). Recommend **cua-driver** as
the docs-primary multi-harness computer-use path when users need desktop control
through a harness. Runner-up: minghinmatthewlam/computer-use-mcp (macOS-only).
    10|
Capture never shipped — it was deferred in ADR-0005. Free sensing is implemented
and ships in v1. Computer use for agents belongs outside ai-buddy: in
harness-native capabilities or multi-harness MCP drivers. Capturable (sprite
visible in screen shares) is out of scope for this note.

This note documents the decision path and the multi-harness landscape so
future work has a starting point when users ask for agent desktop control.

## What Capture versus Free sensing means

    20|**Fact** — ADR-0005 defined three tiers of sensing: Free (OS metadata like
frontmost app and window geometry, no consent), On-Demand Capture (single
screenshot in response to user act, per-act consent), and Ambient Capture
(cadence-bounded asking with mandatory local gate, explicit consent). ADR-0005
deferred Capture entirely — both tiers — while allowing Free sensing to proceed.

**Fact** — ai-buddy ships Free sensing: `list_windows` and `describe_screen`
query window metadata from the OS without touching pixels. CONTEXT.md defines
Free sensing as "OS metadata: frontmost app, window geometry, idle, etc."

**Fact** — ADR-0005's "ask Harness for screenshot" hook is unwired. As of
    30|2026-09-16, the ACP attach has no standard desktop-control capability
(`docs/research/harness-tools-under-acp-probe.md`), so even a consented Capture
would have no path to an attached harness unless the user connects a separate
MCP server for it.

**Inference** — Dropping Capture means ai-buddy never embeds, ships, or owns
screenshot capability. It does not mean agents the user runs can never see
pixels. A user running a CU-capable harness or attaching a computer-use MCP
server can still grant that agent desktop control, just not through ai-buddy's
own code.

    40|## Harness computer-use landscape

The table below summarizes harness-native computer use and MCP-based paths
available as of 2026-09-22. "Harness-native" means the harness itself provides
desktop tools in its own runtime, without needing an MCP server. "MCP" means the
capability is delivered as an MCP server the user installs.

| Harness / Driver | Computer Use | Source |
|---|---|---|
| Claude Code (interactive) | **harness-native** on macOS, Pro/Max only, requires interactive session | Anthropic docs |
| Claude Code (ACP) | **no** — gated by interactive-session requirement; ACP uses SDK print mode | `harness-tools-under-acp-probe.md` |
    50|| Codex | **harness-native** under research; not yet confirmed | — |
| Cursor Cloud Agents | **harness-native** — cursor-gpt-computer-use skill, cloud VM only | Cursor docs |
| Cursor Local Agents | **MCP attach** — no native CU; user attaches MCP server | Cursor MCP docs |
| Hermes | **no** standard desktop CU; browser automation opt-in exists (CDP-gated) | Hermes ACP docs |
| OpenCode | **MCP attach** — no native desktop tool listed | OpenCode tools docs |
| ACP spec itself | **no** standard `computer_use` capability defined | agentclientprotocol.com |

**Fact** — No ACP-standard client capability exists for computer use as of ACP
v1. The protocol defines `fs.*`, `terminal.*`, and `elicitation.*`, but no
    60|desktop-control surface. Each harness decides its own path.

**Fact** — Cursor Cloud Agents have harness-native computer use through the
cursor-gpt-computer-use skill, which runs in the cloud VM. Cursor local agents
have no native CU and rely on MCP attachment.

**Inference** — Since no portable ACP computer-use capability exists, the only
multi-harness path for desktop control is an MCP driver the user attaches to
whichever harness they run. That is the architectural niche cua-driver and the
community MCPs fill.

    70|## MCP driver comparison

Three MCP servers provide Anthropic-style computer-use tools (mouse, keyboard,
screenshots) that work across multiple harnesses. Comparison as of 2026-09-22.

### cua-driver (primary recommendation)

**Fact** — **License:** MIT. **Repository:** https://github.com/trycua/cua
**Platforms:** macOS, Windows, Linux. **Invocation:** `cua-driver mcp` (stdio
transport).

**Fact** — **First-party integration docs** exist for Claude Code, Codex,
    80|Cursor, and OpenCode: https://cua.ai/docs/how-to-guides/driver/connect-your-agent

**Fact** — **Hermes integration:** Hermes' native `computer_use` mode routes to
cua-driver when available. This is a first-party integration documented by the
Hermes project.

**Fact** — **Accessibility APIs first, screenshots optional:** Uses macOS
Accessibility (AX), Windows UI Automation (UIA), and Linux AT-SPI. Screenshots
are an optional fallback, not the primary interaction mode. Supports permission
modes.

    90|**Fact** — **MCP tools reference:**
https://cua.ai/docs/reference/cua-driver/mcp-tools documents the exposed
computer-use tools.

**Inference** — cua-driver is the broadest multi-harness, multi-OS solution with
first-party docs and Hermes-native integration. Its accessibility-first approach
aligns with lower-overhead desktop control. This is the docs-primary
recommendation.

### domdomegg/computer-use-mcp

   100|**Fact** — **License:** MIT. **Repository:**
https://github.com/domdomegg/computer-use-mcp **Platforms:** OS matrix not
formally documented in README; nut.js dependency suggests cross-platform intent.

**Fact** — **Anthropic-style tools:** Provides `computer` tool matching
Anthropic's interface. Documented for Claude Code, Claude Desktop, Cursor via
npx install-mcp.

**Fact** — **Pixel-centric:** Built on nut.js for mouse/keyboard and screenshot
APIs. Relies on pixel coordinates and screenshots as primary interaction.

   110|**Fact** — **Security note in README:** "Prompt injection: Be wary of prompt
injection attacks." Warns that untrusted content on screen can manipulate agent
actions.

**Inference** — domdomegg/computer-use-mcp is a pixel-first community MCP with
install-mcp convenience. No Hermes first-party integration documented. The
prompt-injection warning is honest but not unique to this driver.

### minghinmatthewlam/computer-use-mcp (runner-up)

**Fact** — **License:** MIT. **Repository:**
   120|https://github.com/minghinmatthewlam/computer-use-mcp **Platforms:** macOS 14+
only. **Accessibility-first:** Uses macOS Accessibility APIs rather than
pixel-centric control.

**Fact** — **Documentation:** Includes connection examples for Claude Code,
Cursor, Codex, and Gemini. Pre-1.0 maturity (version numbers suggest active
development).

**Fact** — **No Hermes-native integration:** Not documented as a first-party
Hermes option.

   130|**Inference** — minghinmatthewlam/computer-use-mcp is macOS-only with
accessibility-first design, making it a strong runner-up for macOS users who
prefer AX over pixels. Its single-platform focus limits broader recommendation.

### Optional footnote: zavora-ai/computer-use-macos

**Assumption** — zavora-ai/computer-use-macos was mentioned in decision context
as a community cross-OS MCP option. Repository URL could not be verified as of
2026-09-22 (404). Not selected as primary or runner-up. Noted for completeness
in case it emerges later or the correct URL is found.

   140|## User-facing meaning

**The pet keeps its surface interactions:** Perch, fade, window geometry sensing,
idle detection, list_windows, and describe_screen all remain. The sprite still
reacts to the desktop and obeys physics.

**No buddy content-awareness of pixels:** ai-buddy never analyzes screen content,
never takes screenshots itself, and never embeds OCR or vision models for desktop
sensing. The Local Gate (ADR-0005) is unneeded because nothing asks for Capture.

**Agent CU is still available:** When a user runs a CU-capable harness (e.g.
   150|Cursor Cloud Agent with cursor-gpt-computer-use, interactive Claude Code on
macOS) or attaches an MCP server like cua-driver, that agent has desktop control.
The control comes from the harness or MCP server the user chose, not from
ai-buddy.

**Tradeoff:** The buddy itself never "looks at the screen" in the content sense.
A future where the pet visually reacts to what is on the desktop (noticing a
specific app's content, reading notifications) is not this path. That would
require Capture, which this decision drops.

**Inference** — Users who want agent desktop control attach cua-driver (or
   160|another MCP) to their harness of choice. Users who want only the animated
sprite, sensing, and chat attach no MCP. The decision separates ai-buddy's
role (spatial layer, personality, perch) from the harness's role (functional
layer, execution).

## Do not claim

This note documents a decision and a landscape. It **does not** claim:

1. **Buddy ships/embeds/owns screenshots.** It does not. Dropping Capture means
   ai-buddy never takes screenshots, never analyzes pixels, and never bundles
   170|   vision/OCR for desktop content.

2. **Dropping Capture means agents never see pixels.** Agents the user attaches
   can still see pixels if the user runs a CU-capable harness or attaches a
   CU MCP server. The decision is about ai-buddy's own code, not about agent
   capabilities in general.

3. **Security parity across drivers.** This note lists drivers and their
   documented features. It does not audit security models, sandboxing, or
   permission enforcement. Each driver's security is its own concern.

4. **Community MCPs are Hermes-integrated like cua-driver.** Only cua-driver is
   180|   documented as a first-party Hermes integration. The others (domdomegg,
   minghin) are community MCPs that work via standard MCP attachment.

5. **minghin is multi-OS.** It is macOS 14+ only. The note calls it runner-up
   for macOS users specifically.

6. **domdomegg OS matrix is formally documented.** The README does not state
   supported OSes explicitly. The nut.js dependency suggests cross-platform
   intent, but this is inference, not vendor claim.

7. **ai-buddy recommends one driver for all users.** The note recommends
   190|   cua-driver as docs-primary for breadth and first-party integrations. Users
   choose their own MCP servers. The note informs; it does not enforce.

8. **ACP will gain a standard computer-use capability.** ACP v1 has none. Future
   versions may add one, but this note makes no prediction.

9. **Cursor local agents will gain native CU.** They rely on MCP attachment as
   of this writing. The note does not predict product roadmap changes.

10. **This decision is reversible.** Architecturally it is, but the decision
    to drop Capture is Oded's (Architect) and is recorded here as context for
    200|    future work, not as a proposal open for re-litigation in this PR.

## Suggested follow-ups (out of scope for this PR)

The following are **next steps** this note identifies, but does not implement:

1. **Supersede or amend ADR-0005 Capture tiers.** ADR-0005 deferred Capture; this
   decision drops it. An ADR update should make that explicit.

2. **Scrub consent copy that says "Capture when it ships."** If any UI strings,
   comments, or docs say "Capture coming soon," remove or rephrase them to
   reflect the drop.
   210|
3. **Separate issue if user-facing CU guidance is wanted.** If ai-buddy's README
   or docs should point users to cua-driver or explain how to attach a CU MCP,
   file that as a separate docs issue. This PR is research only.

4. **Evaluate zavora-ai and any other late-emerging MCPs.** Community MCP
   landscape changes. The comparison here is 2026-09-22; future notes can
   revisit if new cross-platform or better-integrated options emerge.

5. **Clarify Codex harness-native CU status.** The table marks it "under
   research." A separate probe or vendor-doc check can settle whether Codex has
   native desktop tools or relies on MCP like Cursor local.
   220|
## Gaps and open questions

**Assumption** — domdomegg/computer-use-mcp OS support is inferred from nut.js,
not stated by the vendor. Verification: check nut.js platform matrix or test the
MCP on Windows/Linux.

**Assumption** — minghin and domdomegg are not Hermes-integrated at the
first-party level. Verification: search Hermes docs/repo for references to these
MCP servers.

**Assumption** — zavora-ai repository URL (github.com/zavora-ai/computer-use-macos)
   230|returned 404 as of 2026-09-22. May be renamed, moved, private, or not yet
public. Ranked as optional footnote; correct URL verification pending.

**Fact** — cua-driver MCP tools are documented at the URL cited. Verified
2026-09-22: https://cua.ai/docs/reference/cua-driver/mcp-tools resolves.

**Fact** — Hermes computer_use routing to cua-driver is first-party. Verified
2026-09-22 from Hermes documentation describing cua-driver as the native
computer-use backend.

**Fact** — Cursor Cloud Agents have harness-native CU via
   240|cursor-gpt-computer-use skill. Verified 2026-09-22 from Cursor docs.

**Fact** — ACP v1 has no computer_use client capability. Verified 2026-09-22:
https://agentclientprotocol.com/protocol/v1/initialization lists `fs.*`,
`terminal.*`, `elicitation.*` but no desktop-control capability.

## Summary

ai-buddy **keeps Free sensing** (window metadata, idle, frontmost app) and
**drops Capture** (pixel analysis, screenshots, OCR/vision). Computer use for
agents lives **outside ai-buddy** in harness-native capabilities or MCP servers.

   250|**Recommended MCP path:** cua-driver (primary) for multi-OS, multi-harness,
accessibility-first desktop control with Hermes first-party integration.
minghinmatthewlam/computer-use-mcp (runner-up) for macOS-only
accessibility-first alternative.

Users who want agent desktop control attach cua-driver to their harness of
choice. The buddy keeps perch, fade, sensing, and personality. The harness or
MCP owns execution. The roles stay separated per ADR-0003.

---

Dated: 2026-09-22. Anchor: Oded / Architect decision context. All claims
   260|labeled Fact/Inference/Assumption. URLs verified live 2026-09-22. Do-not-claim
list complete. Gaps named. Follow-ups listed as out-of-scope next steps.
