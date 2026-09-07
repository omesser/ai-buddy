# Plan: Settings modifier-drag (#460)

Spec: https://github.com/omesser/ai-buddy/issues/460
Worktree: `.worktrees/feat-460-settings-modifier-drag`
Branch: `feat/460-settings-modifier-drag`
Base: `origin/main` (`d3737da9`)

Dragging a 560pt Settings window by its title bar is fiddly. Holding a
platform modifier and pressing the window background should move it. A press
with no modifier, or a drag that starts inside a control, must keep today's
behavior.

## Spec (binding)

- macOS: Command-drag. `platform/macos/settings_window.rs`. Do **not** call
  `setMovableByWindowBackground(true)` ungated — the form is mostly controls.
- Linux: Super-drag via `gtk_window_begin_move_drag` on a modifier press so
  the gesture exists even when the window manager is not configured for it.
  `platform/x11/settings_window.rs`.
- Windows: Alt-drag. Answer the hit test with `HTCAPTION` for background, or
  forward as `WM_NCLBUTTONDOWN`/`HTCAPTION` so snap stays with the window
  manager. `platform/windows/settings_window.rs`.
- Controls keep the drag: text field, popup, tab strip.
- `docs/DEVELOPMENT.md` names the gesture per platform.

## Global Constraints

- TDD. No production code without a failing test first. Watch it fail for the
  expected reason, then write the minimum code to pass.
- Test at the pre-agreed seam: `crate::settings::move_drag` — pure functions,
  no live `NSWindow` / HWND / GTK widget in `cargo test`. Existing macOS
  settings tests already cannot construct an `NSWindow` off the main thread.
- `should_begin_move(modifier_held, hit)` is the one decision: true only when
  the modifier is held **and** the hit is background.
- Hit is `Background` or `Control`. Tab strip, text fields, popups, buttons,
  and checkboxes are `Control`. Empty document / client background is
  `Background`.
- Platform modifiers (test these as named constants, not magic strings):
  macOS Command, Linux Super, Windows Alt.
- Comments say why, not what (`docs/agents/comments.md`). Issue `#460` is
  enough pointer.
- Conventional Commits, capitalized, no full stop. Scope `settings` when it
  earns its place.
- Pre-commit hooks must pass. Never `--no-verify`.
- Do not change the shared form schema. Do not touch tray/opener/hotkey
  leftovers on #197. Do not invent a fourth modifier or a cross-platform
  identical key.

## Task 1: Move-drag policy module (TDD)

Files:
- Create `src-tauri/src/settings/move_drag.rs`
- Edit `src-tauri/src/settings.rs` to `pub mod move_drag;`

RED, then GREEN, one behavior at a time:

1. `should_begin_move(true, Hit::Background)` is true.
2. `should_begin_move(false, Hit::Background)` is false.
3. `should_begin_move(true, Hit::Control)` is false.
4. `should_begin_move(false, Hit::Control)` is false.
5. Platform modifier names: macOS is Command, Linux is Super, Windows is Alt
   (constants or an enum the platforms will match against — not documentation
   comments alone).

Keep the module tiny. No window types, no OS imports. The platforms consume
this; they do not duplicate the boolean.

Tests live in the same file under `#[cfg(test)]`.

Run: `cargo test -p ai-buddy --lib move_drag` (or the equivalent filter that
hits only this module). Full `cargo test -p ai-buddy` once before commit.

Commit: `feat(settings): Decide when a modifier-drag may move the window`

## Task 2: Wire the three Settings renderers and document the gesture

Depends on Task 1. Consume `settings::move_drag`; do not re-decide the boolean
in each renderer.

### macOS — `src-tauri/src/platform/macos/settings_window.rs`

On Command + background, start a window move (e.g. `performWindowDragWithEvent`
from the event that hit background). A Command-drag that hits a text field,
popup, button, or the tab strip must not start a move.

Do not enable `setMovableByWindowBackground(true)` as the ungated "any
background drag moves it" path. If a helper view is required because
`NSWindowDelegate` has no `mouseDown`, keep it small and local.

Hit classification that can be unit-tested without an `NSWindow` (e.g. "this
class of view is a control") belongs next to the policy or as a small helper
in this file's `#[cfg(test)]` module. Do not construct `NSWindow` in tests.

### Linux — `src-tauri/src/platform/x11/settings_window.rs`

On Super (`GDK_MOD4_MASK` / equivalent) + background, call
`gtk_window_begin_move_drag`. Do not swallow a press that belongs to an
entry, text view, button, combo, checkbox, or the notebook tab strip.

GTK 3. The window is already a `gtk::Window`.

### Windows — `src-tauri/src/platform/windows/settings_window.rs`

On Alt + background, treat the press as a caption drag (`HTCAPTION` via
`WM_NCHITTEST` and/or `WM_NCLBUTTONDOWN`) so Aero snap stays with the
window manager. A press with no Alt, or over a child control (edit, button,
tab), stays `HTCLIENT` / today's `window_proc`.

A pure helper that maps `(alt_held, hit)` onto the Win32 hit-test code is
in-bounds and should be tested without a live HWND.

### Docs — `docs/DEVELOPMENT.md`

Under **Settings and Keyring**, name the gesture per platform. One short
paragraph is enough: Command-drag on macOS, Super-drag on Linux, Alt-drag
on Windows, from the window background, not from a field. It is not
discoverable, which is why it is written down.

### Tests

Re-run `move_drag` tests. Add the platform helper tests above. Run
`cargo test -p ai-buddy` once before commit. Run
`pre-commit run --files <touched>` before commit.

Commit: `feat(settings): Move the window with a modifier-drag on its background`

Closes #460 in the commit body or the later PR, not both unless the PR is
the one that lands.
