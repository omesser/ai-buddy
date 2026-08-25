# ai-buddy ships no Executor; computer use is delegated over MCP

ai-buddy is an MCP server (exposing `speak`, `play_behavior`, `list_windows`,
`describe_screen`) and an MCP host (attaching whatever Harness the user
configures). It contains **no code that posts synthetic mouse or keyboard
events**. Actions come from the Harness's native computer use, or from a
user-configured desktop-control MCP server.

A future reader will find an app that advertises controlling your computer and no
executor anywhere in it. That is deliberate.

## Considered Options

- **Embed an agent SDK and write the Executor.** At the API level, computer use is
  reasoning only — the model returns `{action: "left_click", coordinate: [x, y]}`
  and the *client* executes it. The reference implementation drives X11 with
  `xdotool` in Docker; macOS would be ours to write with `CGEvent`/`cliclick`.
  Full loop control, but we own an input-injection layer and its permission
  surface.
- **Spawn Claude Code as a subprocess.** Since March 2026, Claude Code and Claude
  Cowork do computer use natively on macOS and Windows, no setup. Free
  capability — but it is a research preview requiring a Pro or Max plan, it owns
  the loop and its own permission dialogs, and it is Anthropic-specific.
  Depending on it makes our headline layer-2 feature a function of someone else's
  subscription tier.
- **Bundle a third-party desktop-control MCP server.** In-box actions, at the cost
  of shipping unvetted third-party code that has full keyboard and mouse control
  inside our trust boundary. Rejected outright: the worst possible place to
  inherit a CVE.

## Consequences

This deletes the largest chunk of platform-specific, permission-heavy,
liability-carrying code from v1, and it is honest to the staged release plan —
actions are a v2 feature, not the product.

There is no vendor lock-in: desktop control is satisfiable by any of several
open-source MCP servers (`computer-use-mcp`, `mac-use-mcp`, `MacOS-MCP`,
`computer-use-mac-mcp`), with any model, from any MCP-capable Harness. Claude
Code's native version is a zero-config fast path, not a dependency.

The known cost is permission hygiene. On macOS the Accessibility grant attaches
to the process posting events, so an `npx`-launched MCP server has the user
granting Accessibility to *node* or *Terminal* — confusing, and it leaks the
grant to everything else running there. The eventual argument for a thin
first-party Rust Executor (~300 lines of `CGEvent` + `ScreenCaptureKit`) is that
grant reading cleanly as "ai-buddy" — capability is not the reason, and it stays
on the shelf until in-box actions are a headline feature.
