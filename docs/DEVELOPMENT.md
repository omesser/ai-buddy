# Development Guide

Toolchains, verification, trace variables, Completers and Harnesses, Character Packages, and MCP.

## Quick Start

```sh
# Clone and run
git clone https://github.com/omesser/fidget.git fidget
cd fidget
cargo run -p fidget
```

## Local Data

The data folder:

- macOS: `~/Library/Application Support/fidget`
- Linux: `~/.local/share/fidget`
- Windows: `%APPDATA%\fidget` (e.g. `C:\Users\<user>\AppData\Roaming\fidget`)

It holds two files:

- **`memory.md`**: everything the fidgets know about you, shared across all Character Instances. Human-editable Markdown with no size limit.
- **`action-log.jsonl`**: one JSON line per Harness action (prompts, tool calls, usage). Append-only, rotated.

### Action Log rotation

[`file-rotate`](https://crates.io/crates/file-rotate) 0.8.x rotates the log.

- A write that pushes the current file past 2 MB rotates it (`ContentLimit::BytesSurpassed`). `ContentLimit::Bytes` would split a write mid-line and break JSONL.
- Files are `action-log.jsonl`, then `.1` through `.10`. The oldest drops off. Disk ceiling is about 22 MB.
- One writer only. Concurrent writes from several processes are not supported.
- Rotation and write failures are dropped, with at most one stderr line per 60s. The log must never block a Director turn.
- JSON is serialized with `serde_json::to_string` before the write.

To inspect it:

- macOS: `tail -f ~/Library/Application\ Support/fidget/action-log.jsonl`
- Linux: `tail -f ~/.local/share/fidget/action-log.jsonl`
- Windows (PowerShell): `Get-Content -Wait -Tail 50 $env:APPDATA\fidget\action-log.jsonl`
- The **Action Log…** row in the tray and sprite menus opens the current file in the system editor.

### Memory size

Memory has no automatic size limit, on purpose: it is user-owned, and auto-deletion or write refusal would break that. Trim it in any editor, wipe it in Settings (a timestamped backup is kept), and watch what the Harness writes in the Action Log.

## Toolchains

| Toolchain | Needed for | Needed to build? |
|---|---|---|
| **Rust** | Core crate and Tauri shell | yes |
| **Python** | pre-commit, frame generators, pet importer | no |
| **Node** | Renderer unit tests | no |

Any Node with `node --test` works. No package manager, no `node_modules`; `package.json` only declares ESM.

## Pre-commit Hooks

```sh
pre-commit install
```

Covers whitespace, YAML/JSON/TOML, spelling, shell (shfmt + shellcheck), `cargo fmt`, and `cargo clippy --workspace --all-targets -- -D warnings`, as CI runs it. The toolchain is pinned in `rust-toolchain.toml`.

## Trace Variables

All off by default. Switches take `1`/`on`/`true`/`yes` or `0`/`off`/`false`/`no`, in any case.

| Variable | Effect |
|---|---|
| `FIDGET_TRACE_HITTEST` | Click-through decisions |
| `FIDGET_TRACE_FRAMES` | Engine frames per tick: `Grounded pos(x,y)`, `Dragged`, `Perched`, animation |
| `FIDGET_TRACE_DIRECTOR` | Session wakes: prompt, reply, Behavior played or refused |
| `FIDGET_TRACE_ENGINE` | Behavior, Primitive, Animation, and State changes |
| `FIDGET_TRACE_WINDOWS` | Windows only, set to any value: window count and the first 3 bounds on first read |
| `FIDGET_CAPTURABLE` | `1` forces the overlay into screen captures, `0` excludes it. Overrides Settings → Presence → "Appear in screenshots and screen shares". For verify scripts and CI. |

## Verifying the Overlay

### Unit Tests

```sh
cargo test -p fidget-core     # Pure core, builds anywhere
cargo test                    # Everything including the platform shell
node --test tests/*.test.js   # Renderer and design pages
```

### Automated Scripts

```sh
scripts/verify-overlay.sh       # macOS: overlay, physics, hit-testing
scripts/verify-overlay-x11.sh   # Linux X11: EWMH states, click-through
scripts/verify-overlay-win.ps1  # Windows: WS_EX_NOACTIVATE, Perch on dual display
node scripts/chat-ask-order.mjs # Chat surface in headless Chromium: a typed turn, an ask, the answer under it
```

The overlay scripts need a real desktop. `chat-ask-order.mjs` loads `src/chat.html` with `window.__TAURI__` stubbed, so it needs no app and activates no window. It needs a headless Chromium; `FIDGET_CHROME` names one other than Playwright's shell.

```sh
scripts/verify-settings-webview-select-macos.sh     # macOS: AI-source <select> opens above the overlay (#849)
scripts/verify-settings-keyboard-webview.sh         # macOS: keyboard-only Settings (#848)
scripts/verify-settings-webview-clipboard-macos.sh  # macOS: Copy on the BYO row writes the pasteboard (#855)
scripts/verify-settings-webview-phase2-win.ps1      # Windows: window, tabs, Sound round-trip, z-order
scripts/verify-settings-zorder-x11.sh               # Linux: Settings above the overlay
scripts/verify-anchor-taskbar-win.ps1               # Windows: no auto-open Settings, taskbar click opens Settings
scripts/bench-rss-macos.sh                          # macOS: resident set over a run, per process
scripts/bench-rss-linux.sh                          # Linux: RSS baseline (needs a working display)
scripts/bench-rss-windows.ps1                       # Windows: RSS baseline
```

CI does not run the Settings sittings. `verify-settings-keyboard-webview.sh` drives Tab, Space, Enter, and Escape, compares each tab's focus sequence with the AX dump, requires a focused still per tab and a committed `<select>` value, and writes a pass/fail table and stills under `.verify/`. Its pure checks run on fixtures in `scripts/test_verify_settings_keyboard.sh`. The AX / UIA / AT-SPI helpers are `scripts/ax-settings.swift`, `scripts/ax-settings-win.ps1`, and `scripts/ax-settings-linux.py`.

Build the debug binary, then run from the repo root:

```sh
cargo build -p fidget
./scripts/verify-settings-keyboard-webview.sh
.\scripts\verify-settings-webview-phase2-win.ps1
```

The scripts use `target/debug/fidget`. `FIDGET_VERIFY_BIN` names another binary.

On macOS, grant Accessibility to the terminal or IDE that runs a sitting (System Settings > Privacy & Security > Accessibility). Without it the helper exits before it dumps the window. UI Automation on Windows needs no grant.

The bench-rss scripts measure rather than check. They sample the app and its webview helpers and print RSS and peak memory. The default is brief (settle ~3s, sample ~10s). `--research` (bash) or `-Research` (PowerShell) runs the long soak (settle 300s, sample 300s). Results are in [docs/research/memory-rss-and-multi-monitor.md](research/memory-rss-and-multi-monitor.md).

### Manual Verification Checklist

Only the window server can answer these. Run the app, then confirm:

1. **Clicks pass through empty space.** Click anywhere the sprite is not. The click lands underneath.
2. **Clicks on the sprite do not pass through.** Click the sprite's body. The window underneath gets nothing.
3. **Typing is never interrupted.** Type in another application and click the sprite mid-sentence. Every keystroke reaches that application and focus never moves.
4. **Follows you across Spaces.** Switch Spaces. The sprite is on the new one, in the same place.
5. **Motion is continuous.** Watch it fall. It slides rather than jumps, and does not judder at a window's edge.
6. **The art is crisp.** On a Retina display the pixels are hard squares, all the same size. Blur means the integer scale or nearest-neighbour filtering was lost.
7. **It rests on the Dock, not behind it.** Its feet stand on the Dock's top edge. Turn on Dock auto-hiding: within a poll it falls to the bottom of the screen. Turn it off and it is lifted again.
8. **Declared cadence is honoured.** Give a copy of Black Mage a faster idle `fps`. The idle is visibly faster than at the declared 1.
9. **A click makes it react.** Click once without moving. It plays `react` for about half a second, then resumes.
10. **Press and drag picks it up.** It follows the cursor. Release over a window and it lands on that window's top edge.
11. **A flick throws it.** Release while moving and it leaves on an arc. Hold still before releasing and it drops straight down.
12. **It can be put down over the Dock, and does not stay there.** Drop it over the Dock. It settles back onto the Dock's top edge, fully visible.
13. **A window you drag slowly carries it.** With the sprite on a window's top edge, drag the window slowly. The sprite rides the edge and keeps its place.
14. **A window you fling leaves it behind.** Throw the same window by its title bar. The sprite stays where it stood, in the air, and falls.
15. **Two Characters are two companions.** Run BMO, then Nim. BMO hums through a four-frame singing loop; Nim eases through six, blinks, and carries a translucent shadow.
16. **Fullscreen takes the screen and the fidget leaves it.** Enter fullscreen in any app. Within about a tenth of a second the sprite fades out, and fades back when you leave.
17. **Ordinary window switching changes nothing.** Command-Tab, open, close, and drag windows, switch Spaces. The sprite never blinks.
18. **The hotkey puts it away and brings it back at once.** Control-Option-Command-B hides it with no fade. Press again and it is back.
19. **The hotkey outranks the rules.** Hide it with the hotkey, then enter and leave fullscreen. It stays away.
20. **It can leave a real screen share.** Turn off Settings → Presence → "Appear in screenshots and screen shares", then share your whole screen in Zoom, Meet, or Teams. The sprite is on your screen and not in theirs.

With a second display:

21. **A fidget on a seam is whole.** Hold the sprite across the boundary, half on each display. Both halves are drawn and meet.
22. **Either half can be clicked.** Click the half on each display in turn. Both pick it up.
23. **A display can come and go.** Unplug a display while running. The sprite carries on. Plug it back in and it can be dragged onto it within a second or so.

For multiple instances, start with `FIDGET_INSTANCES="bmo:One,bmo:Two,nim:Nim"` and confirm each fidget acts independently.

## Running with a Completer

With no Director key, Static weights pick idle Behaviors from the Character's manifest. No model, no account, no permission. Connect a Completer for model-driven variety.

### Quick Start

OpenAI, Anthropic, and Ollama use `/v1/chat/completions`. [xAI](https://docs.x.ai/developers/model-capabilities/text/comparison) uses `/v1/responses`, selected by `FIDGET_DIRECTOR_BASE_URL=https://api.x.ai`. A full URL ending in `/chat/completions` or `/responses` is used as-is.

```sh
# OpenAI
cd src-tauri
FIDGET_DIRECTOR_API_KEY="$OPENAI_API_KEY" \
FIDGET_DIRECTOR_BASE_URL=https://api.openai.com \
FIDGET_DIRECTOR_MODEL=gpt-4o-mini \
cargo run

# Anthropic (OpenAI-compatible /v1/chat/completions)
cd src-tauri
FIDGET_DIRECTOR_API_KEY="$ANTHROPIC_API_KEY" \
FIDGET_DIRECTOR_BASE_URL=https://api.anthropic.com \
FIDGET_DIRECTOR_MODEL=claude-haiku-4-5 \
cargo run

# xAI — get a key at https://console.x.ai
cd src-tauri
FIDGET_DIRECTOR_API_KEY="$XAI_API_KEY" \
FIDGET_DIRECTOR_BASE_URL=https://api.x.ai \
FIDGET_DIRECTOR_MODEL=grok-4.6 \
cargo run

# Ollama (local, no key)
cd src-tauri
FIDGET_DIRECTOR_BASE_URL=http://localhost:11434 \
FIDGET_DIRECTOR_MODEL=gemma4 \
cargo run
```

### Director Environment

Switches read the same words as the trace variables. Any other value is a typo: the switch stays as Settings has it, and the launch prints a line naming the variable it ignored. An empty value is no override.

| Variable | What it does |
|---|---|
| `FIDGET_DIRECTOR_API_KEY` | Required for a remote provider. For a local server, set it only when the server requires auth. Empty or unset with a remote URL means Static only. |
| `FIDGET_DIRECTOR_BASE_URL` | Provider origin. Default `https://api.openai.com`. |
| `FIDGET_DIRECTOR_MODEL` | Model name. Default `gpt-4o-mini`. |
| `FIDGET_DIRECTOR` | The Director on or off, whatever Settings saved. Off keeps Static even with a key; on still needs a key or a local server. The window and the tray name the variable and disable the toggle. |
| `FIDGET_DIRECTOR_TIMEOUT_SECS` | Timeout for one HTTP Completer request, then Static. Default 30 seconds. Raise it for a cold local server. A Harness turn uses `FIDGET_HARNESS_TURN_TIMEOUT`. |
| `FIDGET_DIRECTOR_MAX_TOKENS` | Ceiling on one HTTP Completer turn. A safeguard against a model that will not stop, not a reply-length budget. Default 1024, or 8192 once the endpoint has been seen to mark its thinking (#606). A value here outranks both. A Harness decides its own reply length. |
| `FIDGET_DIRECTOR_BLANK` | Blank-AI mode, same as Settings → Development → "Blank AI". Empties the built-in Personality Prompt and the app-level instructions (voice rules, behavior list, reply contract) and still sends the Instance Prompt. The Prompt tab marks emptied layers Empty. With no contract the reply is prose, spoken without acting unless the Instance Prompt asks otherwise. The Harness lane keeps a separate session for this mode. |
| `FIDGET_DIRECTOR_REASONING_EFFORT` | Sent verbatim as `reasoning_effort` (chat-completions) or `reasoning.effort` (Responses). Overrides Settings → Development → "Reasoning effort". Default `low`, because sending nothing loses wakes on a local reasoning model at the turn ceiling. The picker offers `low`, `medium`, `high`; other values can be typed and are not validated. A host that refuses the field drops it for the session, and `FIDGET_TRACE_DIRECTOR` names the value. |
| `FIDGET_HARNESS` | Attach a Harness as the Completer: `claude`, `codex`, `copilot`, `cursor-agent`, `goose`, `grok`, `hermes`, `opencode`, `pi`, or any command line that speaks ACP on stdio. The Harness signs in on its own; a not-signed-in one is named in Chat with the command that fixes it. Set and empty is the kill switch (Off, whatever Settings saved). Unset falls through to Settings → AI → AI source. ADR-0022. |
| `FIDGET_HARNESS_CWD` | The Harness's project directory: spawn `current_dir` and ACP `session/new` / `session/load` cwd. Empty means the data folder, not `$HOME`. Overrides Settings → Development → "Working directory". The session file and Action Log stay in the data folder. |
| `FIDGET_HARNESS_TURN_TIMEOUT` | How long a Harness `session/prompt` may run before `session/cancel`, excluding time spent waiting on your answer to an ask, and restarted when you answer. Overrides Settings → Development → "Turn timeout, in seconds". Default 120. Read at attach. |
| `FIDGET_HARNESS_AUTH_RETRY_SECS` | How long to leave a not-signed-in Harness before retrying `session/new`. Overrides Settings → Development → "Auth retry, in seconds". Default 60. Read at attach. |
| `FIDGET_MCP_URL`, `FIDGET_MCP_TOKEN` | The app's loopback MCP endpoint and per-run bearer token. The app sets both on the stdio MCP entry it hands the Harness; with neither set the shim fails every call. Set them by hand to point the shim at a running app. Never written to a file or a log. ADR-0026. |
| `FIDGET_MCP_BIN` | The stdio MCP server binary, when it is not beside the app. Overrides Settings → Development → "MCP server binary". Read at attach. See [MCP Server](#mcp-server) for the three stdio routes. |
| `FIDGET_DIRECTOR_WAKE_SECS` | First proactive model-call wait. Overrides Settings → AI → "First wake, in seconds". Default 120. Each proactive call multiplies the wait by the Character's `[director]` `model_base ^ model_power` (default doubling), capped at two hours. Poke and Summon wake immediately. |

### Settings and Keyring

Settings → AI persists base URL, model, and first wake interval, and stores the API key in the OS secret store. Settings → Development persists the Model API timeout and turn ceiling, blank-AI mode, and the Harness turn timeout, auth-retry interval, MCP server binary, and working directory.

- A working-directory edit respawns the Harness, so process cwd and ACP cwd stay equal. Turn timeout and auth retry land on the next attach.
- Editing the Completer source or HTTP endpoint retargets the running Director with no restart. The session in flight is dropped; a streaming call closes its connection, so the old host stops generating.
- `cargo run` with the env vars unset uses the saved Completer. A field an env var owns shows its value, names the variable, and takes no edit.
- An exported `FIDGET_DIRECTOR_API_KEY` keeps the Keychain out of the launch entirely.
- Settings → Do Not Disturb → Sound mutes cue audio. On by default; off takes effect on the next frame. Do Not Disturb also silences cues and keeps the visual ones (#277). A machine that cannot start an audio context logs one webview console warning and plays the visual cue silently (#292).
- The window moves with a modifier-drag on empty chrome: Command-drag on macOS, Super-drag on Linux, Alt-drag on Windows. Nothing in the UI names this.

**Linux:** the key goes to Secret Service (GNOME Keyring, KWallet), or kernel keyutils when Secret Service is absent. Building the shell needs `libdbus-1-dev`. No packaged secret store is required.

**macOS Keychain ACL:** a saved key's access list names the build that wrote it. An ad-hoc signature names it by a hash that every `cargo build` changes, so a rebuilt app costs two Keychain dialogs. `scripts/dev-sign.sh` signs with a stable identity instead. From the repo root:

```sh
cargo build -p fidget && scripts/dev-sign.sh && ./target/debug/fidget
```

A key saved before the first signed run keeps the old list: clear it in Settings and save it again. Signing also changes the identity macOS grants Accessibility and Screen Recording to, so expect to grant those once more. Released builds are ad-hoc signed too, so updates prompt the same way until there is a Developer ID (#283).

**Accessibility, Screen Recording, and Input Monitoring:** grant them in Settings → What the fidget can see. The pane names the row macOS will show: a `cargo run` from Cursor is listed as Cursor, a packaged build as Fidget. Check the box, then turn that app on in Privacy & Security. Input Monitoring lets the mouse wake an idle sprite, so a poke lands at once instead of up to a second later. Without it the frame loop keeps its idle back-off. The tap starts within about a second of the grant, with no relaunch, and stops when you uncheck the box.

### Local Model Servers

The fidget wakes all day and every Poke is another wake, so a hosted API meters idling, and each wake sends the frontmost application name and the clock off the machine. A server of your own removes the meter. On loopback it also keeps that context on the machine; a LAN box still receives it. "Local" means loopback, an RFC1918 or IPv6 unique-local address, or a `.local` name. A local base URL makes `FIDGET_DIRECTOR_API_KEY` optional.

These servers speak `/v1/chat/completions`:

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

FIDGET_DIRECTOR_BASE_URL=http://localhost:11434 \
FIDGET_DIRECTOR_MODEL=gemma4 \
cargo run
```

**oMLX** (requires API key):

```sh
omlx serve --model mlx-community/Qwen2.5-1.5B-Instruct-4bit --api-key your-key-here

FIDGET_DIRECTOR_API_KEY="$OMLX_API_KEY" \
FIDGET_DIRECTOR_BASE_URL=http://localhost:8000 \
FIDGET_DIRECTOR_MODEL=gemma-4-e2b-it-4bit \
cargo run --bin fidget
```

### Testing Connectivity

`scripts/probe-model.sh` hits the Completer without starting the overlay: GET `/v1/models` (and `/v1/api-key` on xAI), then both POST paths. It reads the same env as `cargo run`, prints status and body, and never prints the key. It also reports whether the configured model is loaded.

```sh
FIDGET_DIRECTOR_API_KEY="$XAI_API_KEY" \
FIDGET_DIRECTOR_BASE_URL=https://api.x.ai \
FIDGET_DIRECTOR_MODEL=grok-4.6 \
scripts/probe-model.sh

# Ollama (no key)
FIDGET_DIRECTOR_BASE_URL=http://localhost:11434 \
FIDGET_DIRECTOR_MODEL=gemma4 \
scripts/probe-model.sh

# oMLX (with key)
FIDGET_DIRECTOR_API_KEY="$OMLX_API_KEY" \
FIDGET_DIRECTOR_BASE_URL=http://localhost:8000 \
FIDGET_DIRECTOR_MODEL=gemma-4-e2b-it-4bit \
scripts/probe-model.sh
```

The app runs the same check once at startup, in the background, and logs a miss:

```
director: http://localhost:11439 unreachable: Connection refused; staying on StaticDirector until it answers
director: http://localhost:11434 model "llama3.2" is not served; it has gemma4:latest
```

Neither line stops anything. A failed wake already falls back to Static.

`scripts/probe-harness.sh` does the same for a Harness. It serves Fidget's MCP endpoint, spawns the Harness, prints what `initialize` advertised, runs one fixed prompt, and reports whether the reply parsed as a Behavior proposal and whether the Harness fetched the tool list. No overlay, and no credential printed: a Harness that is not signed in comes back as the command to run in your own terminal.

```sh
FIDGET_HARNESS=hermes scripts/probe-harness.sh
```

```
probe-harness
  harness      hermes
  command      hermes acp
  dir          /Users/you/Library/Application Support/fidget/probe
  timeout      turn 20s, attach 20s

attach
  agent        hermes-agent
  loadSession  true
  mcp http     false
  mcp          /path/fidget --mcp-stdio
  authMethods  custom runtime credentials, Configure Hermes provider
  session      33f5d650-5476-40c6-876b-cb04f14bfc27

turn
  prompt       Reply with exactly this one line and nothing else: Wave | Hello from the probe.
  stop         end_turn
  reply        Wave | Hello from the probe.
  proposal     Wave | Hello from the probe.
  mcp listed   yes, 1 tools/list request(s)
```

- `mcp` is what the session was handed. `mcp http` is why: `hermes` advertises no HTTP MCP on ACP `initialize`, so it gets the stdio server, which relays to the app (ADR-0026). A Harness that advertises HTTP MCP shows a `http://127.0.0.1:…/mcp` URL instead, never the token. The probe binds that listener itself before it attaches. A failed bind is reported in capitals under `mcp`, and then nothing it reports about tools holds.
- `mcp listed` is whether the Harness asked for `tools/list`. A tool it calls is answered through the app's `dispatch` against an empty desktop and no Instances, so `speak` fails and `list_windows` is empty. Each call prints as `mcp call`.
- Exit code: 2 means never asked (nothing configured, no binary, not signed in), 1 means asked and not answered, 0 means `end_turn`, a completed turn.
- The `probe` folder keeps the session file and the Action Log away from a real install. Memory is not isolated: a `remember` during a probe writes the real `memory.md`.

Dated transcripts belong on the issue that ran the probe. Update the README's [Harness Support](../README.md#harness-support) table when a row's command or user-visible session behavior changes, not when a probe is re-run.

### Provider Details

**Cursor API:** `CURSOR_API_KEY` is for the Cloud Agents API and SDKs, not a Completer. `https://api.cursor.com` has no `/v1/chat/completions`; a POST there is a 404 and Static takes over.

**xAI keys:** a 403 is xAI refusing the key; a bad body is a 400. Keys are granted per endpoint in [console.x.ai](https://console.x.ai), and `/v1/responses` and `/v1/chat/completions` are separate ACLs. A team that requires mTLS wants `https://mtls.api.x.ai`. The Completer retries chat-completions if Responses returns 403 or 404.

**Streaming:** the Completer asks for `stream: true`. The Behavior name is the first line, so the fidget can start moving before the dialogue arrives. Closing a streaming connection also stops the generation, which a whole-reply request does not. A server that rejects the field, or ignores it, still works: the parser handles both shapes.

### Proactive model calls

Session calls stay quiet while the main display is asleep. Settings can turn the Director off, or keep it on with proactive calls disabled.

A Character that should back off faster or slower than doubling says so:

```toml
[director]
model_base = 3
model_power = 1
```

### Reply Contract Measurements

`measure_the_reply_contract_failure_rate` in `src-tauri/src/model.rs` measures how often a model breaks the reply contract. It is `#[ignore]`d and runs against a live server; its doc comment has the command and the `FIDGET_BENCH_*` knobs. The last published results are in the [README before #135](https://github.com/omesser/fidget/blob/f08c6edffaa4cbc310164a594fe4be889e22f9de/README.md#L438).

## Character Packages

Search paths, in order:

1. `~/Library/Application Support/fidget/characters/`
2. Shipped characters (copied from `characters/` at build time)

`FIDGET_CHARACTERS=/path/to/chars` overrides them (colon-separated).

Eight characters ship: **Buddy Bot** (default), BMO, Nim, Black Mage, Cat, Jotaro Kujo, Timber Wolf, Trump.

### Writing a Character

A Character Package is a directory or `.zip` holding a `character.manifest`, a `personality.txt`, and the frames its manifest names. Manifest structure, animation declarations, and Behavior composition stay internal and undocumented until v2.

#### Declaring where the art came from

`[source]` is the one part of the manifest documented before v2, because the [Character Gallery](https://omesser.github.io/fidget/characters.html) publishes it. A package that omits it shows up there with no attribution.

```toml
[source]
art     = "What the Character is, and where its frames came from."
url     = "https://example.com/the-pack"   # optional, http or https only
license = "The license the art carries, or that none is declared."
```

`license` is required whenever `[source]` is present. "None is declared" is a valid value; a missing key is not, because it looks like an unfinished manifest. A package with no `[source]` still loads, but cannot ship from this repository.

#### Writing a personality

`personality.txt` is plain prose the loader never interprets, up to 2000 characters. Temperament alone is not enough: a model given only that converges on the same few assistant-flavored lines. Include three things, unlabeled (#156):

1. **Who the character is and how it carries itself.** Skip what the sprite already shows; spend the words on how it speaks and what it notices.
2. **Fixations:** three to five strong, specific opinions — things it loves, resents, takes personally, or takes credit for.
3. **Sample lines**, verbatim, introduced in prose ("It has been heard to say: …"). They carry the character's recurring bits and catchphrases. Be generous; `characters/black-mage/` shows how far that goes.

#### Universal rules

Leave these out of a personality file. `character_prompt` in `crates/core/src/director.rs` injects them for every Character:

- Stay in character, and never mention being a model or an assistant.
- Fit the bubble — five short sentences at the most.
- Vary, preferring an unused line, while a signature phrase may recur.
- Lean away from the Behaviors that just played.
- React to the moment — what just happened, and what the sprite stands on — when there is something worth remarking on.
- Dialogue is demeanour, never capability: no promising actions on the machine, no claiming abilities.

### Running Multiple Instances

```sh
cd src-tauri && FIDGET_INSTANCES="buddy-bot:One,buddy-bot:Two,nim:Nim" cargo run
```

## Importing Pets

Translate [Pets Codex](https://petscodex.com/), [petdex](https://petdex.dev/), or [Shimeji Shop](https://shimejishop.com/) packs to Character Packages:

```sh
uv venv && uv pip install pillow
npx petscodex install labubu
.venv/bin/python scripts/import-pet.py ~/.codex/pets/labubu --format petscodex -o characters/labubu
cargo run -p fidget-core --example validate -- characters/labubu
```

## Linux Dependencies

```sh
# Debian/Ubuntu (build from source)
sudo apt install libayatana-appindicator3-dev
```

### Install packages (tray, cue audio, AppImage)

**Tray.** The tray icon is how you reach Settings, Character, Memory, and Quit, so the `.deb` depends on `libayatana-appindicator3-1` (the older `libappindicator3-1` is not accepted). The AppImage carries its own copy. Showing the icon also needs a StatusNotifier host, which the desktop provides and no package can declare: GNOME Shell, KDE Plasma, and XFCE's Status Tray plugin are hosts; Plank and a Wayland compositor with no tray protocol are not. Without a host, the sprite's right-click menu is the same menu.

**Cue audio** is Web Audio in WebKitGTK, played through GStreamer. `libwebkit2gtk-4.1-0` already depends on `gstreamer1.0-plugins-base` and `gstreamer1.0-plugins-good` (which ships `pulsesink`), enough under PipeWire-pulse or PulseAudio. An ALSA-only machine also needs `gstreamer1.0-alsa`. With no sound device the fidget stays silent and still draws the visual cue.

The AppImage bundles `libgstreamer` but not the plugin pack (`bundleMediaFramework` stays off; it would add tens of megabytes). Cue audio then needs the host's `gstreamer1.0-plugins-good` (plus `gstreamer1.0-alsa` without Pulse/PipeWire) and a running sink. If those are installed and the AppImage is still mute, GStreamer is looking for plugins inside the image.

**AppImage on Ubuntu** needs `libfuse2` (22.04) or `libfuse2t64` (24.04+).

### Linux X11/Wayland

One build, lane chosen at runtime. XWayland usually answers. A Wayland-only session loses window geometry: screen-edge physics only, no Perches.

### Windows

The NSIS installer ships. The README platform table lists the degraded cells.

#### Harness Process Termination

The ACP Harness child and its descendants (e.g. `npx` spawning Node) go in a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. The child is spawned suspended, assigned to the job, then resumed, so no grandchild outlives a quit or detach. It also gets its own process group (`CREATE_NEW_PROCESS_GROUP`), so Ctrl+C into `cargo run` does not reach it.

## MCP Server

Two transports, not to be conflated:

1. **ACP** (Fidget ↔ Harness): always stdio. Fidget spawns the Harness and prompts it over newline-delimited JSON-RPC.
2. **MCP** (Harness → Fidget): the Harness calls back so `speak`, sensing, and Memory reach the fidget.

The README lists the [tools and resources](../README.md#harness--mcp).

### How it works

Dispatch lives in the running app (ADR-0023). `src-tauri/src/mcp_http.rs` serves the tools on `http://127.0.0.1:<random-port>/mcp` behind a per-run bearer token: 32 fresh bytes in memory, never on disk or in a log. It is thread-per-request with no async runtime, request/response only: no notifications, progress, sampling, SSE push, or prompts. If the bind fails at launch, the app runs without MCP (ADR-0023). The bind is loopback only, because the token authorizes moving the fidget (ADR-0023). The denylist applies to `list_windows`, `describe_screen`, and `fidget://windows`: it filters password managers and redacts password fields.

The ACP `initialize` bit `agentCapabilities.mcpCapabilities.http` decides the route:

- **Advertised** → the Harness gets the loopback URL and token directly.
- **Absent or false** → the Harness gets a stdio MCP server entry to spawn (ADR-0026). That binary is a stateless relay: it posts every JSON-RPC message to the app's endpoint, using `FIDGET_MCP_URL` and `FIDGET_MCP_TOKEN` from its environment. It is found as `FIDGET_MCP_BIN`, else a `fidget-mcp` sidecar beside the app, else the app binary itself (`fidget --mcp-stdio`).
- **`cursor-agent`** ignores `mcpServers` entirely and loads servers only from an approved `.cursor/mcp.json` (#1020). It gets the loopback URL and token through that file.

### How `cursor-agent` is reached

`src-tauri/src/cursor_mcp.rs`. `.cursor/mcp.json` is the only place to define a server (`cursor-agent mcp` has no `add`), and `cursor-agent mcp enable` is the only way to approve one. Approvals are read once per `cursor-agent` process, so before spawning `cursor-agent acp`, attach:

1. Merges `{"url": …, "headers": {"Authorization": "Bearer …"}}` under `mcpServers."fidget"` in `<cwd>/.cursor/mcp.json`, beside existing servers. A file that does not parse is left alone and the attach continues without tools.
2. `chmod 600` the file, because it holds a live credential. Windows has no mode bits here, so the file keeps the project directory's ACL.
3. Runs `cursor-agent mcp enable fidget` in that directory (~380ms).

URL and token are new every app run, so each attach rewrites and re-approves. Within one run the entry is unchanged and a re-attach costs only the spawn.

Cursor appends each approval to `~/.cursor/projects/<slug>/mcp-approvals.json` and never prunes: about 31 bytes per app run. Detach does not call `cursor-agent mcp disable`, because that blocks the server from ever loading again.

Detach removes our entry, and the file and directory if attach created them. While attached, the token sits in the working directory's `.cursor/mcp.json`, owner-only, dead after the app run. What the project's VCS does with an untracked `.cursor/` is the project's business.

### Pointing a Harness you run yourself at Fidget

An attached Harness needs no setup. A Harness you launch yourself gets nothing forwarded, so register the endpoint by hand, once per app launch:

1. Start Fidget and open Settings.
2. Under **Point a Harness you run yourself at Fidget**, pick the Harness.
3. Copy the generated command (or JSON fragment, for Harnesses with no `mcp add`) and run or paste it as the instructions say. Hermes prompts for the token, so its row has a separate Copy for it.
4. Reload or restart the Harness session. Claude Code and OpenCode read MCP config only at session start.

The port is OS-assigned and the token is minted in memory at every launch (ADR-0018), so no document can carry the command, and a stale entry is a dead one. The generator (`byo_registration` in `src-tauri/src/settings.rs`) always hands out the loopback URL and token; the stdio relay plays no part here. The box is empty when the loopback bind failed.

| Harness | What the generator emits | Where the entry lands | Standing |
|---|---|---|---|
| `claude` | `claude mcp remove` then `claude mcp add --transport http …` | `~/.claude.json`, keyed by working directory (local scope) | **verified by hand** |
| `codex` | `export FIDGET_MCP_TOKEN=…` then `codex mcp add --url … --bearer-token-env-var` | `~/.codex/config.toml`, token stays in the environment | **verified by hand** |
| `cursor-agent` | `mcpServers` JSON fragment, then `cursor-agent mcp enable fidget` | `.cursor/mcp.json` in the project | **verified by hand** |
| `grok` | `grok mcp add … --transport http` with a header | `~/.grok/config.toml`, token in the file | **verified by hand** |
| `hermes` | `hermes mcp add --url … --auth header`, token pasted at its prompt | `~/.hermes/config.yaml`, token in `~/.hermes/.env` | **verified by hand** |
| `opencode` | `opencode mcp add --url … --header "Authorization=Bearer …"` | `~/.config/opencode/opencode.json`, token in the file | **verified by hand** |
| `pi` | `mcpServers` JSON fragment for `.mcp.json` or `~/.pi/agent/mcp.json` | the project, or the agent directory | **verified by hand** ‡ |
| `copilot` | `copilot mcp remove` then `copilot mcp add --transport http …` with a header | `~/.copilot/mcp-config.json`, token in the file | **config generated, unverified** |

**Verified by hand** means the box's output was run as given and that Harness's `speak` was recorded in the bubble on a real Mac (2026-09-21/22). **Config generated, unverified** means the shape was checked against the CLI (`copilot` 1.0.88; `remove` comes first because a second `add` fails) but no `speak` is recorded (#1016). A Harness the popup does not list (`custom`) gets the bare URL and token.

‡ `pi` has no MCP client of its own; it needs an adapter plugin such as `pi-mcp-adapter` (verified on pi 0.85.1 with pi-mcp-adapter 2.36.0). The adapter connects lazily: a headless `pi -p` run must call `mcp({"connect": "fidget"})` first, and an interactive session wants `/mcp connect` or `/mcp reconnect fidget`. The tool is reached through Pi's `mcp` proxy as `fidget_speak`. Whether an attached `pi-acp` session lists Fidget's tools is unmeasured (#984).

**Traps:**

- Every registration dies with the app. Under `opencode` a dead entry hangs `opencode run` at startup for minutes with no error; remove the entry or pass `--pure`.
- `opencode` has no `mcp remove`. Edit `~/.config/opencode/opencode.json` by hand.
- `hermes mcp remove` leaves `MCP_FIDGET_API_KEY` in `~/.hermes/.env`, so the next `hermes mcp add` skips the token prompt, reuses the dead token, and fails with `401 Unauthorized`. Delete that line before re-adding. Its `mcp add` and `mcp remove` also rewrite `config.yaml`, stripping comments and re-indenting.
- `codex` needs the `export` in the same shell that launches `codex`. Sourcing it through a pipe leaves the tool unregistered.
- `pi` needs its adapter told to connect. The fragment carries no `"lifecycle": "eager"`, so the server stays disconnected until asked.

### Not served

- **Pixels.** `describe_screen` is window metadata only; Fidget takes no screenshots and runs no OCR or vision ([ADR-0031](./adr/0031-drop-capture-tiers.md)). Agents that need pixels use Harness-native computer use or an MCP server like cua-driver.
- **Input events.** No click, type, or mouse tools (ADR-0003). The Harness owns desktop control.
- **Non-loopback MCP** (ADR-0023).

Whether a Harness sets `mcpCapabilities.http` is the Harness's decision. Hermes speaks HTTP MCP as a client through its own `mcp_servers` config, a different axis from the ACP bit; a Hermes that set the bit would take the loopback path with no change here.

## Further Reading

- [README](../README.md): what it does, how to run, platform support
- [CONTEXT.md](../CONTEXT.md): vocabulary
- [DESIGN.md](../DESIGN.md): design decisions
- [docs/SPEC.md](./SPEC.md): v1 scope and requirements
- [docs/adr/](./adr/): Architecture Decision Records
