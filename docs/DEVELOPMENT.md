# Development Guide

Toolchains, hooks, trace variables, verification, Character Packages, and platform dependencies. Completers, Harnesses, and the MCP server are in [harness.md](./harness.md).

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
| `FIDGET_TRACE_CADENCE` | Bench-only, with no Settings row. The overlay's display frames as `cadence:` lines and the loop's tick count as `cadence-ticks:` once a second, for `scripts/bench-frame-cadence-macos.sh`. Read at launch only |
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
2. **Fixations:** three to five strong, specific opinions - things it loves, resents, takes personally, or takes credit for.
3. **Sample lines**, verbatim, introduced in prose ("It has been heard to say: …"). They carry the character's recurring bits and catchphrases. Be generous; `characters/black-mage/` shows how far that goes.

#### Universal rules

Leave these out of a personality file. `character_prompt` in `crates/core/src/director.rs` injects them for every Character:

- Stay in character, and never mention being a model or an assistant.
- Fit the bubble - five short sentences at the most.
- Vary, preferring an unused line, while a signature phrase may recur.
- Lean away from the Behaviors that just played.
- React to the moment - what just happened, and what the sprite stands on - when there is something worth remarking on.
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

## Further Reading

- [README](../README.md): what it does, how to run, platform support
- [CONTEXT.md](../CONTEXT.md): vocabulary
- [DESIGN.md](../DESIGN.md): design decisions
- [docs/SPEC.md](./SPEC.md): v1 scope and requirements
- [docs/harness.md](./harness.md): Completers, Director environment, local model servers, and the MCP server
- [docs/adr/](./adr/): Architecture Decision Records
