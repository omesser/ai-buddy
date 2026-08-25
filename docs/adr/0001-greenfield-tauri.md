# Greenfield Tauri app, with WindowPet as reference implementation

Two mature open-source projects overlap ai-buddy: WindowPet (MIT, Tauri + React,
already solves transparent overlays, per-pixel click-through, tray, autostart,
updater) and desktop-homunculus (MIT/Apache, Bevy, 3D VRM, MCP server for AI
control). We are building greenfield on Tauri anyway, reading WindowPet as a
reference and lifting its click-through hit-testing and tray/updater code with
attribution.

## Considered Options

- **Fork WindowPet.** Working overlay on day one — but its core loop is "render a
  sprite where the user dropped it," and we replace that centre entirely with
  physics, Perch collision, and a Director. Forking a project to gut its centre
  is slower than starting clean with its source open in a tab, and we would
  inherit its React and state conventions for a fundamentally different loop.
- **MOD on desktop-homunculus.** Largest shortcut on AI wiring — but we would
  inherit Bevy, 3D VRM rendering, an explicitly alpha API that "may change
  without notice," and the position of being a plugin inside someone else's
  product rather than owning one.
- **Electron.** Fastest to a moving sprite, but ~150MB+ binary and 100–200MB RSS
  is a permanent tax on a product whose pitch is "always on your desktop, costs
  you nothing."
- **Pure native per-platform.** Best behavior and footprint, three codebases,
  contradicts the cross-platform requirement.
- **Godot.** Purpose-built for sprite animation, but an odd foundation for the
  Functional Layer, and only wins if the character system is more complex than
  the agent system long-term — which it is not.

## Consequences

The hard platform work (window enumeration, accessibility, global input hooks,
tray) lands in Rust, which is where it belongs; sprite animation stays in the
webview, which does it trivially. Per-pixel mouse hit-testing is ours to
implement regardless of stack — budget a day, not an afternoon.
