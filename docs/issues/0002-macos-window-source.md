# 0002 — macOS WindowSource and platform capabilities

## Why

The Spatial Layer needs to know where windows are. It must learn this without triggering
a permission prompt, because first run grants nothing.

## Scope

`WindowSource`, a trait producing the geometry half of a `WorldSnapshot`, plus its macOS
implementation.

- Polls `CGWindowListCopyWindowInfo` at approximately 10Hz.
- Returns visible window bounds, owning application name, and layer, in descending
  z-order.
- Does **not** read window titles. Titles require Screen Recording consent and are not
  used in v1.
- Enumerates display frames for the multi-monitor coordinate space.

Each platform implementation declares its capabilities rather than assuming them.
`window_geometry` and `absolute_positioning` are declared capabilities. Under Wayland both
are unavailable and the Spatial Layer degrades to screen-edge physics only — a supported
mode, not an error.

Windows is stubbed behind the same trait.

## Acceptance criteria

- No permission prompt appears on first run.
- Window bounds update as windows are moved, resized, opened, and closed.
- A platform reporting no `window_geometry` capability yields snapshots with display
  frames and no window rectangles.
- Polling cost is not visible in Activity Monitor at idle.

## Tests

A fake `WindowSource` for Engine tests. The macOS implementation itself is verified by
hand.
