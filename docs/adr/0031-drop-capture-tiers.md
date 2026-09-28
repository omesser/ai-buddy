# Free sensing only; Capture tiers are dropped

**Status:** Accepted, with two clauses superseded by
[ADR-0032](./0032-one-consent-for-titles-and-application-names.md): free sensing
is no longer defined as the tier needing no permissions, and the "Screen
Recording permission is never requested" line below is withdrawn as written.
Dropping the Capture tiers stands.

## Context

The first sensing decision defined three tiers: Free (OS metadata), On-Demand
Capture (one screenshot with per-act consent), and Ambient Capture (periodic
sampling behind a mandatory on-device Local Gate). Only Free shipped. The
"ask the Harness for a screenshot after consent" hook was never wired, and ACP
has no standard desktop-control capability to wire it to.

Computer use belongs outside fidget: in harness-native capabilities, or in an
MCP driver such as cua-driver that the user attaches.

## Decision

**Free sensing ships. Ambient Capture, On-Demand Capture, and the Local Gate are
dropped**, not deferred. fidget never takes screenshots, never analyzes screen
pixels, and never embeds OCR or vision models for desktop awareness.

Free sensing is OS metadata: frontmost app name, window geometry, time, idle
duration, recent Behaviors. `list_windows` and `describe_screen` read window
metadata and never touch pixels.

## Consequences

- The character keeps perch, fade, window sensing, idle detection,
  `list_windows` and `describe_screen`.
- The character never "looks at the screen" in the content sense. A pet that
  reacts to what a window shows would need Capture, which this drops.
- No Local Gate, because nothing produces frames for it to filter.
- fidget owns consent for Free sensing only. Screen Recording permission is
  never requested. The sprite no longer serves as a privacy indicator, because
  it never looks at pixels.
- Agents that need pixels or desktop control get them from the Harness or an
  MCP server the user chose, not from fidget
  ([ADR-0003](./0003-no-executor-harness-owns-desktop-control.md)).

## Alternatives Considered

- **Defer Capture to v2.** "Coming soon" implies a commitment this retracts.
  Harness-native computer use already fills the role.
- **Capture in fidget, proxied to the Harness.** Makes fidget a screenshot relay
  with vision capability it does not need.
- **Keep the Local Gate without Capture.** It filters Capture frames. Without
  Capture it has no input.
