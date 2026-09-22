# Settings is one webview on the Chat surface's tokens

**Status:** Accepted. Step 9 shipped; the three native renderers are gone.

## Context

Settings has been three native renderers of one form description:
`platform/macos/settings_window.rs` (AppKit), `platform/windows/settings_window.rs`
(Win32), `platform/x11/settings_window.rs` (GTK 3), all interpreting
`settings::form::describe()`. No ADR recorded that choice. It was made when the
tree had no JavaScript surface at all, and it was the right call then: native
was the only credible option.

ADR-0010 then chose a webview for the Chat surface, and gave as its reason that
"Settings already priced the alternative: 1046 and 913 lines of hand-written
AppKit and GTK for a form of checkboxes, and the Windows third is still
unwritten". Those files are 2036 and 1559 lines now, the Windows third is 2896,
and `scripts/verify-settings-macos.sh`, `scripts/ax-settings.swift` and
`scripts/verify-settings-win.ps1` add 1034 lines that exist only because a
native window cannot be asserted on from `node --test`.

Every classifiable Settings bug of the last two months was a renderer bug —
#601, #674, #552, #399, #400, #642, #625 — and none was in the form model. #625
had to be fixed in each renderer. #629, #630 and #631 are one verification epic
per renderer.

`form.rs` describes drawing but not committing: each renderer carries its own
copy of the controller (draft, staged, apply, cancel, shortcut fill), and #534
was a bug in that copy.

The Chat surface is already a Tauri webview (`build_chat`, `main.rs`), themed by
`src/chat-ui.css` under ADR-0019's token seam, tested with `node --test`, and
is what the product's windows are now judged against.

## Decision

Settings is a webview window, `src/settings.html`, on the same stack as Chat:
raw ES modules, no bundler, `chat-ui.css` for tokens, a `settings.css` that
reads only `var(--chat-*)`.

The form stays data. `form::describe()` and the slice of `SettingsView` a tab
reads are serialised to the page by one command, `settings_snapshot`, on open
and on focus. The page renders from that snapshot and nothing else, and a
redraw is a full render, so a `frozen` flag cannot be read at build and
forgotten at draw.

Committing moves out of the renderers into Rust once: `settings_event` takes
`{set_bool | set_text | pick | press}` by row id and folds it into
`SettingsPatch` and the `SettingsSession` operations the three renderers each
called. TCC consent, Keychain, opening and wiping Memory, spawn and dismiss stay
in Rust behind that command; none of them reaches JavaScript.

The page uses semantic HTML only — `<input>`, `<select>`, `<button>`,
`<details>`, `<label for>`, `role="tab"` — so the platform webview exposes it to
Accessibility, and a `controls(tab, view)` function returns the same flat
`{role, id, label, value, frozen}` list `ax-settings.swift dump` reads today, so
the AX assertions become node tests.

The three native renderers and the three verify scripts are deleted once the
webview covers every tab, one live smoke per platform exists, and keyboard-only
operation is checked.

## Considered Options

- **Keep three native renderers and fix them.** The bugs are fixable one at a
  time; #601 and #674 are open and known. Rejected: the record shows the
  fixes are per renderer and the verification is per renderer, and each new
  row type lands three times. The cost is not one bug, it is the multiplier.
- **Native on macOS, webview elsewhere.** Keeps Aqua where the owner lives and
  fixes Windows where it is worst. Rejected: it keeps the multiplier at two,
  keeps the AppKit renderer and its script, and makes the one product look
  like two across machines.
- **A webview in a native shell (native tab bar, web panes).** Rejected: the
  tab bar is the cheapest part to draw and the join is another platform seam.

## Consequences

- One interpreter, one controller, one place a display bug can be. Roughly
  5500 to 6300 fewer lines, and platform-`cfg`'d Settings code shrinks to the
  macOS call that raises the window above the overlay's `NSFloatingWindowLevel`
  panel, which `always_on_top` does not guarantee.
- Settings looks like Chat, and any second `.chat-ui-*` design restyles both.
  On macOS it stops looking like a System Settings pane and stops following
  the OS light mode, because Chat does not either. That is accepted.
- Accessibility changes modality — a web area inside a window rather than
  native controls — but not presence, provided the HTML stays semantic. The
  live check is a condition of deleting the native renderers, not a follow-up.
- Two implementations are alive until Step 9. The default is the webview on
  every platform. `AI_BUDDY_SETTINGS_NATIVE=1` opens a native renderer.
  `AI_BUDDY_SETTINGS_WEBVIEW=1` still selects the webview. Both on: native.
  Native stays frozen to bug fixes. Step 9 deletes the three renderers and
  both flags. This ADR is not Accepted until that delete lands.
- ADR-0010's stance on the stack holds: no build step, no dependency.
  TypeScript is #692 and unaffected.
- `move_drag.rs` and the Win32 hit-test tests go with the renderers; alt-drag
  is `startDragging()` under `core:window:allow-start-dragging`.
