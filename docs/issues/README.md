# ai-buddy v1 — issues

Work items for the v1 scope defined in [SPEC.md](../SPEC.md). Vocabulary is
[CONTEXT.md](../../CONTEXT.md); decisions are [DESIGN.md](../../DESIGN.md) and
[docs/adr/](../adr/).

Build order runs roughly top to bottom. `0001` gates every visual decision and is worth
doing before anything else.

| # | Title | Depends on |
|---|-------|-----------|
| [0001](./0001-transparent-overlay-click-through.md) | Transparent overlay with alpha hit-testing | — |
| [0002](./0002-macos-window-source.md) | macOS WindowSource and platform capabilities | — |
| [0003](./0003-engine-core.md) | Engine core: snapshot, frame, State machine | — |
| [0004](./0004-physics.md) | Physics: gravity, throw, bounds, multi-monitor | 0003 |
| [0005](./0005-perch-collision.md) | Perch collision on window top edges | 0003, 0004 |
| [0006](./0006-interaction-verbs.md) | The five interaction verbs | 0001, 0003 |
| [0007](./0007-character-package.md) | Character Package format, loader, validation | 0003 |
| [0008](./0008-primitives-and-behaviors.md) | Primitives and declarative Behaviors | 0003, 0007 |
| [0009](./0009-shipped-characters.md) | Two shipped Characters | 0007, 0008 |
| [0010](./0010-static-director.md) | Static Director | 0008 |
| [0011](./0011-model-director.md) | Model-backed Director | 0010 |
| [0012](./0012-hide-rules-and-z-order.md) | Hide rules and z-order | 0001 |
| [0013](./0013-character-instances.md) | Character Instances | 0007, 0012 |
| [0014](./0014-memory.md) | Memory store | — |
| [0015](./0015-mcp-server.md) | MCP server and tool surface | 0013, 0014 |
| [0016](./0016-harness-attach.md) | Harness attachment and action log | 0015 |
| [0017](./0017-chat-surface.md) | Chat surface | 0016 |
| [0018](./0018-shell-tray-settings.md) | Tray, settings, autostart, updater | 0012, 0013 |
