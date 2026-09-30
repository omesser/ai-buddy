# The panel anchor was a second webview

Follow-up to `memory-rss-and-multi-monitor.md`, for #645. The earlier Linux
short run (one Instance, `bmo:One`, 5 s settle, 30 s sample) sat near 820 MB
because the process list held two `WebKitWebProcess`es for one display. One
of them was the panel anchor.

## What the anchor was

`build_anchor_window` is the taskbar button on Linux and Windows, the stand-in
for the macOS Dock icon. It was a `WebviewWindow` on the overlay document
(`WebviewUrl::default()`, which is `index.html`). That page calls `character`
and listens for frames. Frames are emitted to `overlay-*` labels, so the
anchor never drew a sprite. wry builds a new WebKit `WebContext` for each
webview, and WebKitGTK runs one `WebKitWebProcess` per context. The button
was a renderer process.

It is now a `Window` with no document. Focus on Linux and the Windows
click-activate handler are the same. Chat, Settings, and the overlays are
still webviews. Tauri only exposes that window builder behind the `unstable`
feature; the feature adds no crate and does not change the runtime.

## What this machine measured

| | |
|---|---|
| Machine | this agent's Linux VM, WebKitGTK 2.52.6, no DRI3 (software GL) |
| Display | 1, 1920×1200 |
| Scenario | 1 Instance, `bmo:One`, `FIDGET_DIRECTOR=0` |
| Script | `scripts/bench-rss-linux.sh --settle 5 --seconds 30 --interval 2` |
| Release launch | the script always execs `target/debug/fidget`; the release rows copied `target/release/fidget` onto that path |

Same script, same scenario, back to back:

| Build | Anchor | Processes | Total RSS min / median / max |
|---|---|---|---|
| debug, before | webview | 4 | 839 / **880** / 883 MB |
| debug, after | window only | 3 | 605 / **652** / 657 MB |
| release, before | webview | 4 | 832 / **852** / 866 MB |
| release, after | window only | 3 | 582 / **583** / 608 MB |

Release is the user-facing pair: **852 → 583 MB median, −269 MB**. Debug on
the same scenario was **880 → 652 MB, −228 MB**. An earlier debug run, on the parent of `origin/main`, read 870 MB median,
so the short run is not a one-off.

Per-process median RSS, release:

| | fidget | WebKitNetworkProcess | WebKitWebProcess | WebKitWebProcess |
|---|---|---|---|---|
| before | 226 MB | 48 MB | 236 MB | 342 MB |
| after | 216 MB | 48 MB | 319 MB | — |

The process that disappeared is the lighter web process, about 236 MB. The
overlay's web process stayed.

Debug versus release, before the change, is 880 against 852. The debug
binary is not why a one-Instance buddy was ~850 MB. The WebKit helpers are
the system library either way. The fidget process moved 226 → 216 MB in
release and 239 → 236 MB in the paired debug runs.

Summed RSS counts a shared library in every process that maps it. One debug
snapshot of proportional set size (PSS), taken about 6 s after the overlay
line, fell from 492 MB to 392 MB. The removed web process was 135 MB PSS,
and the surviving one rose about 30 MB. The unique cut is smaller than the
RSS delta and still about 100 MB.

## What was discarded

- **Loading every Character at launch.** `load_all_characters` still runs.
  The eight installed packages are 3.4 MB of PNG. That is not this resident
  set. The macOS note's 79 MB was peak footprint during encode, and this
  run did not profile the heap.
- **A shared WebKit process, or a smaller cache.** Each wry webview has its
  own context, so a process-model flag would not merge the anchor with the
  overlay. Cache model and WebGL were left at wry's defaults. The anchor
  having no document removes the second context instead.
- **Chat and Settings.** This scenario never opens them. They are still
  created on demand.
- **The short settle.** Two debug runs before the change (870 and 880 MB)
  and a release run before it (852 MB) agree. It was not settle noise.

## What remains

- One overlay `WebKitWebProcess`, ~319 MB RSS median in the release after
  row, on a 1920×1200 window with software GL. Displays still cost pixels.
- `WebKitNetworkProcess`, ~48 MB, unchanged.
- The fidget process, ~216 MB RSS in release. A heap profile of that number
  is still open, as the macOS section already said.
- macOS and Windows were not remeasured. Windows uses the same
  `build_anchor_window`, so a WebView2 process should go with the document.
  That is unchecked here.
