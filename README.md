# Desktop Buddy

A personality-driven AI desktop companion. Choose a character — each has its own authored personality — and the Director picks idle behaviors and spoken lines to match. No chat window required. The sprite also perches on windows, responds to gestures, and stays out of your way.

<p align="center">
  <img src="./branding/logo-art/logo-512.png" width="200" alt="Buddy Bot" />
</p>

[![CI](https://github.com/omesser/ai-buddy/actions/workflows/tests.yml/badge.svg)](https://github.com/omesser/ai-buddy/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

![Buddy Bot walk](./docs/readme/buddy-bot-walk.gif)

## What It Does

- **Personality-driven behavior.** Each character has an authored `personality.txt`. The Director reads it and picks idle Behaviors + short dialogue that fit — no chat window, no prompting required. Static weights when offline; connect a Completer for model-driven variety.
- **Perches on windows.** Falls, lands on window edges, rides them when dragged slowly, drops when yanked or closed.
- **Reacts to gestures.** Click to poke, drag to pick up, fling to throw. It arcs, lands, and keeps going.
- **Stays out of your way.** Fades when you go fullscreen, hides instantly on Control-Option-Command-B, never appears in screen captures or shares.
- **Lives its own life.** Walks, idles, sits, sleeps — animated behaviors run constantly.

## See It

**Try the interactive demo:**
- [Buddy Cues](https://omesser.github.io/ai-buddy/cues.html) — Gestures and physics on a draggable sprite

## Interact

![Buddy Bot react](./docs/readme/buddy-bot-react.gif)

- **Poke:** Click once. It plays its `react` animation, then goes back to what it was doing.
- **Pick up:** Click and drag. It follows your cursor.
- **Throw:** Drag and release while moving. It leaves your hand on an arc and lands.
- **Perch:** Let it settle on a window's top edge. Drag that window slowly and it rides along. Fling the window and it drops.
- **Hide hotkey:** Control-Option-Command-B. Instant. Press again to bring it back.
- **Fullscreen:** The sprite fades out when any application goes fullscreen, fades back in when you exit.

## Characters

Buddy Bot is the default. Eight characters ship in-repo; each moves and behaves differently.

| Character | Description | Personality |
|---|---|---|
| <img src="./docs/readme/buddy-bot-walk.gif" height="96" alt="Buddy Bot" /><br>**Buddy Bot** | Logo mascot. Grok Imagine–generated art at 90×90, smooth render. | Friendly, curious, treats the desk like a shared workspace. [full prompt](./characters/buddy-bot/personality.txt) |
| <img src="./docs/readme/black-mage-talk.gif" height="96" alt="Black Mage" /><br>**Black Mage** | FF1 Black Mage from 8-Bit Theater. Pixel art at 3x scale for desktop readability. | Cynical spellcaster. Cryptic, theatrical, more comfortable with incantations than conversation. [full prompt](./characters/black-mage/personality.txt) |
| <img src="./docs/readme/bmo-sing.gif" height="96" alt="BMO" /><br>**BMO** | Small games console from shimejishop free pack. Drawn art, soft lines, scale 1. | Earnest and childlike, delighted to be here, eager to help. [full prompt](./characters/bmo/personality.txt) |
| <img src="./docs/readme/cat-walk.gif" height="96" alt="Cat" /><br>**Cat** | Scottish Fold imported from petscodex. Chibi gray-and-white style. | Treats every window as furniture. Busy, curious, never generic, never helpful. [full prompt](./characters/cat/personality.txt) |
| <img src="./docs/readme/jotaro-kujo-react.gif" height="96" alt="Jotaro Kujo" /><br>**Jotaro Kujo** | 17-year-old delinquent imported from petscodex. Chibi style. | Terse, perpetually bored, tougher than his indifference suggests. [full prompt](./characters/jotaro-kujo/personality.txt) |
| <img src="./docs/readme/nim-sleep.gif" height="96" alt="Nim" /><br>**Nim** | Modern pixel art with translucent shadow. Twice the frames, motion eases. | Sleeps eleven hours a day. Soft-spoken, easily charmed, slow to arrive anywhere. [full prompt](./characters/nim/personality.txt) |
| <img src="./docs/readme/timber-wolf-scan.gif" height="96" alt="Timber Wolf" /><br>**Timber Wolf** | Clan OmniMech from BattleTech. Frame captures from Sketchfab 3D model (CC BY 4.0). | Patrol mech. Desktop is a sector to secure, reports are brief. Clan warriors don't waste words. [full prompt](./characters/timber-wolf/personality.txt) |
| <img src="./docs/readme/trump-talk.gif" height="96" alt="Trump" /><br>**Trump** | Caricature imported from petscodex. Navy suit, red tie. | The desktop is his rally. Bombastic, sure this is the greatest desktop in history. [full prompt](./characters/trump/personality.txt) |

Character Packages bundle identity, art, personality, and tuning. The format is first-class but undocumented until v2.

## Install

Download a build from [GitHub Releases](https://github.com/omesser/ai-buddy/releases).

Or clone and run from the repo root (macOS, Linux, Windows):

```sh
git clone https://github.com/omesser/ai-buddy.git
cd ai-buddy
cargo run -p ai-buddy
```

Linux packages, toolchains, and hooks are under [Developing](#developing).

### macOS

Apple Silicon. The Release ships a `.dmg`. Open it and copy `ai-buddy` to Applications.

The build is ad-hoc signed, not notarized, so Gatekeeper will warn on the first open. Double-click the app, dismiss the dialog, then System Settings → Privacy & Security → Open Anyway. Note the button is time-limited after the blocked launch. Developer ID and notarization are a follow-up.

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

Some Windows platform cells are still `stub` or `degraded` — see [Platform Support](#platform-support). The installer and the shell binary are real; those cells are about overlay and sensing depth, not the package.

## Running it

**Works offline.** With no Director key, Static weights pick idle Behaviors from the Character's manifest. No model, no account, no permission required.

**Connect a Completer (optional).** The Director can reach OpenAI, Anthropic, Ollama, or other `/v1/chat/completions` providers. Set the API key, base URL, and model in Settings → Director, or via env vars for one run:

```sh
# OpenAI (or export env vars to persist)
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$OPENAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.openai.com \
AI_BUDDY_DIRECTOR_MODEL=gpt-4o-mini \
cargo run

# Ollama (local, no key)
cd src-tauri
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
cargo run
```

**Switch characters** (env or Settings):

```sh
cd src-tauri
# Any of: buddy-bot (default), black-mage, bmo, cat, jotaro-kujo, nim, timber-wolf, trump
AI_BUDDY_CHARACTER=nim cargo run
```

See [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for:
- Provider details (Anthropic, xAI, Ollama, oMLX, llama.cpp, LM Studio)
- Director env vars reference
- Local model server setup
- Keyring/secret store (macOS Keychain, Linux Secret Service)
- `probe-model.sh` for testing Completer connectivity

## Platform Support

What ships where, read from the platform seam rather than from intent. Honest about degraded and stub cells.

| Capability | macOS | Linux | Windows |
|---|---|---|---|
| Overlay that never takes focus | yes | yes † | degraded |
| Click-through off the sprite | yes | yes † | yes |
| Grab, Throw and Poke | yes | yes † | degraded |
| Perch on window edges | yes | yes † | degraded |
| Dock or panel as a Perch | yes | degraded | degraded |
| Fade out for a fullscreen app | yes | yes † | degraded |
| Never captured in a screen share | yes | degraded | stub |

- `yes` — implemented.
- `degraded` — runs in reduced form. A supported mode, not an error.
- `stub` — the arm compiles and does nothing. Windows only.
- `†` — needs an X server (usually XWayland). See [DEVELOPMENT.md](./docs/DEVELOPMENT.md).

## Developing

**Want to help?** [Open issues](https://github.com/omesser/ai-buddy/issues) track bugs, features, and research. Pull requests welcome — see [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for toolchains, hooks, and verification. [Alternatives comparison](./docs/research/alternatives.md) shows how Desktop Buddy differs from other desktop pets.

See [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for:
- Toolchains (Rust, Python, Node)
- Pre-commit hooks
- Verifying the overlay (unit tests, `verify-overlay.sh`, 23-step human checklist)
- Trace variables (HITTEST, FRAMES, DIRECTOR, ENGINE)
- Character writing instructions (personality structure, universal rules)
- Character Packages (search paths, AI_BUDDY_CHARACTERS env)
- Importing a pet from petscodex or Shimeji-ee
- Running several buddies at once

**Design and decisions:**
- [CONTEXT.md](./CONTEXT.md) — Vocabulary
- [DESIGN.md](./DESIGN.md) — Design decisions (includes [chat mockups](https://omesser.github.io/ai-buddy/chat-mockups.html) as speculative design — chat UI not shipped, [#17](https://github.com/omesser/ai-buddy/issues/17))
- [docs/SPEC.md](./docs/SPEC.md) — v1 scope and requirements
- [docs/adr/](./docs/adr/) — Architecture Decision Records

## Prior Art and Attribution

[WindowPet](https://github.com/SeakMengs/WindowPet) (MIT) is the reference for a Tauri desktop pet. ai-buddy is a greenfield build rather than a fork, for the reasons in [ADR-0001](./docs/adr/0001-greenfield-tauri-not-fork-windowpet.md). The overlay is an independent implementation. The tray, launch-at-login, and updater follow WindowPet's shape under MIT.

Character art provenance is documented in each Character Package manifest.

## License

MIT.
