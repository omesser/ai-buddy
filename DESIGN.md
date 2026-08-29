# ai-buddy — design

A desktop companion in the spirit of Windows 95-era desktop mascots, with a
model behind it. An animated sprite lives on your screen, obeys physics, perches
on your windows, and has a personality. When you summon it, it reaches an agent
Harness you supply and does real work on your machine.

This document records what was decided, why, and what was rejected. Vocabulary
is defined in [CONTEXT.md](./CONTEXT.md) and used precisely here.

## Shape of the product

Two layers, deliberately separate.

The **Spatial Layer** is always on, entirely local, and contains no model. It
owns physics, window geometry, Behaviors, and the interaction verbs. It works
offline, with no permissions granted, no API key, and no Harness attached. This
is the layer that has to be worth having on screen when everything else is off.

The **Functional Layer** is invoked, asynchronous, and does the real work. It is
reached by Summoning the buddy. It performs actions through an external Harness
the user attaches. ai-buddy never bundles one.

The **Director** sits between them. It is an occasional model call that observes
context and proposes a Behavior. It never runs in the frame loop and never
drives animation directly.

```
┌─────────────────────────────────────────────────────────────┐
│  ai-buddy (Tauri)                                           │
│                                                             │
│  ┌───────────────────────┐   ┌───────────────────────────┐  │
│  │ Spatial Layer (Rust)  │   │ Webview (sprite render)   │  │
│  │ • physics @ 60fps     │──▶│ • PNG frames, pixelated   │  │
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
│  │ recall / remember     │   │                           │  │
│  └──────────┬────────────┘   └───────────────────────────┘  │
└─────────────┼───────────────────────────────────────────────┘
              │ MCP
     ┌────────▼─────────┐        ┌──────────────────────────┐
     │ Harness (BYO)    │───────▶│ Executor (theirs)        │
     │ Claude Code /    │        │ native computer use, or  │
     │ Codex / any      │        │ desktop-control MCP srv  │
     └──────────────────┘        └──────────────────────────┘
```

## Decisions

### 1. Nostalgia first, capability second

The character has to be delightful before it is useful. Idle life, animation,
and presence come first; developer and productivity abilities are a second
layer built on top.

The reasoning: agentic desktop control is becoming commodity infrastructure, and
the labs will ship it natively. A companion worth keeping on screen is the
durable part.

### 2. Spatial before functional

Physical presence — reacting to windows, obeying gravity, being grabbed and
thrown — ships before any ability to operate the machine. The spatial layer is
read-only with respect to the system and needs no permissions.

### 3. Cross-platform architecture, macOS first

macOS is the first implemented platform because it is the development machine.
Windows is stubbed behind the same platform interface and implemented later.

**Linux is not one platform.** Under Wayland a client cannot query other
windows' geometry, cannot position itself at absolute screen coordinates, and
cannot reliably pin itself over the desktop. X11 supports all of it. The spatial
layer is therefore an *optional capability the platform declares*, not an
assumption. On Wayland the buddy degrades rather than fails.

### 4. Tauri, greenfield

Rust core with a webview front end. The sprite is 2D animation a webview handles
trivially. The hard parts — window enumeration, global input, tray, platform
capability detection — are Rust-side code that has to be written regardless of
stack, and Tauri puts them where they belong.

Rejected:

- **Electron** — same webview model, but roughly 150MB of binary and 100–200MB
  resident for a program whose pitch is "always there, costs you nothing." That
  number becomes a permanent argument.
- **Native per platform** — best behavior and footprint, three codebases.
  Contradicts the cross-platform requirement.
- **Godot** — genuinely good at sprite animation and state machines, but an odd
  foundation for the Functional Layer, and the chat surface fights the engine.
- **Forking [WindowPet](https://github.com/SeakMengs/WindowPet)** (MIT,
  Tauri + React, Windows/macOS/Linux) — it already solves click-through,
  pixel-perfect drag, tray, autostart, and updates, and has no physics, no
  window awareness, and no model. Rejected because the novel work replaces its
  central loop, and gutting the centre of a codebase is slower than starting
  clean. Its click-through hit-testing and tray/updater code are lifted
  directly under MIT, with attribution.

**Known cost:** in both Tauri and Electron, mouse click-through is per-window,
not per-pixel. A small sprite in a large transparent window swallows clicks
across the whole rectangle unless the cursor is tracked and ignore-mouse-events
toggled by hit-testing the sprite's alpha. This is a day of work, not an
afternoon. WindowPet's implementation is the reference.

### 5. The Director proposes; it never animates

The model wakes occasionally — on a timer, or on a notable event such as a new
frontmost app or a long idle — and emits a short Behavior for the local engine
to play cheaply. It is never in the frame loop.

Rejected:

- **Prompt-at-authoring only** (character prompt compiles to static weights, no
  runtime model) — kept as the configurable fallback, so the buddy still has a
  life when offline or when no key is present.
- **Model in the loop** — paying tokens for a cartoon to decide to scratch
  itself. Unusable battery, cost, and latency.

This is the decision that keeps the character visibly alive while the Functional
Layer is thinking, which is exactly where a naive design looks broken.

### 6. Characters are packages; the engine owns the vocabulary

A Character Package contains animations, a Character Manifest, a Personality Prompt, and
Behavior declarations. The format is first-class from day one, with two shipped
Characters, and it stays internal and undocumented until v2.

The engine owns the **Primitives** — the State machine and the units of motion
and expression. No Character can invent one. A Character declares **Behaviors**
as data: named sequences of Primitives with weights and trigger conditions.
Declarative, validatable, not Turing-complete, and diffable.

Rejected:

- **Built-in character enum** — retrofitting a package boundary onto hardcoded
  characters is a rewrite.
- **Character-owned behavior graphs** — this is what
  [Shimeji-ee](https://kilkakon.com/shimeji/) does with per-character XML (New
  BSD, Java, Windows-first). After fifteen years the overwhelming majority of
  community packages are art reskins of the default XML, because the graph was
  too hard to author. The engine-primitives split keeps the distinctiveness
  without owning a scripting language, and is the only version where an
  AI-generated Character Package is safe to load.

A Personality Prompt governs demeanour, never capability. Character Packages are
untrusted input to a model that can reach an agent Harness; prompt injection
through a package is not theoretical.

**Required Animation Set: 8** — `idle`, `walk`, `fall`, `land`, `sit`, `sleep`,
`react`, `talk`. A declared optional set is used when present. A Character with
8 animations must work; one with 30 should look better. Eight keeps a hobbyist
package to an evening's drawing.

**Shipped Characters: two, one faithful Win95 (16-color, hard pixels,
dithering), one modern pixel art.** Two styles validate the package abstraction
against real variance before the format is published.

The package on disk, as a directory or the same tree inside an archive:

```
mochi/
├── character.manifest    # name, per-Animation frames, fps, loop mode, Behaviors
├── personality.txt       # Personality Prompt — demeanour only, never capability
└── frames/
    ├── idle-0.png        # one PNG per frame, named by the manifest
    ├── idle-1.png
    ├── walk-0.png
    └── ...
```

Frame size and frame count are read from the art rather than declared: a
declared size can disagree with the art, and a derived one cannot. The manifest
is one `key = value` declaration per line and rejects every key it does not
know, which is what stops a package from declaring itself a capability.

### 7. Physics, Perches, and five verbs

The buddy obeys gravity. Grab it, fling it, it arcs and lands. This is the
novelty, and it is roughly an integrator plus collision against a rect list that
is already being polled.

**Window collision is top edges only.** Each visible window's top edge is a
one-way **Perch**: land on it, walk along it, fall off the ends, pass up through
it. Sides and bottoms are ignored entirely. That is most of the perceived
aliveness for a fraction of the collision work, and it avoids the bad cases —
sprite trapped inside an occluded window, jitter where windows overlap.

The verb set is fixed at five: **Grab**, **Throw**, **Poke**, **Menu**,
**Summon**. Every verb is a tax on every Character that will ever exist, so
additions wait for v2.

### 8. Always-on-top, single z-level, aggressive hiding

One window level. On macOS: a non-activating panel at floating level, joining
all Spaces, stationary, never taking focus, never in the app switcher.

"Sits on your window" needs always-on-top. "Hides behind your window" needs
desktop level. One window cannot be both, and restacking dynamically by sprite
state produces flicker on every platform. Peeking out from behind windows is
given up deliberately.

The investment goes into **hide rules** instead: fade out when a fullscreen app
or screen share is frontmost, under Do Not Disturb, and on a hotkey. A companion
that knows when to disappear is the difference between a pet and malware.

### 9. Sensing: no permissions until they buy something

**First run grants nothing.** Window awareness uses
`CGWindowListCopyWindowInfo` polling at ~10Hz, which returns window bounds,
owner app, and layer with no permission prompt. Smoothness comes from
interpolating in the render layer, not from event fidelity. Window *titles*
require Screen Recording consent on macOS 10.15+, and sitting on a window's edge
does not need titles.

Accessibility becomes a deliberate upgrade tied to the Functional Layer, where
the user understands the trade.

Beyond that, two consented modes:

- **Ambient Capture** — configurable periodic sampling plus capture on
  interaction. Needs Screen Recording.
- **On-Demand Capture** — a single capture in direct response to a user act.

**Every Capture passes a mandatory Local Gate.** On-device processing first —
perceptual hashing for meaningful change, on-device OCR via the macOS Vision
framework — and only a changed, interesting frame escalates to the Director.
Most ticks cost nothing and never leave the machine. At one capture per minute
that is roughly 1,440 frames a day; sending each to a cloud vision model is
untenable on cost and battery alike. The Gate also produces *better* Director
input than raw images: "the frontmost window's text went from a passing test to
a failing one" beats a JPEG.

The sampling interval is user-configurable. It controls sampling, not spend.

**The sprite is the privacy indicator.** Its eyes open, or it turns toward your
window, exactly when it is looking, and it visibly cannot look while asleep. No
other product category can render surveillance state as character animation.
This is a load-bearing rule, not polish.

### 10. No Executor

ai-buddy does not post synthetic mouse or keyboard events. It ships an **MCP
server** exposing buddy-side tools — speak, play a Behavior, list windows,
describe the screen, read and write Memory — and attaches a user-configured
Harness. Clicking is the Harness's job. The character is ai-buddy's.

The verification behind this:

- At the API level, Anthropic's
  [computer use tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
  is reasoning only. The model returns actions; the client executes them. The
  [reference implementation](https://github.com/anthropics/anthropic-quickstarts/blob/main/computer-use-demo/README.md)
  is a Docker/Linux container driving X11 with `xdotool`. Embedding an SDK means
  writing the executor.
- At the product level this changed on
  [23–24 March 2026](https://claude.com/blog/dispatch-and-computer-use): Claude
  Code and Claude Cowork do computer use natively on macOS, Windows following
  about ten days later. The Harness genuinely brings its own executor.

Consequences accepted:

- The capability is a research preview gated behind a Pro or Max subscription.
- Not portable across Harnesses. Other vendors follow the API pattern — actions
  out, client executes — so "BYO Harness" does not imply "any Harness can drive
  the desktop." A Harness without an executor can still chat and sense.
- Permissions belong to the Harness, which runs its own consent dialogs.

A `CGEvent` executor stays on the shelf as the answer if the subscription gate
proves fatal. It is not built on spec.

Rejected:

- **Spawn Claude Code as a subprocess** — fastest demo, wrong foundation. It is
  a coding agent in a costume, and ai-buddy would learn what happened by parsing
  stream output.
- **Provider abstraction layer** — MCP already is that layer.

One first-party adapter ships so the out-of-box experience is not "install a
harness first."

### 11. Permission surface: as small as possible

ai-buddy owns consent for **sensing only** — Screen Recording, microphone,
capture cadence. It owns **no** consent for acting, and does not duplicate the
Harness's confirmation prompts. Two dialogs for one click teaches users to click
through both.

Harness activity is surfaced in a visible Action Log. One denylist stays
ai-buddy's regardless of what the Harness permits: password fields and
explicitly excluded applications never enter a Capture.

No undo system. A real undo journal for arbitrary desktop actions is a research
project, and a fake one is worse than none.

### 12. Memory is one shared file the user owns

A single record of what the buddies know about the user, shared by every
Character Instance. Instances differ in personality and behavior, never in
knowledge. A second buddy knows your name on day one.

**One Markdown file**, append-structured under stable headings. Malformed
content is still valid Markdown, so a bad hand-edit degrades rather than breaks.
Headings are advisory and never parsed for correctness. It is the format the
model writes best, which matters because the `remember` tool does the writing.

The user can read it, edit it in any external editor, and wipe it. A single
timestamped backup is kept before each wipe. The loader tolerates malformed
content rather than crashing, and treats the file as untrusted input — the user
can type anything into it and it reaches Harness prompts.

Memory reaches the Harness as **MCP tools** (`recall`, `remember`), not as
injected prompt text. Tools mean ai-buddy does not own relevance ranking, and
every read and write appears in a log the user can inspect.

Splitting per-Instance memory back out stays possible later. It is not built now.

### 13. Voice: nothing listens by default

- **Trigger** — global hotkey push-to-talk, plus click-to-chat. Wake word is an
  opt-in, and when enabled uses **on-device detection only** (openWakeWord,
  Porcupine). Nothing is transmitted until the name fires.
- **Transcription** — a trait with two implementations. On macOS 26+, Apple's
  `SpeechAnalyzer` / `SpeechTranscriber`: on-device, no model download, no
  binary bloat, and benchmarked around twice as fast as Whisper Large V3 Turbo.
  Everywhere else — Windows, Linux, older macOS — `whisper.cpp` via
  [`whisper-rs`](https://github.com/tazz4843/whisper-rs).

An always-listening microphone in a desktop pet is the fastest available route
to being called spyware.

### 14. Multi-monitor: one coordinate space

The overlay is sized to the union of visible display frames, and physics runs in
a single coordinate space. Reconciling per-display windows is considerably
harder.

Two real problems to budget for: differing backing scale factors between
displays, and gaps between non-aligned displays. Clamp to the union of visible
frames, not to the bounding rectangle, so the sprite cannot walk into dead
space.

## v1 scope

**In:**

- Spatial Layer: Tauri overlay, physics, Perches, five verbs, hide rules,
  multi-monitor, tray
- Character Package format, engine Primitives, declarative Behaviors
- Two shipped Characters, 8 required animations each
- Director on the free sensing tier — frontmost app name, idle duration, time of
  day, recent Behaviors, Personality Prompt. No permissions required.
- Static-weights fallback when no model is configured
- MCP server and Harness attach
- Chat surface
- Memory

**Deferred:**

- Voice: hotkey push-to-talk, transcription, wake word
- Ambient and On-Demand Capture, the Local Gate, Screen Recording consent
- Windows implementation
- Published Character Package format and authoring documentation

**Explicitly not planned:** ambient screenshots without a Local Gate, an
ai-buddy Executor, an undo system, a provider abstraction layer, per-Instance
memory.

With nothing configured, ai-buddy is a complete product: spatial layer, physics,
Director, ambient reactions, and a nudge to connect a Harness. No API key, no
subscription, no permission prompts. That state is the default demo.

## Prior art

- **[Shimeji-ee](https://kilkakon.com/shimeji/)** — New BSD, Java/Swing,
  Windows-first, macOS via patched forks. Originally Shimeji by Yuki Yamada,
  Group Finity, 2009, zlib/libpng. Per-character XML behavior graphs. Read for
  its formalisation of what a desktop mascot can do; rejected as a foundation.
- **[WindowPet](https://github.com/SeakMengs/WindowPet)** — MIT, Tauri + React,
  three platforms, 45+ pets, custom pets, pixel-perfect drag, click-through,
  above-taskbar placement. No physics, no window awareness, no model. The
  reference implementation for the overlay mechanics.
- **[desktop-homunculus](https://github.com/not-elm/desktop-homunculus)** —
  MIT/Apache, Bevy, 3D VRM characters, MOD system with a TypeScript SDK, and a
  built-in MCP server for driving characters from Claude Code or Codex. macOS
  supported, Linux planned, early alpha. The closest existing thing to this
  idea.
- **[UI-TARS-desktop](https://github.com/bytedance/UI-TARS-desktop)** — Apache-2.0,
  Electron, vision-only. Evidence that a model-agnostic open-source Executor
  exists, if the Harness ever stops bringing one. Not a companion.

Differentiators, stated deliberately: 2D pixel-art nostalgia rather than 3D VRM;
a Director that gives the character its own life rather than a puppet an agent
poses; window-edge physics, which neither project has.

## Open risks

- **Click-through hit-testing** is the first thing that can look broken. Solve
  it before anything else in the overlay.
- **Director quality** is unproven. A model that proposes dull or repetitive
  Behaviors makes the whole thesis feel worse than static weights. Recent
  Behavior IDs feed back in to suppress repeats; measure this early.
- **The Pro/Max gate** on Harness computer use limits who can use the Functional
  Layer at all.
- **Wayland** degrades the spatial layer to nearly nothing.
- **Prompt injection** reaches a model with Harness access through three paths:
  Character Packages, the Memory file, and captured screen content.
- **Asset generation is the top risk to the character library.** Pixel art is
  the cheapest format to store and the hardest to generate: image models produce
  pixel-art-*styled* images at high resolution, with anti-aliased edges and
  drifting palettes, rather than grid-aligned sprites. Consistency of one
  character across the six to eight frames of a walk cycle is the hard part, not
  any single frame. Test the mitigation before committing to a library size —
  one high-resolution reference sheet per Character, downscaled by a scripted
  nearest-neighbour and fixed-palette pass so every frame lands on the same grid
  — and prove it on one Character before authoring ten.
- **A disconnected display** strands the sprite on coordinates that no longer
  exist. Handle it explicitly; it is otherwise the first bug report.

## Decision index

The numbering is the original decision log's. The sections above group and
rewrite those decisions, so the numbers do not line up with them; the index is
kept because it is the only route from a numbered decision to the ADR that
records it.

| # | Decision | Choice |
|---|---|---|
| 1 | Centre of gravity | Nostalgia companion first; productivity as a second layer |
| 2 | Meaning of "interact with screen" | Spatial first (geometry, Perches); functional second (Summoned) |
| 3 | Platforms | Cross-platform architecture, macOS first, Windows stubbed |
| 4 | Harness | BYO via MCP — see decision 17 |
| 5 | Runtime | Tauri (Rust + webview) — [ADR-0001](./docs/adr/0001-greenfield-tauri-not-fork-windowpet.md) |
| 6 | Model's role in idle | Director proposes Behaviors occasionally — [ADR-0004](./docs/adr/0004-director-outside-frame-loop.md) |
| 7 | Character | First-class package format, with pre-built Characters shipped |
| 8 | macOS window awareness | `CGWindowListCopyWindowInfo` polling @10Hz, no permissions |
| 9 / 14 | Behavior ownership | Engine-owned Primitives, Character-declared Behaviors — [ADR-0002](./docs/adr/0002-engine-owns-primitives-characters-declare-behaviors.md) |
| 10 | Physics and verbs | Gravity + Throw; Perch = window top edges only; five verbs, capped |
| 11 | Z-order | Always-on-top, non-activating, `canJoinAllSpaces`; aggressive auto-hide |
| 12 / 16 | Sensing | Ambient titles + configurable periodic capture + On-Demand — [ADR-0005](./docs/adr/0005-sensing-posture.md) |
| 13 | Codebase origin | Greenfield; WindowPet (MIT) as reference — [ADR-0001](./docs/adr/0001-greenfield-tauri-not-fork-windowpet.md) |
| 15 | Voice | Hotkey PTT + click-to-chat; wake word opt-in, on-device detection only |
| 15b | Transcription | Trait: Apple `SpeechAnalyzer` on macOS 26+, `whisper.cpp` elsewhere |
| 17 / 22 | Computer use | MCP server + MCP host; no first-party Executor — [ADR-0003](./docs/adr/0003-no-executor-harness-owns-desktop-control.md) |
| 18 | Capture processing | Mandatory Local Gate; only changed and interesting frames escalate |
| 19 | Permissions we own | Sensing only. Never duplicate the Harness's action prompts |
| 20 | Memory | One shared plaintext Markdown file the user owns; chat history session-scoped |
| 21 | No Harness attached | Fully charming — full Spatial Layer, chat shows a connect nudge |
| 23 | Art | True pixel art, integer nearest-neighbour — [ADR-0006](./docs/adr/0006-pixel-art-integer-scaling.md) |
| 24 | Displays | Overlay spans the union of display frames; the sprite stays put across monitors unless dragged |
| 25 | Release staging | v1 is charm, chat, Memory, and Harness attach; voice and Ambient Capture deferred |
