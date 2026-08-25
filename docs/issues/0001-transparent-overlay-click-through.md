# 0001 — Transparent overlay with alpha hit-testing

## Why

Click-through is per-window, not per-pixel, in Tauri. A small sprite in a large
transparent window swallows clicks across the whole rectangle. This is the first thing
that can make ai-buddy look broken, and it gates every other visual decision.

## Scope

A transparent, always-on-top Tauri overlay window that renders a placeholder sprite and
passes clicks through everywhere the sprite is not.

- Non-activating panel at floating level on macOS.
- Joins all Spaces, stationary, excluded from the application switcher.
- Never accepts first responder status or steals keyboard focus.
- Cursor tracked; ignore-mouse-events toggled by hit-testing the sprite's current alpha.

[WindowPet](https://github.com/SeakMengs/WindowPet) is MIT and its implementation is the
reference. Lift it and record the attribution in the README.

## Acceptance criteria

- Clicking transparent area reaches the window underneath.
- Clicking the sprite does not reach the window underneath.
- Typing into another application is never interrupted by the overlay.
- The overlay does not appear in the application switcher.
- The overlay follows the user across Spaces.
- Verified by hand against overlapping windows on more than one display.

## Tests

Manual. Rendering and panel configuration are thin, platform-specific, and expensive to
fake convincingly. Record the manual check in the PR.
