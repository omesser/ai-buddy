# Tiered sensing, a mandatory Local Gate, and the sprite as privacy indicator

Sensing is tiered: a **Free** tier needing no permissions (frontmost app name,
window geometry, time, idle duration, recent Behaviors) which is all v1 ships
with; a consented **Ambient** tier (window titles plus configurable periodic
capture); and a consented **On-Demand** tier (a capture in direct response to a
Poke, call, or chat message). Every Capture passes an on-device **Local Gate** —
perceptual-hash change detection plus on-device OCR — and only a
changed-and-interesting frame may reach the Director. The Character's animation
state *is* the surveillance indicator: its eyes open and it turns toward the
window exactly when it is looking, and it visibly cannot look while asleep.

## Considered Options

- **Every capture straight to a cloud vision model.** At one capture per minute
  that is ~1,440 images/day. Simplest to build; the bill and the battery are
  brutal, and the entire screen history leaves the machine.
- **Local-only ambient understanding.** All ambient signal from on-device OCR and
  window metadata; cloud vision only on explicit Summon. Safest, and gives up the
  richest ambient reactions.
- **No ambient screenshots at all.** The original recommendation. Overruled
  deliberately: content awareness is where the companion becomes engaging rather
  than decorative, and the capture cadence is a user-configurable knob.

## Consequences

The Local Gate being non-optional is what makes "every 15 seconds" and "every 5
minutes" the same architecture instead of two different cost curves — the cadence
knob controls sampling, not spend. Most frames never leave the device. It also
yields *better* Director input than raw images: "the frontmost window's text
changed from a passing test to a failing one" beats a JPEG on both cost and
usefulness.

Permissions we own cover **sensing only**. There is deliberately no consent UI
for *acting* — the Harness owns that, and stacking a second dialog on top teaches
users to click through both. One denylist remains ours regardless of what the
Harness is permitted: password fields and user-excluded apps are never captured.

The sprite-as-indicator rule is load-bearing, not polish. No other product
category can render surveillance state as character animation, and it is the
reason a user trusts this over a menu-bar app doing the identical thing. Any
future capture path that cannot be expressed as a visible Character state is
thereby out of bounds.
