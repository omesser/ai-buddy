# Tiered sensing, consent for every Capture, a mandatory Local Gate, and the sprite as privacy indicator

Sensing is tiered: a **Free** tier needing no permissions (frontmost app name,
window geometry, time, idle duration, recent Behaviors) which is all v1 ships
with; an **Ambient** tier (window titles, and a Capture the Character asks for
while the user works); and an **On-Demand** tier (a Capture in direct response
to a Poke, a call, or a chat message). Every Capture passes an on-device **Local
Gate** — perceptual-hash change detection plus on-device OCR — and only a
changed-and-interesting frame may reach the Director. The Character's animation
state *is* the surveillance indicator: its eyes open and it turns toward the
window exactly when it is looking, and it visibly cannot look while asleep.

## No Capture is ever taken silently

Every Capture needs the user's explicit permission for that Capture. The
platform grant — Screen Recording on macOS, its equivalent elsewhere — is
necessary and not sufficient: it makes capturing possible, and it does not
authorise any particular one.

**The ask is a Character act, not a system dialog.** Wanting to look is a
Behavior the Character plays: it asks, in the chat surface when one is open
(#17) and in a small request window when none is, and it waits. Only after the
user says yes does ai-buddy ask the attached Harness for the screenshot, and
only then can the Character react to what is on screen. A refusal is an answer;
the Character takes it and carries on.

**Permission is for one Capture.** There is no standing allowance and no
remembered "allow always" — a grant the user cannot see being spent is a silent
Capture with extra steps. A user act that names the screen is its own permission
for the Capture it asks for: "what is on my screen?" does not need a second
prompt, and the answer says a Capture was taken.

This is not the second confirmation ADR-0003 refuses. That rule is about
*acting*, and it forbids ai-buddy stacking a dialog on top of the Harness's own.
Consent for sensing is ours alone, there is exactly one prompt, and the Harness
is not asked to capture anything until the prompt is answered.

## Considered Options

- **Every capture straight to a cloud vision model.** At one capture per minute
  that is ~1,440 images/day. Simplest to build; the bill and the battery are
  brutal, and the entire screen history leaves the machine.
- **Local-only ambient understanding.** All ambient signal from on-device OCR and
  window metadata; cloud vision only on explicit Summon. Safest, and gives up the
  richest ambient reactions.
- **A consented tier, sampled periodically under one standing grant.** What this
  ADR said until now: the user consents to the Ambient tier once, and a cadence
  knob then samples the screen while they work. Overruled. The knob made the
  captures the user could not see the majority of them, and a tier grant given
  in settings weeks earlier is not consent to the capture happening now.
- **An "allow always" the prompt offers.** The obvious kindness, and it converts
  this decision back into the one above on the first click.
- **No ambient Captures at all.** The original recommendation, now most of the
  way back: an ambient Capture exists, and the Character has to ask for it every
  time.

## Consequences

The Ambient cadence knob changes meaning. It bounds how often the Character may
*ask*, never how often it captures, and asking too often is its own failure — a
buddy that interrupts to request a screenshot is worse than one that never looks.

Content awareness gets rarer and costlier in attention than a sampling design
would give. That is the price of the rule, and it is paid deliberately: the
buddy reacts to screen detail it was allowed to see, and to nothing else.

The Local Gate stays mandatory, and it no longer carries the argument it was
introduced with. It exists to keep an already-consented Capture from reaching
the Director when it holds nothing new, not to make silent sampling acceptable.
Most consented frames still never leave the device.

Asking needs somewhere to happen and something to play. The chat surface (#17)
carries it when open; a small request window is Shell furniture on every
platform when it is not. A Character with no way to look like it is asking
cannot ask, which puts the ask in reach of the same Required Animation argument
as every other expression.

Permissions we own cover **sensing only**. There is deliberately no consent UI
for *acting* — the Harness owns that. One denylist remains ours regardless of
what the Harness is permitted: password fields and user-excluded apps are never
captured, and a granted permission does not lift that.

The sprite-as-indicator rule is load-bearing, not polish. No other product
category can render surveillance state as character animation, and it is the
reason a user trusts this over a menu-bar app doing the identical thing. Any
future capture path that cannot be expressed as a visible Character state is
thereby out of bounds.

Reversing this means a Capture the user did not authorise, whether by a standing
grant or a cadence they set once, and giving up the claim that the buddy only
ever sees what it was allowed to see.
