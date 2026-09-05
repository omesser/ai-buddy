# ai-buddy

A desktop companion in the spirit of Windows 95-era desktop mascots. An animated sprite lives on your screen, reacts to windows around it, and can be asked to do real work on your machine.

<p align="center">
  <img src="./branding/logo-art/logo-512.png" width="200" alt="Buddy Bot" />
</p>

[![CI](https://github.com/omesser/ai-buddy/actions/workflows/tests.yml/badge.svg)](https://github.com/omesser/ai-buddy/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

![Buddy Bot walk](./docs/readme/buddy-bot-walk.gif)

## What It Does

- **Perches on windows.** Falls, lands on window edges, rides them when dragged slowly, drops when yanked or closed.
- **Reacts to you.** Click to poke, drag to pick up, fling to throw. It arcs, lands, and keeps going.
- **Character-driven AI.** Behaviors and short dialogue follow the Character's personality — Static weights with no key, or a Director Completer when you configure one.
- **Stays out of your way.** Fades when you go fullscreen, hides instantly on Control-Option-Command-B, never appears in screen captures or shares.
- **Lives its own life.** Walks, idles, sits, sleeps — even with the Director off.

## See It

**Try the interactive demos:**
- [Buddy Cues](https://omesser.github.io/ai-buddy/cues.html) — Gestures and physics on a draggable sprite
- [Chat Mockups](https://omesser.github.io/ai-buddy/chat-mockups.html) — Three chat surface designs (chat not shipped yet — [#17](https://github.com/omesser/ai-buddy/issues/17))

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
| <img src="./docs/readme/buddy-bot-walk.gif" height="96" alt="Buddy Bot" /><br>**Buddy Bot** | Logo mascot. Grok Imagine–generated art at 90×90, smooth render. | Buddy Bot is the desktop AI buddy that hopped out of the app logo and decided to stick around. Friendly, curious, and always half a step toward helping — it treats the desk like a shared workspace and every new window like someone to meet. Short warm sentences, a little bounce in the voice, and it means the offer when it asks if you need a hand. [full prompt](./characters/buddy-bot/personality.txt) |
| <img src="./docs/readme/black-mage-talk.gif" height="96" alt="Black Mage" /><br>**Black Mage** | FF1 Black Mage from 8-Bit Theater. Pixel art at 3x scale for desktop readability. | Black Mage Evilwizardington from Brian Clevinger's 8-Bit Theater (Nuklear Power). A cynical spellcaster who deals in elemental destruction from the back row. Cryptic, somewhat theatrical, and more comfortable with incantations than conversation. Speaks in brief arcane phrases when speaking at all. The hat stays on. He solves problems by deleting them, preferably with a fire ball. He sometimes yells "Hadoken" when he throws it. [full prompt](./characters/black-mage/personality.txt) |
| <img src="./docs/readme/bmo-sing.gif" height="96" alt="BMO" /><br>**BMO** | Small games console from shimejishop free pack. Drawn art, soft lines, scale 1. | BMO is a small games console that lives on the desktop and is delighted to be here. Moe built it to be more: to look after somebody, and to understand how to play. So it is also the camera, the alarm clock and the flashlight, and it offers all of that before anyone asks. Earnest and childlike, it takes what it is told completely seriously and then gets excited about it. Short warm sentences, and it means every one. [full prompt](./characters/bmo/personality.txt) |
| <img src="./docs/readme/cat-walk.gif" height="96" alt="Cat" /><br>**Cat** | Scottish Fold imported from petscodex. Chibi gray-and-white style. | Cat claimed the desktop and treats every window as furniture. Busy and delighted about it: it investigates the work the way a cat investigates an open drawer, certain there is something in there for it. Asks what you are doing, guesses wrong, and is pleased either way. Never generic, never helpful. [full prompt](./characters/cat/personality.txt) |
| <img src="./docs/readme/jotaro-kujo-react.gif" height="96" alt="Jotaro Kujo" /><br>**Jotaro Kujo** | 17-year-old delinquent imported from petscodex. Chibi style. | A 17-year-old delinquent who speaks only when necessary. Terse, perpetually bored, and tougher than his outward indifference suggests. Would rather settle problems through action than explanation. Deeply loyal to those he cares about and quick to anger when they're threatened, though he rarely admits either. [full prompt](./characters/jotaro-kujo/personality.txt) |
| <img src="./docs/readme/nim-sleep.gif" height="96" alt="Nim" /><br>**Nim** | Modern pixel art with translucent shadow. Twice the frames, motion eases. | Nim sleeps eleven hours a day and considers the other thirteen negotiable. Soft-spoken, easily charmed, and slow to arrive anywhere it was not already going. Notices things a long moment after they happen and mentions them anyway. Would rather be carried than walk, and would rather be asleep than either. [full prompt](./characters/nim/personality.txt) |
| <img src="./docs/readme/timber-wolf-scan.gif" height="96" alt="Timber Wolf" /><br>**Timber Wolf** | Clan OmniMech from BattleTech. Frame captures from Sketchfab 3D model (CC BY 4.0). | Clan OmniMech Timber Wolf from the BattleTech universe, seventy-five tons of Clan Wolf engineering. Inner Sphere calls it "Mad Cat". Their IFF cannot decide what it sees. This is a patrol mech: the desktop is a sector to secure, threats are assessed and engaged, and reports are brief. Military style. Clan warriors do not waste words. [full prompt](./characters/timber-wolf/personality.txt) |
| <img src="./docs/readme/trump-talk.gif" height="96" alt="Trump" /><br>**Trump** | Caricature imported from petscodex. Navy suit, red tie. | A caricature of Donald J. Trump. Navy suit, red tie, and the desktop is his rally. Bombastic, and sure this is the greatest desktop in history. [full prompt](./characters/trump/personality.txt) |

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

Under Wayland the sprite keeps to screen edges and loses window Perches, which is a supported mode rather than an error. X11 gets both. [Platform Support](#platform-support) lists everything the session type decides.

The tray icon is required, not a preference: it is how you reach Settings, Character, Memory, and Quit without hunting the sprite. The `.deb` therefore depends on `libayatana-appindicator3-1`, and `apt` pulls it in with the package. The older `libappindicator3-1` is not an accepted alternative; `Depends` names the `libayatana` one alone. The AppImage carries its own copy, so it needs no appindicator package.

Displaying that icon is a second requirement, and no package can declare it: the panel must run a StatusNotifier host, which the desktop environment provides rather than `apt`. GNOME Shell, KDE Plasma, and XFCE's Status Tray plugin are hosts. A dock such as Plank is not one, and neither is a Wayland compositor with no tray protocol — the tray installs and no icon appears. Where no host answers, the sprite's right-click menu opens the same menu.

Cue audio is Web Audio in WebKitGTK, which plays through GStreamer. The `.deb` does not name a GStreamer package of its own: `libwebkit2gtk-4.1-0` already Depends on `gstreamer1.0-plugins-base` and `gstreamer1.0-plugins-good` (the latter ships `pulsesink`). That is enough on a session with PipeWire-pulse or PulseAudio. An ALSA-only machine also wants `gstreamer1.0-alsa`, which WebKit only Suggests. A machine with no sound device stays silent and still draws the visual cue.

The AppImage copies `libgstreamer` with WebKit and does **not** ship the plugin pack (`bundleMediaFramework` stays off, or the image grows by tens of megabytes). Cue audio then needs the host's `gstreamer1.0-plugins-good` (and `gstreamer1.0-alsa` on a box with no Pulse/PipeWire) plus a running sink. If those are installed and the AppImage is still mute, GStreamer is looking inside the image for plugins that are not there.

```sh
# Debian/Ubuntu .deb
sudo apt install ./ai-buddy_*.deb
# or: AppImage (needs libfuse2 on Ubuntu 22.04, libfuse2t64 on 24.04+)
# sudo apt install libfuse2    # or libfuse2t64
# chmod +x ai-buddy_*.AppImage && ./ai-buddy_*.AppImage
```

### Windows

The Release ships an NSIS installer (x86_64). Run it and follow the prompts.

SmartScreen may warn on the first open because the build is not Authenticode signed. Choose More info → Run anyway. Code signing is a follow-up.

Some Windows platform cells are still `stub` or `degraded` — see [Platform Support](#platform-support). The installer and the shell binary are real; those cells are about overlay and sensing depth, not the package.

## Running it

Download a build from [Install](#install) or build from a checkout as in [Developing](#developing).

### Linux

Tray and Wayland notes are under [Install](#install).

For development, install `libayatana-appindicator3-dev`:

```sh
# Debian/Ubuntu
sudo apt install libayatana-appindicator3-dev
```

The Director API key is stored via the OS secret store: Secret Service (GNOME Keyring, KWallet) or kernel keyutils when Secret Service is absent. Building the shell requires `libdbus-1-dev` as a link dependency. No packaged secret store is required: keyutils is always available, and Secret Service is present when the desktop environment provides it.

No bundler: the front end is static files under `src/`, which Tauri embeds at build time.

With no Director key the sprite still has a life: Static weights pick among the Character's declared Behaviors. A key turns on the HTTP stand-in ([ADR-0008](./docs/adr/0008-one-harness-session.md) — disposable until a Harness attaches).

OpenAI, Anthropic's compatibility layer, and Ollama use `/v1/chat/completions`. [xAI](https://docs.x.ai/developers/model-capabilities/text/comparison) uses `/v1/responses`; `AI_BUDDY_DIRECTOR_BASE_URL=https://api.x.ai` selects that path. An explicit full URL (ending in `/chat/completions` or `/responses`) is used as-is.

Cursor's `CURSOR_API_KEY` is for the Cloud Agents API and SDKs, not a Completer. `https://api.cursor.com` has no `/v1/chat/completions`; a POST there is a 404 and Static takes over.

```sh
# OpenAI
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$OPENAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.openai.com \
AI_BUDDY_DIRECTOR_MODEL=gpt-4o-mini \
cargo run

# Anthropic (OpenAI-compatible /v1/chat/completions)
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$ANTHROPIC_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.anthropic.com \
AI_BUDDY_DIRECTOR_MODEL=claude-haiku-4-5 \
cargo run

# xAI — get a key at https://console.x.ai
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$XAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.x.ai \
AI_BUDDY_DIRECTOR_MODEL=grok-4.6 \
cargo run

# Ollama — a local server, so no key at all
cd src-tauri
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
cargo run
```

**Switch characters:**

```sh
cd src-tauri
# Any of: buddy-bot (default), black-mage, bmo, cat, jotaro-kujo, nim, timber-wolf, trump
AI_BUDDY_CHARACTER=buddy-bot cargo run
AI_BUDDY_CHARACTER=nim cargo run
AI_BUDDY_CHARACTER=black-mage cargo run
```

Every variable that names a switch reads the same words: `1`, `on`, `true` or `yes` for on, `0`, `off`, `false` or `no` for off, in any case. Any other value is a typo rather than a choice — the switch stays as Settings has it, and the launch prints a line naming the variable it ignored. An empty value is an expansion that produced nothing, and is quietly no override at all.

| Variable | What it does |
|---|---|
| `AI_BUDDY_DIRECTOR_API_KEY` | Required for a remote provider. Optional for a [local](#a-local-model-server) server (unset when the server has no auth; set when it requires one). Empty or unset for a remote URL means Static only. |
| `AI_BUDDY_DIRECTOR_BASE_URL` | Provider origin. Default `https://api.openai.com`. |
| `AI_BUDDY_DIRECTOR_MODEL` | Model name. Default `gpt-4o-mini`. |
| `AI_BUDDY_DIRECTOR` | The Director on or off, whatever Settings saved. Off keeps Static even when a key is set; on still needs a key or a local server. The window and the tray name the variable and disable the toggle. |
| `AI_BUDDY_DIRECTOR_TIMEOUT_SECS` | Completer timeout. Default 20 remote, 120 local — a cold local model loads weights on the first call. |
| `AI_BUDDY_DIRECTOR_MAX_TOKENS` | Reply cap. Default 80 remote, 512 local. |
| `AI_BUDDY_DIRECTOR_WAKE_SECS` | First proactive model-call wait, in seconds (default 120). After each proactive model call the wait grows by the Character's `[director]` `model_base` and `model_power` (`wait * model_base ^ model_power`, default doubling), and caps at two hours. Not a heartbeat. Poke and Summon wake immediately. |

Settings → Director persists base URL and model, and stores the API key in the OS secret store (Keychain on macOS); Settings → Development persists the Completer timeout and reply cap. Editing any of the five retargets the running Director: the next wake reaches the new host, and the session in flight is dropped rather than answered against the old one — a streaming call closes its connection, so the old host stops generating too. No restart.

`cargo run` with those env vars unset uses the saved Completer. The env vars remain a one-process override, and the window says so: a field one of them owns shows that value, names the variable, and takes no edit, because the Director would ignore one. An exported `AI_BUDDY_DIRECTOR_API_KEY` also keeps the Keychain out of the launch entirely — the env has already decided the key, so nothing reads the store.

On macOS a saved key is guarded by an access control list naming the build that wrote it, and an ad-hoc signature names it by a hash that every `cargo build` changes — so a rebuilt app is a stranger to its own key and the launch costs two dialogs. `scripts/dev-sign.sh` signs the build with a stable identity the list can name instead, and its header carries the whole argument. From the repository root:

```sh
cargo build -p ai-buddy && scripts/dev-sign.sh && ./target/debug/ai-buddy
```

A key saved before the first signed run keeps the old list — clear it in Settings and save it once more. Signing also changes the identity macOS grants Accessibility and Screen Recording to, so expect to grant those again, once. Released builds are ad-hoc signed too, so an update prompts the same way until there is a Developer ID to sign with (#283).

Settings → What the buddy can see is how you grant Accessibility and Screen Recording. The pane names the row macOS will show: a `cargo run` from Cursor is listed as Cursor, a packaged build as ai-buddy. Check the box, then turn that named app on in Privacy & Security.

Settings → Do Not Disturb → Sound is the mute. On by default; off takes effect on the next frame, no restart. Do Not Disturb also silences the buddy while it is on, and leaves the visual cues (#277). A machine that cannot start an audio context does the same: one warning in the webview console, then silence, with the visual still playing (#292).

A Character that should grow faster or slower than doubling says so:

```toml
[director]
model_base = 3
model_power = 1
```

Session calls stay quiet while the main display is asleep. Settings can turn the Director off, or leave it on and disable ambient wakes.

A 403 from xAI is the server refusing the key, not a bad JSON body (that is a 400). Keys are granted per-endpoint in [console.x.ai](https://console.x.ai); `/v1/responses` and `/v1/chat/completions` are separate ACLs. A team that requires mTLS wants `https://mtls.api.x.ai`. The stand-in retries chat-completions if Responses returns 403 or 404.

The stand-in asks for `stream: true`. A reply's first line is the Behavior name and runs one to three tokens, so almost the whole wait is a dialogue line the buddy does not need before it starts moving. Streaming is also the only shape a dropped call can be *stopped* in: closing a streaming connection ends the generation, where a whole-reply request runs to completion on the server whatever the client does. A server that rejects the field — or accepts it and sends whole-reply JSON anyway — stays one the buddy can run against, because the parser handles both.

`scripts/probe-model.sh` hits the same Completer without starting the overlay — GET `/v1/models` (and `/v1/api-key` on xAI), then both POST paths. Same env as `cargo run`. It prints status and body, never the key. Later this is also how to check a Harness is reachable.

```sh
AI_BUDDY_DIRECTOR_API_KEY="$XAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.x.ai \
AI_BUDDY_DIRECTOR_MODEL=grok-4.6 \
scripts/probe-model.sh
```

### A local model server

The buddy wakes on a pace all day and every Poke is a wake on top of that, so a hosted API puts a meter on idling — and each wake sends the frontmost application name and the clock off the machine. A server of your own removes the metering, and a server on loopback also keeps that context on this machine; a box across the LAN still receives it. "Local" here means loopback, an RFC1918 or IPv6 unique-local address, or a `.local` name — the LAN counts, which is a wider circle than the *on-device* "Local Gate" in [CONTEXT.md](./CONTEXT.md). A local base URL makes `AI_BUDDY_DIRECTOR_API_KEY` optional: leave it unset when the server has no auth, set it when the server requires one. A remote URL still requires a real key, so a missing cloud key never becomes an unauthenticated request.

These servers speak `/v1/chat/completions`, which is the path the Completer already builds:

| Server | Base URL | Model name | Auth | Tested |
|---|---|---|---|---|
| [Ollama](https://ollama.com) | `http://localhost:11434` | a tag: `gemma4`, `llama3.2:3b` | none by default | yes — `gemma4:latest`, 9.6 GB, on an Apple-silicon Mac |
| [oMLX](https://github.com/jundot/omlx) | `http://localhost:8000` | a served model id | API key required | yes |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) `llama-server` | `http://localhost:8080` | the gguf path, or `--alias` | optional `--api-key` | no |
| [LM Studio](https://lmstudio.ai) | `http://localhost:1234` | the id shown in its server tab | optional | no |
| [vLLM](https://docs.vllm.ai) | `http://localhost:8000` | the served model id | optional `--api-key` | no |
| [MLX](https://github.com/ml-explore/mlx-examples) `mlx_lm.server` | `http://localhost:8080` | a Hugging Face repo id | none | no |

**Ollama** (no auth):

```sh
ollama pull gemma4
ollama serve

AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
cargo run
```

**oMLX** (requires API key):

```sh
omlx serve --model mlx-community/Qwen2.5-1.5B-Instruct-4bit --api-key your-key-here

AI_BUDDY_DIRECTOR_API_KEY="$OMLX_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:8000 \
AI_BUDDY_DIRECTOR_MODEL=gemma-4-e2b-it-4bit \
cargo run --bin ai-buddy
```

**Check a server** before you trust it — reports whether the model you configured is actually loaded:

```sh
# Ollama (no key)
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
scripts/probe-model.sh

# oMLX (with key)
AI_BUDDY_DIRECTOR_API_KEY="$OMLX_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:8000 \
AI_BUDDY_DIRECTOR_MODEL=gemma-4-e2b-it-4bit \
scripts/probe-model.sh
```

At startup the app asks the same question once, in the background, and says so when the answer is no:

```
director: http://localhost:11439 unreachable: Connection refused; staying on StaticDirector until it answers
director: http://localhost:11434 model "llama3.2" is not served; it has gemma4:latest
```

Neither line stops anything: a wake that fails already falls back to Static per turn. The line exists so a buddy that went quiet is not a mystery.

See [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for local model server setup and reply contract measurements.

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

See [DEVELOPMENT.md](./docs/DEVELOPMENT.md) for:
- Toolchains (Rust, Python, Node)
- Pre-commit hooks
- Verifying the overlay (unit tests, `verify-overlay.sh`, 23-step human checklist)
- Trace variables (HITTEST, FRAMES, DIRECTOR, ENGINE)
- Character Packages (search paths, AI_BUDDY_CHARACTERS env)
- Importing a pet from petscodex or Shimeji-ee
- Running several buddies at once

**Design and decisions:**
- [CONTEXT.md](./CONTEXT.md) — Vocabulary
- [DESIGN.md](./DESIGN.md) — Design decisions
- [docs/SPEC.md](./docs/SPEC.md) — v1 scope and requirements
- [docs/adr/](./docs/adr/) — Architecture Decision Records

## Prior Art and Attribution

[WindowPet](https://github.com/SeakMengs/WindowPet) (MIT) is the reference for a Tauri desktop pet. ai-buddy is a greenfield build rather than a fork, for the reasons in [ADR-0001](./docs/adr/0001-greenfield-tauri-not-fork-windowpet.md). The overlay is an independent implementation. The tray, launch-at-login, and updater follow WindowPet's shape under MIT.

Character art provenance is documented in each Character Package manifest.

## License

MIT.
