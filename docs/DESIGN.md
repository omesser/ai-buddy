# ai-buddy — Design

An AI-powered desktop companion in the spirit of Windows 95-era desktop mascots.
A pixel-art sprite lives on your screen, obeys physics, perches on your window
edges, has a personality, and — when you ask it to — gets an agent harness to do
real work on your machine.

Vocabulary is defined in [CONTEXT.md](../CONTEXT.md). Decisions that carry
lock-in are recorded in [docs/adr/](./adr/).

---

## 1. Shape of the thing

Two layers, deliberately separate:

**Spatial Layer** — always on, entirely local, no model in the loop. Physics,
window geometry, animation, the five interaction verbs. Runs offline, with zero
permissions granted, at a flat cost of zero. This is the product's centre of
gravity and the part nothing else has built.

**Functional Layer** — asynchronous, explicitly Summoned, delegates to a
user-attached Harness. The Character reports on it; the Character is not it.

The rule that keeps them honest: **the Spatial Layer must be worth having with
the model switched off.** If the sprite is not delightful when nothing is
attached, the Functional Layer does not save it.

```
┌─────────────────────────────────────────────────────────────┐
│  ai-buddy (Tauri)                                           │
│                                                             │
│  ┌───────────────────────┐   ┌───────────────────────────┐  │
│  │ Spatial Layer (Rust)  │   │ Webview (sprite render)   │  │
│  │ • physics @ 60fps     │──▶│ • PNG strips, pixelated   │  │
│  │ • window poll @ 10Hz  │   │ • integer nearest-neighb. │  │
│  │ • Perch collision     │   │ • per-pixel hit test      │  │
│  │ • Behavior player     │   └───────────────────────────┘  │
│  └──────────┬────────────┘                                  │
│             │ occasional                                    │
│  ┌──────────▼────────────┐   ┌───────────────────────────┐  │
│  │ Director              │   │ Local Gate                │  │
│  │ • proposes Behaviors  │◀──│ • phash change detect     │  │
│  │ • never in frame loop │   │ • on-device OCR (Vision)  │  │
│  └───────────────────────┘   └─────────▲─────────────────┘  │
│                                        │ Captures           │
│  ┌───────────────────────┐   ┌─────────┴─────────────────┐  │
│  │ MCP server (ours)     │   │ Sensing                   │  │
│  │ speak / play_behavior │   │ • CGWindowList @10Hz      │  │
│  │ list_windows          │   │ • Ambient (consented)     │  │
│  │ describe_screen       │   │ • On-Demand               │  │
│  └──────────┬────────────┘   └───────────────────────────┘  │
└─────────────┼───────────────────────────────────────────────┘
              │ MCP
     ┌────────▼─────────┐        ┌──────────────────────────┐
     │ Harness (BYO)    │───────▶│ Executor (theirs)        │
     │ Claude Code /    │        │ native computer use, or  │
     │ Codex / any      │        │ desktop-control MCP srv  │
     └──────────────────┘        └──────────────────────────┘
```

---

## 2. Settled decisions

| # | Decision | Choice |
|---|---|---|
| 1 | Centre of gravity | Nostalgia companion first; productivity as a second layer |
| 2 | Meaning of "interact with screen" | Spatial first (geometry, Perches); functional second (Summoned) |
| 3 | Platforms | Cross-platform architecture, macOS first, Windows stubbed |
| 4 | Harness | BYO via MCP — see #17 |
| 5 | Runtime | Tauri (Rust + webview) — [ADR-0001](./adr/0001-greenfield-tauri-not-fork-windowpet.md) |
| 6 | Model's role in idle | Director proposes Behaviors occasionally — [ADR-0004](./adr/0004-director-outside-frame-loop.md) |
| 7 | Character | First-class package format, with pre-built characters shipped |
| 8 | macOS window awareness | `CGWindowListCopyWindowInfo` polling @10Hz, no permissions |
| 9 / 14 | Behavior ownership | Engine-owned Primitives, character-declared Behaviors — [ADR-0002](./adr/0002-engine-owns-primitives-characters-declare-behaviors.md) |
| 10 | Physics & verbs | Gravity + Throw; Perch = window top edges only; five verbs, capped |
| 11 | Z-order | Always-on-top, non-activating, `canJoinAllSpaces`; aggressive auto-hide |
| 12 / 16 | Sensing | Ambient titles + configurable periodic capture + On-Demand — [ADR-0005](./adr/0005-sensing-posture.md) |
| 13 | Codebase origin | Greenfield; WindowPet (MIT) as reference — [ADR-0001](./adr/0001-greenfield-tauri-not-fork-windowpet.md) |
| 15 | Voice | Hotkey PTT + click-to-chat; wake word opt-in, on-device detection only |
| 15b | Transcription | Trait: Apple `SpeechAnalyzer` on macOS 26+, `whisper.cpp` elsewhere |
| 17 / 22 | Computer use | MCP server + MCP host; no first-party Executor — [ADR-0003](./adr/0003-no-executor-harness-owns-desktop-control.md) |
| 18 | Capture processing | Mandatory Local Gate; only changed/interesting frames escalate |
| 19 | Permissions we own | Sensing only. Never duplicate the Harness's action prompts |
| 20 | Memory | Light, local, plaintext, per-Character. Chat history session-scoped |
| 21 | No harness attached | Fully charming — full Spatial Layer, chat shows a connect nudge |
| 23 | Art | True pixel art, integer nearest-neighbour — [ADR-0006](./adr/0006-pixel-art-integer-scaling.md) |
| 24 | Displays | One buddy, one screen; follows across Spaces; stays put across monitors |
| 25 | Release staging | v1 charm · v1.1 chat · v2 actions |

---

## 3. Spatial Layer

**Physics.** Gravity, velocity, ballistic Throw, landing. Fixed-timestep
integrator at 60Hz, decoupled from the window poll.

**Perches.** Every visible window's **top edge only** is a one-way platform: the
sprite lands on it, walks along it, falls off the ends, and passes upward through
it. Sides and bottoms are ignored — this is ~80% of the perceived aliveness for
~20% of the collision work, and it sidesteps the bad cases (sprite trapped inside
an occluded window, jitter where two windows overlap).

**Window geometry.** `CGWindowListCopyWindowInfo` polled at 10Hz gives bounds,
owner app, and layer with **no permission prompt**. Window *titles* need Screen
Recording — deliberately not required for the Spatial Layer. Smoothness comes
from interpolating in the render layer, not from event fidelity.

**Z-order.** One always-on-top, non-activating panel: never takes focus, never
appears in the app switcher, `canJoinAllSpaces` + stationary so it follows you
across Spaces. Dynamic restacking to fake "hiding behind your window" is
rejected — it flickers on every platform. Invest in auto-hide rules instead
(fullscreen app frontmost, screen sharing, Do Not Disturb, hotkey).

**Interaction verbs.** Capped at five. Every added verb is a tax on every future
Character.

| Input | Verb | Result |
|---|---|---|
| press + move | Grab | sprite follows cursor |
| release with velocity | Throw | ballistic arc, then lands |
| click | Poke | reaction animation, possibly dialogue |
| right-click | Menu | character switch, settings, quit |
| double-click | Summon | opens the Functional Layer |

**Click-through.** Transparent-window mouse ignoring is per-window, not
per-pixel. A small sprite in a large transparent window eats clicks across the
whole rect unless you hit-test the sprite's alpha and toggle
ignore-mouse-events accordingly. WindowPet (MIT) has solved this; lift it with
attribution.

---

## 4. Character Package

Engine owns the Primitives. Characters compose them as data. Sketch, not final:

```
mochi/
├── character.json        # identity, render_mode, required animation map
├── behaviors.json        # named Behaviors: Primitive sequences, weights, triggers
├── personality.md        # Personality Prompt — demeanour only, never capability
└── sprites/
    ├── idle.png          # PNG strips; sizes in the Character Manifest
    ├── walk.png
    └── ...
```

Rules that hold regardless of schema detail:

- A Character **cannot invent a Primitive**. If one is missing, the engine's
  vocabulary gets extended for everyone — no per-package scripting runtime.
- A Character **cannot declare capability**. The Personality Prompt governs
  demeanour. Packages are untrusted input; a prompt-injection vector into an
  agent with computer control is not theoretical.
- Behaviors are **declarative, validatable, non-Turing-complete, diffable**. A
  lazy Character ships weights only; an elaborate one ships thirty Behaviors.
  Neither can hang the frame loop.
- The format is internal for v1. Two built-in Characters, no public authoring
  docs until the schema has survived contact with real art.

---

## 5. Functional Layer

**Summon** via hotkey push-to-talk, click-to-chat, or (opt-in) wake word.
Wake-word detection runs **on-device** (openWakeWord / Porcupine) — no audio
leaves the machine until the name fires. Transcription is a trait: Apple
`SpeechAnalyzer` / `SpeechTranscriber` on macOS 26+ (benchmarked ~2× faster than
Whisper Large V3 Turbo, no model download, no bundle bloat), `whisper.cpp` via
`whisper-rs` on Windows, Linux, and older macOS.

**ai-buddy is an MCP server and an MCP host.** As a server it exposes
`speak`, `play_behavior`, `list_windows`, `describe_screen` — that is the BYO
story, and it costs nothing extra because those tools must exist anyway. As a
host it attaches whatever Harness the user configures. One first-party adapter
so the out-of-box path isn't "install a harness first."

**We ship no Executor.** Clicking and typing come from the Harness's native
computer use or from a user-configured desktop-control MCP server. See
[ADR-0003](./adr/0003-no-executor-harness-owns-desktop-control.md).

**The sprite is not a puppet.** The Director already samples windows and
screen state, so ai-buddy *observes* the agent working rather than waiting to be
told. The Character stays visibly alive across a 30-second agent turn — which is
exactly where a naive design looks broken.

---

## 6. Sensing and trust

Three tiers, in order of escalation:

1. **Free** — no permissions. Frontmost app *name*, window geometry, time of
   day, idle duration, recent Behaviors, the Personality Prompt. Enough for good
   idle life. This is what v1 ships with.
2. **Ambient** (consented) — window titles, plus configurable periodic capture.
3. **On-Demand** (consented) — a capture taken in direct response to a Poke,
   call, or chat message.

**The Local Gate is mandatory.** Every Capture is processed on-device first —
perceptual hash for meaningful change, on-device OCR via the macOS `Vision`
framework — and only a changed-and-interesting frame escalates to the Director.
This is what makes "every 15 seconds" and "every 5 minutes" the same
architecture instead of two different cost curves, and it gives the Director
*better* input than raw images: "the frontmost window's text changed from a
passing test to a failing one" beats a JPEG on cost and usefulness both.

**The sprite is the privacy indicator.** Its eyes open and it turns toward your
window exactly when it is looking; it visibly cannot look while asleep. No other
product category can render surveillance state as character animation. This is a
load-bearing design rule, not a polish item — it is the reason a user trusts this
over a menu-bar app doing the identical thing.

**Permissions we own: sensing only.** Screen Recording, mic, capture cadence.
Zero consent UI for *acting* — the Harness owns that, and stacking a second
dialog on top teaches users to click through both. One denylist stays ours
regardless: password fields and excluded apps are never captured, even when the
Harness would be permitted.

**Memory.** Light, local, plaintext-inspectable, per-Character — familiar name,
recent Behavior IDs, mood scalar, last-seen. A few KB of JSON buys most of the
felt continuity without becoming a searchable record of everything the user did.
Functional Layer conversation history stays separate and session-scoped.

---

## 7. Release staging

**v1 — charm.** Sprite walks, falls, Perches, gets Thrown, reacts. Director
produces varied idle life. Two built-in Characters. No chat, no Harness, no
permissions, no API key, no subscription. Shippable and screenshot-able, and
genuinely novel — nothing open-source has Perch physics plus a Director.

**v1.1 — chat.** Hotkey PTT, transcription, chat surface, Harness attached for
conversation only. No actions.

**v2 — actions.** MCP server exposed, Ambient sensing, actions through a
configured Harness. A first-party Rust Executor (`CGEvent` + `ScreenCaptureKit`,
~300 lines) only if the permission-hygiene argument wins.

**Displays.** One buddy, one screen at a time. Follows across Spaces. Stays put
across monitors unless dragged or its display disconnects — handle the
disconnect explicitly; a sprite stranded on coordinates that no longer exist is
otherwise the first bug report.

---

## 8. Prior art

| Project | License | Stack | What we take | What it lacks |
|---|---|---|---|---|
| [Shimeji-ee](https://kilkakon.com/shimeji/) | New BSD | Java/Swing, Windows-first | Its XML behavior model as the best existing formalisation of "what can a mascot do" — and the lesson that fully character-owned graphs go unused | Wrong stack; no AI; macOS only via forks |
| [WindowPet](https://github.com/SeakMengs/WindowPet) | MIT | Tauri + React | Per-pixel click-through hit-testing, tray, autostart, updater | No physics, no window awareness, no AI |
| [desktop-homunculus](https://github.com/not-elm/desktop-homunculus) | MIT/Apache-2.0 | Bevy, 3D VRM | Proof the MCP-server-for-mascot-control pattern works | 3D VRM not pixel art; early alpha; character is a puppet, not alive |
| [UI-TARS-desktop](https://github.com/bytedance/UI-TARS-desktop) | Apache-2.0 | Electron, vision-only | Evidence that a model-agnostic OSS Executor exists | Not a companion |

**Differentiators, stated deliberately:** 2D pixel-art nostalgia rather than 3D
anime; a Director that gives the Character *its own life* rather than a puppet an
agent poses; Perch physics, which none of them have.

---

## 9. Open risks

**Asset generation is the top risk to the character library.** Pixel art is the
cheapest format to *store* and the hardest to *generate*: image models produce
pixel-art-*styled* images at high resolution with anti-aliased edges and drifting
palettes, not grid-aligned sprites — and consistency of one character across the
6–8 frames of a walk cycle is the actual hard problem, not the individual frame.
Mitigation to test early, before committing to a library size: generate one
high-res reference sheet per Character, then downscale with a scripted
nearest-neighbour + fixed-palette quantisation pass so every frame lands on the
same grid and palette. Prove the pipeline on one Character before authoring ten.

**Wayland breaks the Spatial Layer.** Under Wayland a client cannot query other
windows' geometry or position itself at absolute screen coordinates, by design.
X11 does both. "Linux support" therefore means X11-only with the Spatial Layer
degraded or absent on Wayland — so window awareness must be an **optional
capability the platform declares**, not an assumption baked into the physics.

**Prompt injection reaches a machine with hands.** Captures ingest whatever is
on screen; Character Packages carry model-facing prose. Both are untrusted
input, and the Functional Layer has an Executor downstream. The mitigations are
already structural — Characters cannot declare capability, the Harness owns
action confirmation, the Local Gate strips before escalation — but this stays a
standing risk rather than a solved problem.

**Claude Code's native computer use is a fast path, not a dependency.** It is a
research preview requiring a Pro or Max plan. Because desktop control is
satisfiable by any of several OSS desktop-control MCP servers, no Anthropic
subscription is on the critical path — verify this stays true as the MCP server
landscape churns.
