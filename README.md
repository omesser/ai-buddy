# ai-buddy

A desktop mascot that lives on your screen — and acts in character.

Pick a Character with an authored personality. The Director chooses idle Behaviors and short spoken lines to match. It also perches on windows, reacts to gestures, and stays out of your way while you work.

<p align="center">
  <img src="./branding/logo-art/logo-512.png" width="200" alt="Buddy Bot" />
</p>

[![CI](https://github.com/omesser/ai-buddy/actions/workflows/tests.yml/badge.svg)](https://github.com/omesser/ai-buddy/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

![Buddy Bot walk](./docs/readme/buddy-bot-walk.gif)

## What It Does

- **Personality-driven AI.** Each Character ships with a `personality.txt`. The Director uses it to pick idle Behaviors and short dialogue. Works offline with Static weights; optionally connect a Completer (API key or local model) for more variety.
- **Perches on windows.** Falls, lands on a window's top edge, rides a slow drag, drops when you yank or close the window.
- **Reacts to gestures.** Poke, pick up, throw — it arcs, lands, and keeps going.
- **Stays out of your way.** Fades for fullscreen, hides on Control-Option-Command-B. Appears in screenshots by default; opt-out available in settings.
- **Lives its own life.** Walks, idles, sits, sleeps — even with the Director off.

## See It

Try [Buddy Cues](https://omesser.github.io/ai-buddy/cues.html) — gestures and physics on a draggable sprite in the browser.

## Interact

![Buddy Bot react](./docs/readme/buddy-bot-react.gif)

- **Poke** — click once for a react, then it resumes.
- **Summon** — double-click to open a chat window for that buddy.
- **Pick up** — click and drag; it follows the cursor.
- **Throw** — release while moving; it flies on an arc and lands.
- **Perch** — let it settle on a window's top edge; drag slowly to ride, fling to drop.
- **Hide** — Control-Option-Command-B toggles the buddy instantly.
- **Fullscreen** — fades out for fullscreen apps, fades back when you exit.

### Talk to it

Summon opens a chat window belonging to that buddy. What you type is another
turn in the same conversation that decides what it does on your desktop, so an
answer arrives as speech in the bubble and as a Behavior it plays, not only as
text. Lines it says when nobody asked appear here too, labelled with what it was
reacting to.

<img src="./docs/readme/chat-surface.png" width="420" alt="The chat surface: a line labelled WHEN SUMMONED, a typed question, and BMO's answer, over a status bar naming the Behavior, State and next wake" />

The bar along the bottom names what the buddy is doing right now — the Behavior,
the Primitive under it, the Animation playing, its State, and how long until it
next thinks. It needs a Director; see [Running it](#running-it).

## Characters

Buddy Bot is the default. Eight Characters ship in the repo; each moves and speaks differently.

| Character | Description | Personality |
|---|---|---|
| <img src="./docs/readme/buddy-bot-walk.gif" height="96" alt="Buddy Bot" /><br>**Buddy Bot** | Logo mascot. Smooth 90×90 render. | Friendly, curious, treats the desk like a shared workspace. [full prompt](./characters/buddy-bot/personality.txt) |
| <img src="./docs/readme/black-mage-talk.gif" height="96" alt="Black Mage" /><br>**Black Mage** | FF1 Black Mage from 8-Bit Theater. Pixel art, scaled up for the desktop. | Cynical spellcaster. Cryptic, theatrical, more comfortable with incantations than conversation. [full prompt](./characters/black-mage/personality.txt) |
| <img src="./docs/readme/bmo-sing.gif" height="96" alt="BMO" /><br>**BMO** | Small games console (Shimeji shop pack). Soft drawn lines. | Earnest and childlike, delighted to be here, eager to help. [full prompt](./characters/bmo/personality.txt) |
| <img src="./docs/readme/cat-walk.gif" height="96" alt="Cat" /><br>**Cat** | Scottish Fold, chibi gray-and-white. | Treats every window as furniture. Busy, curious, never generic, never helpful. [full prompt](./characters/cat/personality.txt) |
| <img src="./docs/readme/jotaro-kujo-react.gif" height="96" alt="Jotaro Kujo" /><br>**Jotaro Kujo** | Chibi JoJo delinquent (petscodex import). | Terse, perpetually bored, tougher than his indifference suggests. [full prompt](./characters/jotaro-kujo/personality.txt) |
| <img src="./docs/readme/nim-sleep.gif" height="96" alt="Nim" /><br>**Nim** | Modern pixel art with a soft shadow. | Sleeps eleven hours a day. Soft-spoken, easily charmed, slow to arrive anywhere. [full prompt](./characters/nim/personality.txt) |
| **Timber Wolf** | BattleTech OmniMech (Sketchfab, Editorial). | Patrol mech. Desktop is a sector to secure, reports are brief. Clan warriors don't waste words. [full prompt](./characters/timber-wolf/personality.txt) |
| <img src="./docs/readme/trump-talk.gif" height="96" alt="Trump" /><br>**Trump** | Caricature in a navy suit and red tie. | The desktop is his rally. Bombastic, sure this is the greatest desktop in history. [full prompt](./characters/trump/personality.txt) |

Characters are packages of art, personality, and tuning. Packaging details live in [DEVELOPMENT.md](./docs/DEVELOPMENT.md); the on-disk format is still evolving.

## Install

Download a build from [GitHub Releases](https://github.com/omesser/ai-buddy/releases).

Or clone and run from the repo root (macOS, Linux, Windows):

```sh
git clone https://github.com/omesser/ai-buddy.git
cd ai-buddy
cargo run -p ai-buddy
```

### macOS

Apple Silicon. The Release ships a `.dmg`. Open it and copy `ai-buddy` to Applications.

The build is ad-hoc signed, not notarized, so Gatekeeper will warn on the first open. Double-click the app, dismiss the dialog, then System Settings → Privacy & Security → Open Anyway. Note the button is time-limited after the blocked launch. Notarization is a follow-up.

The same missing signature costs two Keychain dialogs at launch — "ai-buddy wants to use your confidential information stored in ai-buddy" — for anyone who saved a Director API key. An ad-hoc signature has no identity, so macOS records the app in the key's access list as a hash of that exact build, and the next release is a different hash and a stranger to its own key. Always Allow answers both, and holds until the next update replaces the hash. Exporting `AI_BUDDY_DIRECTOR_API_KEY` keeps the Keychain out of the launch entirely. A stable signing identity is what ends it ([#283](https://github.com/omesser/ai-buddy/issues/283)).

### Linux

The Release ships an AppImage and a `.deb` (x86_64).

Under Wayland the sprite keeps to screen edges and loses window Perches — a supported mode, not an error. X11 gets both.

```sh
# Debian/Ubuntu .deb
sudo apt install ./ai-buddy_*.deb
# or: AppImage (needs libfuse2 on Ubuntu 22.04, libfuse2t64 on 24.04+)
# sudo apt install libfuse2    # or libfuse2t64
# chmod +x ai-buddy_*.AppImage && ./ai-buddy_*.AppImage
```

Tray hosts, cue audio (GStreamer), and AppImage fuse notes: [DEVELOPMENT.md](./docs/DEVELOPMENT.md#linux-dependencies).

### Windows

The Release ships an NSIS installer (x86_64). Run it and follow the prompts.

SmartScreen may warn on the first open because the build is not Authenticode signed. Choose More info → Run anyway. Code signing is a follow-up.

## Running it

**Works offline.** With no API key, Static weights pick idle Behaviors from the Character. No model, no account required.

**Optional Completer.** Point Settings → Director (or env vars) at OpenAI, Anthropic, Ollama, or any OpenAI-compatible `/v1/chat/completions` endpoint:

```sh
# OpenAI (or export env vars to persist)
AI_BUDDY_DIRECTOR_API_KEY="$OPENAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.openai.com \
AI_BUDDY_DIRECTOR_MODEL=gpt-4o-mini \
cargo run -p ai-buddy

# Ollama (local, no key)
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
cargo run -p ai-buddy
```

**Optional Harness.** An agent you already run answers instead, over ACP, and signs in on its own:

```sh
AI_BUDDY_HARNESS=claude cargo run -p ai-buddy   # names and standing below
```

**Switch characters** (env or Settings):

```sh
# Any of: buddy-bot (default), black-mage, bmo, cat, jotaro-kujo, nim, timber-wolf, trump
AI_BUDDY_CHARACTER=nim cargo run -p ai-buddy
```

See [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for provider details, Director env vars, local model servers, keyring/secret store, and `probe-model.sh`.

## Harness Support

Which Harness you attach changes what ai-buddy can do with it.
We use `scripts/probe-harness.sh` to test and prove various behaviors.

| Harness | Command | Standing |
|---|---|---|
| <img src="https://cdn.simpleicons.org/claude" width="14" alt="" /> `claude` | `npx -y @agentclientprotocol/claude-agent-acp@latest` | Zed's adapter over the Claude Agent SDK; no first-party ACP mode. **Verified 2026-09-07**: `end_turn` on a fresh session and again on a resumed one. |
| <img src="https://cdn.simpleicons.org/hermes" width="14" alt="" /> `hermes` | `hermes acp` | First-party. **Verified 2026-09-07**: `end_turn` on a fresh session, and on a resumed one once a failed first turn reopens the session it had only said it loaded (#448). |
| <img src="https://cdn.simpleicons.org/opencode" width="14" alt="" /> `opencode` | `opencode acp` | First-party. **Verified 2026-09-09**: `end_turn` on a fresh session and again on a resumed one. |
| `grok` | `grok agent stdio` | First-party, Grok Build. `grok` alone is the interactive TUI, so the subcommand is the whole of the row. **Verified 2026-09-09**: `end_turn` on a fresh session and again on a resumed one (#457). |
| anything else | as typed, split on whitespace | Unnamed, unverified, and it works: any command that speaks ACP on stdio attaches. <img src="https://cdn.simpleicons.org/githubcopilot" width="14" alt="" /> GitHub Copilot CLI (`copilot --acp --stdio`) reaches ai-buddy this way today and earns a named row once a turn is smoked (#457). Google has none: the Gemini CLI is sunset, and Antigravity speaks its own protocol rather than ACP (#603), so it needs an adapter (#604). |

How they handle session differs, and changes what ai-buddy can do with them:

| Harness | Fresh session | Resumed session | `loadSession` | MCP transport † | Auth methods ‡ |
|---|---|---|---|---|---|
| `claude` | yes | yes | yes | http | none advertised when signed in |
| `hermes` | yes | yes, after the reopen | yes | stdio | two: custom runtime credentials, Configure Hermes provider |
| `opencode` | yes | yes, same session id | yes | http | Login with opencode |
| `grok` | yes | yes, same session id | yes | http | three: xai.api_key, cached_token, Grok |
| anything else | unverified | unverified | unverified | unverified | unverified |

- † What `initialize` advertised: `claude`, `opencode` and `grok` set `agentCapabilities.mcpCapabilities.http` (the running app hands the loopback URL); `hermes` omits it and gets the stdio binary that relays to the same endpoint (ADR-0023, ADR-0026). A probe does not bind that listener, so it still hands `opencode` and `grok` the `--mcp-stdio` shim. `opencode` and `grok` also advertise `sse`, which nothing here reads.
- ‡ `authMethods` is what is *available*, not what is outstanding — an empty list is no proof a login is unnecessary. Only `session/new` answering `-32000` is (ADR-0022).

### Harness ↔ MCP

Two transports, two distinct axes:

1. **ACP** (ai-buddy ↔ Harness): always stdio. How ai-buddy attaches to the Harness and prompts it.
2. **MCP** (Harness → ai-buddy tools): How the Harness calls back so `speak` and sensing reach the buddy.

**MCP transport** is gated by what the Harness advertises in ACP `initialize` → `agentCapabilities.mcpCapabilities.http`:
- **true** → loopback HTTP URL + bearer token (ADR-0023, #491)
- **omitted/false** → stdio MCP server entry; shim relays to same loopback endpoint (ADR-0026, #501)

**Seven tools** from `crates/core/src/dispatch.rs`:

| Tool | Category | What it does |
|---|---|---|
| `speak` | Expression | Make the Character speak dialogue |
| `play_behavior` | Expression | Play a named Behavior |
| `list_windows` | Sensing | List visible windows with bounds and owner |
| `describe_screen` | Sensing | Describe screen (v1: window metadata only) |
| `recall` | Memory | Read everything Memory holds |
| `remember` | Memory | Write one fact under a heading |
| `list_instances` | Identity | List Character Instances and their names |

**Explicitly not served:** mouse/keyboard/Executor tools (ADR-0003). No click, no type, no input events by design.

## Platform Support

What works today on each OS. Degraded and stub mean reduced or no-op — supported honesty, not a crash.

| Capability | macOS | Linux | Windows |
|---|---|---|---|
| Overlay that never takes focus | yes | yes † | yes |
| Click-through off the sprite | yes | yes † | yes |
| Grab, Throw and Poke | yes | yes † | yes |
| Perch on window edges | yes | yes † | yes |
| Dock or panel as a Perch | yes | degraded | degraded |
| Fade out for a fullscreen app | yes | yes † | degraded |
| Capturable; opt-out in settings | yes | degraded | yes |
| Native settings window | yes | yes † | in progress |

- `yes` — implemented.
- `degraded` — runs in reduced form. A supported mode, not an error.
- `in progress` — foundation in place, iteration ongoing.
- `†` — needs an X server (usually XWayland). See [DEVELOPMENT.md](./docs/DEVELOPMENT.md).

## Developing

**Want to help?** [Open issues](https://github.com/omesser/ai-buddy/issues) welcome bugs, ideas, and PRs. Start with [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for toolchains, hooks, verification, character writing, and imports. See how ai-buddy compares to other desktop pets in [alternatives.md](./docs/research/alternatives.md).

**Design and decisions:**
- [CONTEXT.md](./CONTEXT.md) — vocabulary
- [DESIGN.md](./DESIGN.md) — design decisions (the chat window ships; the [chat mockups](https://omesser.github.io/ai-buddy/chat-mockups.html) are a Dated page: a frozen proposal, not what ships. [#17](https://github.com/omesser/ai-buddy/issues/17) tracks what is left)
- [docs/SPEC.md](./docs/SPEC.md) — v1 scope
- [docs/adr/](./docs/adr/) — ADRs

## Prior Art and Attribution

[WindowPet](https://github.com/SeakMengs/WindowPet) (MIT) inspired the Tauri desktop-pet shape. ai-buddy is a greenfield build, not a fork ([ADR-0001](./docs/adr/0001-greenfield-tauri-not-fork-windowpet.md)). Overlay code is independent; tray, launch-at-login, and updater follow WindowPet's MIT-licensed patterns.

The Chat window's mind mark — the small brain beside what answers — is the
`brain` glyph from [Font Awesome Free](https://fontawesome.com/) 6.x, used
under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) and inlined as
a path in `src/chat.html`. The licence asks for the credit; this is it.

Character provenance is in each Character Package manifest, under `[source]`, and on the [Character Gallery](https://omesser.github.io/ai-buddy/characters.html). A package is prose, a manifest and art: the personality and the manifest — animations, Behaviors, Director and cursor tuning — are this project's work and MIT throughout. The art is not always ours. Some characters adapt art that declares no license, and each manifest names what it adapts and whose IP the character is.

## License

MIT.
