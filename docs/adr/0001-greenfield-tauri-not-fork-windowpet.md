# Build greenfield on Tauri rather than forking WindowPet

[WindowPet](https://github.com/SeakMengs/WindowPet) is MIT-licensed, built on Tauri
and React, runs on all three platforms, and already solves click-through,
pixel-perfect drag, tray, autostart, and auto-update. It has no physics, no window
awareness, and no model. We start clean on Tauri anyway, because the work that makes
ai-buddy distinct — window-edge collision, the Director, the Character Package — all
replaces WindowPet's central loop, and gutting the centre of a codebase is slower than
starting from a good reference. Its click-through hit-testing and tray/updater code
are lifted directly under MIT, with attribution.

## Considered Options

- **Fork WindowPet** — a working overlay on day one, at the cost of inheriting React
  and state conventions built around "render a sprite where the user dropped it."
- **Electron** — same webview model, roughly 150MB of binary and 100–200MB resident.
  A permanent argument for a program whose pitch is "always there, costs you nothing."
- **Native per platform** — best behavior and footprint, three codebases.
- **Godot** — strong at sprite animation and state machines, awkward foundation for
  the Functional Layer.
- **MOD on [desktop-homunculus](https://github.com/not-elm/desktop-homunculus)** — the
  largest shortcut on model wiring, but it means inheriting Bevy, 3D VRM, an alpha API
  that may change without notice, and shipping a plugin inside someone else's product.

## Consequences

Click-through is per-window, not per-pixel, in both Tauri and Electron. A small sprite
in a large transparent window swallows clicks across the whole rectangle unless the
cursor is tracked and ignore-mouse-events toggled by hit-testing the sprite's alpha.
Budget a day, and use WindowPet's implementation as the reference.
