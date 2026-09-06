# The Harness runs headless and ai-buddy draws the chat surface

The attached Harness is spawned as a subprocess in ACP mode, where it renders
nothing. The chat surface #17 opens is ours: a Tauri webview window ai-buddy
builds. We never launch, embed, or wrap the Harness's own TUI, even though
that UI already solves everything ours has to.

ACP mode and interactive mode are two modes of one process. `opencode acp`
speaks newline-delimited JSON-RPC on stdio and draws nothing; `opencode`
draws a TUI and speaks no protocol. So showing the user the real Harness
interface costs a second process, and a second process is a second
conversation — the split brain ADR-0008 forbids. Only opencode escapes that,
because `opencode serve` and `opencode attach --session` are two clients of one
    20|session store. Claude Code, Hermes, Pi and Grok Build have no equivalent. A
capability four of five Harnesses lack cannot be the shape of the feature.

## What we draw, and what we do not

A log-style chat: the user's turns, the agent's text as it streams, tool calls
as one-liners, and the forwarded `session/request_permission` prompt. Not a
workbench. No diff view, file tree, plan panel, skill browser or settings pane
— those are the Harness's own surfaces, and what ai-buddy needs to remember of
them belongs in the Action Log.

One thing more: a status bar under the log, drawing what the Spatial Layer is
doing — the Behavior, Primitive and Animation being played, the State under
them, which way the sprite faces, what the last wake was told had happened, and
how long until the next one. That is not the workbench this section refuses.
What it refuses is the *Harness's* surfaces: a diff, a file tree, a plan, a
skill browser are all drawn better by the TUI we chose not to embed, so
rebuilding them here makes a worse copy of it. The bar draws state ai-buddy
itself owns, which no Harness surface carries and which nothing else in the
product shows.

The line, for the next pull request that wants a sixth thing in this window:
the bar may say what our own layers are doing right now, in one line, with
nothing to operate. Anything the user acts on, anything with a history to
scroll or a filter to set, and anything the Harness reported rather than the
Engine played, is the workbench arriving by another door.

The log takes the same widening. Every response the session Director produces
appears here, ambient as much as reactive — a line said in the Speech bubble
and nowhere else leaves one conversation with two places to read it and this
log missing the half the user did not type. One the user did not type is
labelled with what the Director was reacting to, in the status bar's own words:
a Summon and a Poke are prompts too, and only the ambient wake is unprompted.
A response that proposes a Behavior and no Speech has no line to draw; the bar
above names the Behavior, and nothing is held for a surface that opens later.

It is a webview rather than a native window because Settings already priced
the alternative: `platform/macos/settings_window.rs` and
`platform/x11/settings_window.rs` are 1046 and 913 lines of hand-written
AppKit and GTK for a form of checkboxes, and the Windows third is still
unwritten (#197). Chat is also a text-layout problem — wrapping, scrollback,
selection, streaming append — which is the one thing a webview is. `docs/SPEC.md`
already assigns the webview to the sprite and the chat surface.

Vanilla JavaScript, not React. `package.json` declares no dependencies and the
project has no build step; one window does not justify a bundler and
    40|`node_modules`.

## Considered Options

- **Embed a terminal and run the Harness TUI inside it.** `xterm.js` over
  `portable-pty` is a solved, cross-platform, few-hundred-line job, and it
  inherits permissions, cancellation, plans and diffs on all five Harnesses
  instead of rebuilding them. Rejected on two counts. The buddy would have to
  start a turn by typing into the PTY, which races with whatever the user is
  typing and yields no signal that the turn finished. And a black terminal of
    50|  file paths is a poor thing for a Windows-95 mascot to open when you
  double-click it. Kept on the shelf: if drawing a permission prompt proves
  worse than it looks, this is the fallback.
- **ACP, plus an optional button that opens the Harness TUI.** The button is a
  second process, so on four of five Harnesses it opens a different
  conversation from the one animating the sprite.
- **Attach to a Harness the user is already running and prompt a child session
  under theirs.** opencode alone supports it. Its child sessions start with
  fresh context, so a child per message is ADR-0008's split brain with extra
  steps; one long-lived child fixes that and still binds the buddy to whichever
    60|  repository the user happened to have open, and drops ambient wakes into the
  session list of their real work.
- **A Rust-native chat window** in egui, iced or Slint. A third UI toolkit
  beside the webview and the two native Shells, and hand-rolled text layout, to
  avoid a dependency the app already ships.

## Consequences

We own the permission prompt. Forwarding the Harness's own
`session/request_permission` is not the second confirmation ADR-0003 refuses —
    70|the rule is that ai-buddy never *answers* it, and never adds one of its own.

The `Completer` seam is filled by one ACP `session/prompt` per wake. Two
dependencies would carry most of that: `acp-cli` is a Rust crate whose
`AcpBridge` is the ACP client and which already knows the launch command for
seventeen agents, and `@hafbit/acp-components-core` is framework-agnostic
JavaScript holding the session, streaming and permission state. Both are
pre-1.0 and neither has an unambiguous canonical home. Reviewed in ADR-0017:
neither was adopted.

The Harness is spawned with its working directory at `~/.ai-buddy`, not the
user's project, so nothing the buddy says is flavoured by a repository it was
    80|never asked about.

Attachment is opt-in, and settings says the price in words: ai-buddy spawns and
holds a full agent process for as long as the app runs. Static weights and the
HTTP Completer stay the path for everyone who declines.

The spawned Harness authenticates itself, and ai-buddy holds no credential for
it (#371). All five keep their credential in a home-relative file or the OS
keychain and read it at process start, so a CLI the user has already logged in
to hands its login to a subprocess we spawn as the same user. Anthropic permits
that shape by name and forbids the alternative: an end user may sign in to the
unmodified Claude Code binary with their own subscription, and a third-party
developer may not collect, store, or intermediate Claude.ai credentials.
`SecretStore` also fits the Director key for a reason that does not carry over
— ai-buddy *is* the HTTP client there, and with a Harness the Harness talks to
the provider while we talk ACP over a pipe. A credential we store and never
send buys nothing.

So `director-api-key` stays the only account ai-buddy owns, and a pull request
touching attachment may not:

1. Prompt for a Harness password, token, or API key.
2. Read another tool's credential file or keychain item.
3. Copy a Harness credential into `SecretStore`.
4. Set a provider key into the child environment on the user's behalf.
   `ANTHROPIC_API_KEY` overrides a subscription login with no prompt in
   non-interactive mode, which bills a subscriber twice without saying so.
5. Set `CLAUDE_CONFIG_DIR` or pass `--bare`. Both cut the child off from the
   login the user already has.
6. Render, proxy, or intermediate a vendor login. Name the command and let the
   user run it in their own terminal.
7. Log, print, or fingerprint a Harness credential.
8. Modify, repackage, or wrap a Harness binary, or disable one of its
   authentication methods.

The Chat surface therefore distinguishes three states rather than two. Not
attached is the `empty-none` copy it already draws. Attached but not
authenticated names the one command that fixes it, read from ACP's
`authMethods` and `auth_required` where the Harness implements them and from a
pre-flight probe where it does not. Attached and refused is a failed turn,
which ADR-0008 already routes to `StaticDirector`.

Reversing this means writing a terminal host and giving up the turn-completion
signal, or keeping ACP and accepting two conversations.
