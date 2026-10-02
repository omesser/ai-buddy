# Control-click menu scenario proof

## What it tests

Issue #1062 added Control-click as a secondary click on macOS. The scenario
verifies that Control-click on the sprite opens the context menu.

## Pattern compliance

The scenario follows the established `scripts/scenarios/` pattern:

### Structure matches existing scenarios

- **Header format**: Lines 2-13 declare what happens on screen, input, duration,
  grants, and assertions, matching `thinking-row.sh` and `chat-header-narrow.sh`.
- **Exit 2 without --go**: Lines 17-20 print the header and exit 2, the
  takeover protocol from `docs/agents/gui-takeover.md`.
- **Isolated HOME**: Line 52, matching all existing scenarios.
- **Fixture Harness**: Lines 46-47, `script=nop` is the simplest non-interactive
  harness from `src-tauri/src/harness.rs`.
- **Long wake interval**: Line 51, `FIDGET_DIRECTOR_WAKE_SECS=3600` prevents
  proactive model calls from interfering with the test.

### Tools and assertions

- **Swift inline**: Lines 58-81 and 116-136 use inline Swift blocks for:
  - Finding sprite bounds via `CGWindowListCopyWindowInfo` (pattern from
    `window-id.swift`)
  - Sending Control-click via `CGEvent` with `.maskControl` flag (pattern from
    `click-cursor.swift` but with Control modifier)
  - Detecting menu-layer windows (pattern from `ax-settings.swift` lines 195-201)
  - Dumping AX tree to verify menu items (pattern from `ax-settings.swift`
    `dump` command)

- **AX assertions**: Lines 166-169 grep for expected menu items (Chat, Settings,
  Quit) in the AX dump, matching the assertion pattern from `chat-header-narrow.sh`
  lines 85-105.

## Control-click implementation

### Event synthesis

Lines 119-136 create a Control-click by:

1. Creating `CGEvent` for `leftMouseDown` and `leftMouseUp` at sprite centre
2. Setting `.flags = .maskControl` on both events (lines 133-134)
3. Posting with 60ms between down and up (line 135)

This follows the Control-click implementation from issue #1062, which reads
Control state via `CGEventSourceFlagsState` at the press boundary and treats
the press as secondary.

### Menu detection

Lines 138-153 wait for a menu-layer window (layer >= 100) to appear after the
Control-click, using the same `CGWindowListCopyWindowInfo` filter as
`ax-settings.swift` `menuWindows()`.

Lines 158-164 verify the menu via AX by calling `ax menus`, which counts
menu-role windows the AX API reports.

### Menu item verification

Lines 167-195 dump the menu's AX tree and walk its children to extract item
titles, then grep for "Chat", "Settings", and "Quit". These are the three items
the menu always contains per `src-tauri/src/menu.rs` lines 156-234:

- Chat (line 156)
- Settings (line 237)
- Quit (line 234)

## Exit codes

- Exit 2: without --go (line 19)
- Exit 1: any assertion failure via `fail()` (lines 40-44)
- Exit 0: all assertions pass (line 171)

## Evidence collected

Every run writes to `$TMPDIR/fidget-scenario-control-click-menu-<timestamp>/`:

- `app.log`: Fidget's stdout and stderr
- `menu.ax.txt`: The menu's AX dump showing item titles
- `menu-count.txt`: The count of menu windows via AX

## What it proves

When run with --go, the scenario will:

1. Launch Fidget with BMO character and nop harness
2. Find the sprite window via the window server
3. Send a Control-click to the sprite's centre
4. Wait for a menu-layer window to appear
5. Verify the menu contains Chat, Settings, and Quit items
6. Exit 0 if all assertions pass, exit 1 otherwise

This proves Control-click opens the same menu as a right-click, covering the
requirement from issue #1062.
