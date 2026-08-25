# 0012 — Hide rules and z-order

## Why

A companion that knows when to disappear is the difference between a pet and malware. The
investment goes into hiding, not into stacking cleverness.

## Scope

One fixed window level, always on top. Restacking by sprite state is rejected — it
produces flicker on every platform, and peeking out from behind windows is given up
deliberately. See [DESIGN.md](../../DESIGN.md) decision 8.

Hiding is implemented as visibility rules:

- A fullscreen application is frontmost.
- Screen sharing is active.
- Do Not Disturb is on.
- A global hotkey toggles hide and show.

## Acceptance criteria

- The buddy fades out when a fullscreen application takes the front.
- The buddy is not visible during screen sharing.
- The buddy is quiet under Do Not Disturb.
- The hotkey hides and shows instantly.
- No flicker or restacking is observable during normal window switching.

## Tests

Manual, on a real machine, including a real screen share.
