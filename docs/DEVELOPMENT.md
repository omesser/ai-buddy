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

Checks overlay windows, frame loop physics, hit-testing. Needs real desktop. See original README lines 668–826 for 23-step manual checklist.

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
