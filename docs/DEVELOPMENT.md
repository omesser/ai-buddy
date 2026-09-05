# Development Guide

This document covers toolchains, verification, trace variables, Character Packages, and developer workflows that were moved from the main README for conciseness.

## Quick Start

```sh
# Clone and run
git clone https://github.com/omesser/ai-buddy.git
cd ai-buddy
cargo run -p ai-buddy
```

## Toolchains

Three toolchains, each earning its place:

| Toolchain | Needed for | Needed to build? |
|---|---|---|
| **Rust** | Everything: core crate, Tauri shell | yes |
| **Python** | pre-commit, frame generators, pet importer | no |
| **Node** | Renderer unit tests only | no |

Node tests the webview arithmetic that runs between Engine ticks. Any Node with `node --test` works; no package manager, no `node_modules`. See `package.json` for ESM declaration only.

## Pre-commit Hooks

```sh
pre-commit install
```

Covers whitespace, YAML/JSON/TOML, spelling, shell (shfmt + shellcheck), `cargo fmt`, `cargo clippy -D warnings`. Toolchain pinned in `rust-toolchain.toml`.

## Verifying the Overlay

### Unit Tests

```sh
cargo test -p ai-buddy-core     # Pure core, builds anywhere
cargo test                      # Everything including platform shell  
node --test tests/*.test.js     # Renderer interpolation
```

### Automated Script (macOS only)

```sh
scripts/verify-overlay.sh       # or --keep to leave app running
```

Checks overlay windows, frame loop physics, hit-testing. Needs real desktop.

### Manual Verification Checklist

A human is still needed for the last step, because only the window server can answer it. Run the app, then confirm:

1. **Clicks pass through empty space.** Click the desktop or a window anywhere the sprite is not. The click lands underneath.
2. **Clicks on the sprite do not pass through.** Click the sprite's body. The window underneath does not receive the click.
3. **Typing is never interrupted.** Put the cursor in another application and type. Click the sprite mid-sentence and keep typing. Every keystroke reaches the other application and focus never moves.
4. **Follows you across Spaces.** Switch Spaces. The sprite is present on the new one, in the same screen position.
5. **Motion is continuous, not stepped.** Watch it fall. It slides down the screen rather than jumping between positions, and it does not judder when it crosses a window's edge.
6. **The art is crisp.** On a Retina display the pixels are hard squares with no blur or soft edges, and every pixel of the sprite is the same size as every other. A blurred sprite means the integer scale or the nearest-neighbour filtering was lost.
7. **It rests on the Dock, not behind it.** Let the sprite settle at the bottom of the screen. Its feet stand on the Dock's top edge and the whole sprite is visible. Then turn on Dock auto-hiding in System Settings: within a poll the sprite falls the rest of the way to the bottom of the screen, because the Dock gave the space back. Turn it off and the sprite is lifted again.
8. **Declared cadence is honoured.** Point ai-buddy at a copy of Black Mage whose idle declares a faster `fps`, and the idle is visibly faster than it was at the declared 1.
9. **A click makes it react.** Click the sprite once without moving the mouse. It plays its `react` animation for about half a second, then goes back to what it was doing.
10. **Press and drag picks it up.** Press on the sprite and move. It follows the cursor. Release over a window and it lands on that window's top edge.
11. **A flick throws it.** Drag and release while still moving and it leaves your hand on an arc. Hold still for a moment before releasing and it drops straight down instead.
12. **It can be put down over the Dock, and does not stay there.** Drag it down over the Dock. Let go and it settles back onto the Dock's top edge, fully visible.
13. **A window you drag slowly carries it.** Let the sprite settle on a window's top edge, then drag that window slowly. The sprite rides the edge and keeps its place along it.
14. **A window you fling leaves it behind.** With the sprite perched, throw the same window: grab the title bar and move fast. The sprite stays where it stood, in the air, and falls.
15. **The two shipped Characters are two companions.** Run each in turn and watch it idle. BMO hums to itself through a four-frame singing loop; Nim eases through six, blinks, and carries a translucent shadow.
16. **A fullscreen application takes the screen and the Character leaves it.** Put any application into fullscreen. Within about a tenth of a second the sprite fades out. Leave fullscreen and it fades back in.
17. **Ordinary window switching changes nothing.** Command-Tab between applications, open and close windows, drag them around, switch Spaces. The sprite never blinks.
18. **The hotkey puts it away and brings it back at once.** Press Control-Option-Command-B. The sprite is gone on the keystroke, with no fade. Press it again and it is back, instantly.
19. **The hotkey outranks the rules.** Press the hotkey to put the sprite away, then enter a fullscreen application and leave it again. The sprite stays away.
20. **It is absent from a real screen share.** Start a real share — Zoom, Meet, Teams — sharing your whole screen. The sprite is on your screen and not in theirs.

The last three need a second display:

21. **A Character on a seam is whole.** Drag the sprite slowly across the boundary between two displays and hold it there, half on each. Both halves are drawn, and they meet.
22. **Either half can be clicked.** With the sprite straddling, click the half on each display in turn. Both pick it up.
23. **A display can come and go.** With the app running, unplug a display. The sprite carries on. Plug it back in: the sprite can be dragged onto it again within a second or so.

For multiple instances (24–27), start with `AI_BUDDY_INSTANCES="bmo:One,bmo:Two,nim:Nim"` and confirm each buddy acts independently.

## Trace Variables

Set environment variables for live debugging (all off by default):

| Variable | Traces |
|---|---|
| `AI_BUDDY_TRACE_HITTEST` | Click-through decisions |
| `AI_BUDDY_TRACE_FRAMES` | Engine frames (state, position, animation) per tick |
| `AI_BUDDY_TRACE_DIRECTOR` | Session wakes: prompt, reply, Behavior played |
| `AI_BUDDY_TRACE_ENGINE` | Behavior/Primitive/Animation/State changes |
| `AI_BUDDY_CAPTURABLE` | Allow screen captures (default: excluded) |

Values: `1`/`on`/`true`/`yes` for on, `0`/`off`/`false`/`no` for off (case-insensitive).

## Director Environment

| Variable | What it does |
|---|---|
| `AI_BUDDY_DIRECTOR_API_KEY` | API key (required for remote, optional for local) |
| `AI_BUDDY_DIRECTOR_BASE_URL` | Provider origin (default: `https://api.openai.com`) |
| `AI_BUDDY_DIRECTOR_MODEL` | Model name (default: `gpt-4o-mini`) |
| `AI_BUDDY_DIRECTOR_TIMEOUT_SECS` | Timeout (default: 20 remote, 120 local) |
| `AI_BUDDY_DIRECTOR_MAX_TOKENS` | Reply cap (default: 80 remote, 512 local) |
| `AI_BUDDY_DIRECTOR_WAKE_SECS` | First wake wait (default: 120s), then exponential backoff |

### Local Model Servers

Tested servers supporting `/v1/chat/completions`:

| Server | Base URL | Auth | Tested |
|---|---|---|---|
| [Ollama](https://ollama.com) | `http://localhost:11434` | none | yes |
| [oMLX](https://github.com/jundot/omlx) | `http://localhost:8000` | key required | yes |
| llama.cpp | `http://localhost:8080` | optional | no |
| LM Studio | `http://localhost:1234` | optional | no |

```sh
# Ollama example
ollama pull gemma4 && ollama serve
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 cargo run
```

Check server: `scripts/probe-model.sh` with same env vars.

## Character Packages

Search paths (in order):
1. `~/Library/Application Support/ai-buddy/characters/`
2. Shipped characters (copied from `characters/` at build time)

Override with `AI_BUDDY_CHARACTERS=/path/to/chars` (colon-separated).

Eight characters ship: **Buddy Bot** (default), BMO, Nim, Black Mage, Cat, Jotaro Kujo, Timber Wolf, Trump.

### Writing a Character

A Character Package is a directory or `.zip` archive holding a `character.manifest`, a `personality.txt`, and the frames its manifest names. The format is first-class but undocumented until v2.

#### Writing a personality

`personality.txt` is plain prose the loader never interprets, up to 2000 characters. A register alone is not enough: a model given only temperament converges on the same three assistant-flavored lines. A good one contains three things, unlabeled (#156):

1. **Who the character is and how it carries itself**, fused — the paragraph or two every shipped file opens with. Skip what the sprite already shows: a description of the art buys nothing, and the words are better spent on how the character speaks and what it notices.
2. **A fixations paragraph**: three to five strong, specific opinions that generate material — things it loves, resents, takes personally, or takes credit for.
3. **Sample lines**, verbatim, introduced in prose ("It has been heard to say: …"). Well-chosen lines also carry the character's recurring bits, which is why bits get no section of their own. Be generous: each line is another calibration point, and `characters/black-mage/` shows how far that goes. A catchphrase belongs here: the prompt asks for variety but leaves repetition the character owns to the personality.

#### Universal rules

Leave these out of a personality file. `character_prompt` in `crates/core/src/director.rs` injects them once for every Character, so the files cannot drift apart on them:

- Stay in character, and never mention being a model or an assistant.
- Fit the bubble — five short sentences at the most.
- Vary, preferring an unused line, while a signature phrase may recur.
- Lean away from the Behaviors that just played.
- React to the moment — what just happened, and what the sprite stands on — when there is something worth remarking on.
- Dialogue is demeanour, never capability: no promising actions on the machine, no claiming abilities.

The package format (manifest structure, animation declarations, Behavior composition) stays internal and undocumented until v2.

### Running Multiple Instances

```sh
cd src-tauri && AI_BUDDY_INSTANCES="buddy-bot:One,buddy-bot:Two,nim:Nim" cargo run
```

## Importing Pets

Translate petscodex or Shimeji-ee packs to Character Packages:

```sh
uv venv && uv pip install pillow
npx petscodex install labubu
.venv/bin/python scripts/import-pet.py ~/.codex/pets/labubu --format petscodex -o characters/labubu
cargo run -p ai-buddy-core --example validate -- characters/labubu
```

Supported: [Pets Codex](https://petscodex.com/), [petdex](https://petdex.dev/), [Shimeji Shop](https://shimejishop.com/).

## macOS Developer Signing

Stable signature for Keychain access:

```sh
cargo build -p ai-buddy && scripts/dev-sign.sh && ./target/debug/ai-buddy
```

Grant permissions: Settings → What the buddy can see.

## Linux Dependencies

```sh
# Debian/Ubuntu
sudo apt install libayatana-appindicator3-dev
```

Tray requires StatusNotifier host (GNOME Shell, KDE Plasma, XFCE panel). GStreamer for audio.

## Platform Details

### Linux X11/Wayland

Single build with runtime lane selection. XWayland usually answers. Wayland-only sessions lose window geometry (screen-edge physics only, no Perches).

### Windows

NSIS installer ships. Some cells stub/degraded (see platform table in main README). Shell binary is real; stubs are about overlay depth.

## Further Reading

- Main README: What it does, how to run, platform support
- [CONTEXT.md](../CONTEXT.md): Vocabulary
- [DESIGN.md](../DESIGN.md): Design decisions  
- [docs/SPEC.md](./SPEC.md): v1 scope and requirements
- [docs/adr/](./adr/): Architecture Decision Records
