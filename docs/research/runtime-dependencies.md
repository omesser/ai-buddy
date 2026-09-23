# Runtime dependencies beyond the binary

Research for issue #735 — what ai-buddy needs beyond the compiled binary to run, and whether each dep should be self-contained or declared. Dated 2026-09-23. Motivating bug: #726 (npx missing → silent Codex fail).

## What this note covers

Census of runtime deps beyond the ai-buddy binary. For each: why needed, how detected today, required vs optional, self-contain options, and recommendation (keep external and declare, bundle, or eliminate). Followed by failure UX sketch and follow-up issues.

## Census table

| Dependency | Why Needed | Detection Today | Required vs Optional | Self-Contain Options | Recommendation |
|---|---|---|---|---|---|
| **Node.js + npx** | Launch registry ACP adapters for Claude / Codex / Pi (npx -y @agentclientprotocol/claude-agent-acp@latest, etc.) | `spawn` fails → reports argv[0] missing → logs to stderr; Chat shows `*- not running` silently (#726) | Optional. Required only for Claude/Codex/Pi presets when user picks them. First-party ACP harnesses (hermes, opencode, grok, cursor-agent) need zero Node. | (1) Bundle Node runtime + pinned adapter trees (~50–100MB+, licenses become ours, update cadence ours); (2) prefer/default first-party ACP where available; (3) declare loudly when missing. | **Declare.** Accept external dep for Claude/Codex/Pi. Never silent. Fix #726 loud failure first. Optional follow-up: spike bundling Node + adapters if install-Node is unacceptable. |
| **Harness CLI on PATH** | First-party ACP: hermes (`hermes acp`), opencode (`opencode acp`), grok (`grok agent stdio`), cursor-agent (`cursor-agent acp`). Binary-distribution registry agents: antigravity-acp (agy_acp_server), goose, copilot, junie, kimi, and 14 more. | Same as npx: `spawn` fails, argv[0] reported missing, stderr line, silent Chat `*- not running`. | Optional. Required only when user picks that harness. Default Character+StaticDirector needs no harness. | (1) Bundle the CLI binaries (licenses, sizes, update cadence); (2) declare and present install steps; (3) named presets for high-value zero-npx agents; (4) custom argv escape hatch (already exists). | **Declare per preset.** Named presets for hermes/opencode/grok/cursor-agent stay as-is (PATH binary). High-value additions to consider: goose, copilot (#457), antigravity-acp (#604 refresh). Custom argv already covers long tail. Present install steps in Settings/landing when missing. |
| **Platform WebView** | Tauri app shell requires native webview for Chat and Settings windows. | Implicit. On macOS: system WebKit (shipped). On Windows: WebView2 (user-installed or bundled via evergreen bootstrapper). On Linux: WebKitGTK (`libwebkit2gtk-4.1`) must be installed. | Required for Chat / Settings surfaces. Character+overlay alone can run without webview (hypothetically), but product includes Chat. | (1) Bundle/bootstrap on Windows (Tauri does this via WebView2 evergreen); (2) declare on Linux; (3) macOS ships WebKit. | **Declared on Linux only.** README already lists libwebkit2gtk-4.1-dev for build; runtime .deb pulls it as dep. AppImage: user must install libwebkit2gtk. Windows: Tauri WebView2 bootstrapper handles it. macOS: system WebKit. |
| **GStreamer (Linux)** | Cue audio playback on Linux (crate `gstreamer`, gst-plugins-base). macOS uses AVFoundation. Windows uses platform audio APIs. | Graceful. Audio cue plays or silently skips if GStreamer unavailable; no crash, no error surface. | Optional. Buddy works without sound. Cue audio is personality flavor, not functional requirement. | (1) Bundle GStreamer libs; (2) declare in README/install; (3) leave optional / best-effort. | **Declare, keep optional.** README notes GStreamer for Linux audio. No loud failure; silent skip is acceptable for optional audio. |
| **libfuse2 / libfuse2t64 (Linux AppImage)** | AppImage runtime requires FUSE to mount the squashfs payload. | External. User sees "cannot execute" or "FUSE not available" from AppImage runtime, not from ai-buddy. | Required for AppImage distribution only. .deb does not need it. | (1) Migrate to AppImage type 2 with bundled runtime (still needs kernel FUSE or `--appimage-extract`); (2) declare in README; (3) offer .deb as FUSE-free path. | **Declare for AppImage.** README already notes libfuse2 / libfuse2t64. .deb is the FUSE-free alternative. Keep both distributions. |
| **X11 vs Wayland (Linux)** | Overlay window positioning, Perch on windows, per-window metadata. Wayland restricts these (no arbitrary positioning, no reading other windows). X11 allows full feature set. | Implicit. Wayland detected at runtime → overlay keeps to screen edges, Perches disabled. X11 → full features. No error; supported Wayland mode. | Optional feature set difference, not a dep. Both envs work; Wayland is more restricted. | N/A. Desktop environment choice, not a bundling decision. | **Document.** README already says "Under Wayland the sprite keeps to screen edges and loses window Perches — a supported mode, not an error. X11 gets both." No change. |
| **MCP stdio binary (AI_BUDDY_MCP_BIN)** | When BYO harness registers ai-buddy as MCP server via stdio shim, the shim spawns this binary to bridge stdio ↔ loopback HTTP. | Environment variable AI_BUDDY_MCP_BIN. If unset or binary missing, BYO harness stdio path fails. HTTP path (Authorization header) works without it. | Optional. Only needed for BYO harness stdio transport. HTTP transport with bearer token works for all five tested harnesses without stdio binary. | (1) Bundle shim binary beside app; (2) Settings snippet defaults to HTTP path (already preferred); (3) stdio as fallback/advanced. | **Keep external, document HTTP as primary.** Research already concluded HTTP+bearer is the primary BYO path for all harnesses; stdio shim is fallback. No bundling needed; Settings generates HTTP snippets. |

## Synthesis: what ai-buddy needs on top of the binary

**Measured from code and registry** (2026-09-23):

Registry total: 41 agents.
Distribution: 19 binary, 20 npx, 2 uvx.

ai-buddy `launch()` today (harness.rs lines 101-134):
- `claude`: `npx -y @agentclientprotocol/claude-agent-acp@latest`
- `codex`: `npx -y @agentclientprotocol/codex-acp@latest`
- `pi`: `npx -y pi-acp@latest`
- `cursor-agent`: `cursor-agent acp`
- `grok`: `grok agent stdio`
- `hermes`: `hermes acp`
- `opencode`: `opencode acp`
- custom: split whitespace, user-supplied argv

**Inference:** Three presets need npx. Four need PATH binary (but not npx). Many registry binary agents (goose, copilot, antigravity-acp, junie, kimi, 14 more) have no named ai-buddy preset yet but could.

**Fact:** Registry `distribution.npx` means "install via npx package," not "must run under npx forever." After global install or brew, many agents become PATH binaries (measured: gemini, copilot, grok/opencode already have PATH forms; registry lists npx install flavor). ai-buddy already uses PATH for grok/opencode.

**Inference:** The npx dep is not universal. It is specific to three registry adapters (Claude/Codex/Pi) when the user picks those presets and Node is not installed. First-party ACP harnesses and binary-distribution agents avoid npx entirely.

## Call: self-contained vs declare for each dependency

### Node.js + npx

**Call: Declare.**

Accept external dep for Claude/Codex/Pi presets. Install Node (https://nodejs.org/), ensure npx on PATH. Loud failure when missing (fix #726 first). Never silent `*- not running`.

**Reasoning:** (1) Three of seven named presets need it; four do not. (2) Bundling Node is 50–100MB+, licenses, and update cadence. (3) Many users already have Node. (4) Preferring first-party ACP (hermes, opencode, grok, cursor-agent) over npx adapters is product/UX, not shipping Node.

**Optional spike:** Bundle Node + pinned claude-agent-acp / codex-acp / pi-acp trees. Size/license table. Eliminates network/npx cold start. Only if "install Node" is unacceptable compromise.

**Do not:** Vendor acpx. Wrong layer (measured: acpx still calls npx for Claude/Codex/Pi adapters; does not remove Node dep). See architect comment 2026-09-17.

### Harness CLIs on PATH

**Call: Declare per preset.** Present install steps in Settings/landing when selected harness CLI is missing.

Named presets stay as-is: hermes, opencode, grok, cursor-agent. High-value new named presets to consider: goose, copilot (#457), antigravity-acp (#604 refresh with registry binary). Custom argv already exists as escape hatch for long tail (kimi, junie, kiro, iflow, stakpak, etc.).

**Reasoning:** Each is a separate product. Bundling 5+ harness binaries for "pick one" is larger than declaring "install the one you pick." Registry already has 19 binary agents; we cannot bundle all.

**Detection improvement:** Loud failure when selected CLI not on PATH. Same fix as npx (#726).

### Platform WebView

**Call: Declared on Linux only.**

macOS: system WebKit (shipped).
Windows: Tauri WebView2 evergreen bootstrapper handles it (user-installed or bundled).
Linux: libwebkit2gtk-4.1 (runtime) must be installed. README already notes libwebkit2gtk-4.1-dev for build; .deb package pulls runtime dep automatically. AppImage: user must install libwebkit2gtk-4.1 (or libwebkit2gtk-4.1-0 on older distros).

**Reasoning:** Tauri chose WebView2 bootstrapper strategy for Windows; we inherit that. macOS WebKit is system. Linux is the only platform where the dep is external and user-installed, but it is standard desktop library.

**No change needed.** Already documented.

### GStreamer (Linux audio)

**Call: Declare, keep optional.**

README notes GStreamer (gst-plugins-base) for Linux Cue audio. No loud failure; silent skip is acceptable for optional audio. Buddy works without sound.

**Reasoning:** Audio is personality flavor, not functional requirement. Graceful degradation (silent skip) is better UX than loud failure for an optional feature. Many users have GStreamer installed (common desktop dep).

**No change needed.** Already documented in DEVELOPMENT.md.

### libfuse2 / libfuse2t64 (Linux AppImage)

**Call: Declare for AppImage.**

README already notes libfuse2 (Ubuntu 22.04) or libfuse2t64 (24.04+). .deb is the FUSE-free alternative.

**Reasoning:** AppImage runtime requires FUSE to mount squashfs. Two distributions (AppImage + .deb) cover both "user wants portable bundle" and "user wants native package manager." Cannot eliminate FUSE from AppImage without migrating to type 2 + `--appimage-extract` fallback (out of scope for this spike).

**No change needed.** Already documented.

### MCP stdio binary (AI_BUDDY_MCP_BIN)

**Call: Keep external, document HTTP as primary.**

BYO harness registration via HTTP + bearer token is the primary path for all five tested harnesses (claude, hermes, opencode, grok, codex). Stdio shim is fallback/advanced. No bundling needed.

**Reasoning:** Research (docs/research/byo-harness-mcp-registration.md) concluded HTTP path works for all, stdio costs "absolute path to a binary the user has to locate, and buys nothing the header route does not already give." Settings snippets default to HTTP.

**No change needed.** Already documented and implemented.

### uv / uvx (for registry uvx agents)

**Call: Out of scope for V1.**

Registry has 2 uvx agents (fast-agent, minion-code). Same "declare the runner" problem as npx, different toolchain (Python uv instead of Node). No named ai-buddy presets for uvx agents yet. Custom argv escape hatch already covers them if user has uv installed.

**Reasoning:** Same as npx: bundling a package runner for 2 agents is heavier than declaring "install uv if you want fast-agent." Defer until user demand or named preset for one of them.

**No follow-up issue.** Revisit when/if uvx agent becomes a named preset candidate.

## Failure UX sketch when declared dep is missing

### Problem: #726

Landing "Codex" button looked successful. Chat header showed `codex - not running`. Console logged `` `npx` is not installed ``. User saw silent dead session.

**Root cause:** `spawn` fails when argv[0] not on PATH. Rust reports OS "command not found" to stderr. Harness attach shows "not running" in header but never surfaces why. Landing dismisses itself and user lands in dead Chat.

**Principle:** Never silent `*- not running`. If connect attempt fails, user must see why and what to install.

### Proposed failure UX (sketch, not implementation)

When spawn of selected harness fails (npx, or any PATH binary):

1. **Detect argv[0] missing at spawn.** Already happens (measured: harness.rs reports argv[0] in note_missing).
2. **Landing does not dismiss.** Stay on "Connect a Harness to get started" landing.
3. **Toast or inline error below button:** "`npx` is not installed. Install Node.js from https://nodejs.org/ to use Claude/Codex/Pi." (or equivalent for other CLIs: "Install Hermes from https://… to use Hermes.")
4. **Chat header shows `<harness> - not running (missing <tool>)`** if user somehow bypassed landing (e.g. env var set to codex, no npx).
5. **Retry when fixed.** After user installs the dep, pressing Connect button again should work (no restart required, or restart prompt if lazy path).

**Variations per dep:**

- **npx:** "Install Node.js from https://nodejs.org/ (includes npx) to use Claude, Codex, or Pi."
- **hermes CLI:** "Install Hermes from https://hermes-agent.nousresearch.com/ to use Hermes."
- **opencode CLI:** "Install OpenCode from https://opencode.ai/ to use OpenCode."
- **grok CLI:** "Install Grok from https://grok.com/… to use Grok." (URL TBD)
- **cursor-agent:** "Install Cursor Agent from https://cursor.com/… to use Cursor Agent." (URL TBD)
- **goose / copilot / antigravity-acp / etc.:** Same pattern, with correct install URLs.

**Implementation out of scope for this spike.** Follow-up issue: #726 if still open/unfixed, or new "present harness install steps when CLI missing" ticket.

### Optional deps (no loud failure)

**GStreamer (Linux audio):** Silent skip is acceptable. No error surface. README already documents.

**WebView on Linux:** If libwebkit2gtk missing, app fails to start (Tauri/GTK error, not ai-buddy-specific). No special handling needed beyond .deb dependency and README note for AppImage users.

**FUSE for AppImage:** AppImage runtime handles the error ("FUSE not available"). README already documents. .deb is FUSE-free alternative.

## Named preset candidates (zero-npx, not yet in ai-buddy)

**Measured:** Registry has 19 binary-distribution agents. ai-buddy has named presets for 4 of them (cursor-agent, opencode, grok via PATH form, hermes). The rest require custom argv.

**High-value candidates for new named presets** (from architect deep pass + registry):

1. **goose** (`goose acp`) — Block/open-source, strong "local agent" candidate.
2. **copilot** (`copilot --acp --stdio`) — GitHub Copilot. Issue #457 closed without named row; revisit.
3. **antigravity-acp** (`agy_acp_server.par` / `.exe`) — Google's official ACP server for Antigravity. Registry v1.1.1 binary on dl.google.com. Updates #604 (premise "agy has no ACP" now stale; official binary adapter exists).
4. **junie** (`junie --acp=true`) — JetBrains.
5. **kimi** (`kimi acp`) — Moonshot.

**Do not add:** `gemini --acp` (Gemini CLI is sunset; Google path is Antigravity). Noted in architect correction 2026-09-17.

**Follow-up issue:** "Evaluate named presets for goose, copilot, antigravity-acp" (or split per agent).

## Follow-up issues to file

Based on Done-when checklist and this research:

1. **#726 (if still open): Loud failure when npx/harness CLI missing.** Landing stays visible, toast/error shows which dep to install, Chat header explains `*- not running (missing <tool>)`. Covers npx and all PATH harness CLIs.

2. **Optional spike: Bundle Node + pinned ACP adapters.** Self-contain option for Claude/Codex/Pi. Size/license table. Eliminates npx dep and network cold start. Only if "install Node" is unacceptable. Decision input for V1 vs declare-only.

3. **Settings/landing copy: Distinguish first-party ACP vs npx-adapter presets.** When presenting harness options, clarify which need Node (Claude/Codex/Pi) vs which are standalone CLIs (hermes/opencode/grok/cursor-agent). Helps user choose zero-dep option when possible.

4. **Named presets evaluation: goose, copilot (#457), antigravity-acp (#604).** Add `launch()` rows + landing buttons for high-value zero-npx agents. Includes smoke test via probe-harness.sh. Links to install URLs in error messages.

5. **(Optional) ADR or docs: Runtime dep philosophy.** Document the "self-contain vs declare" decision framework for future deps. Criteria: size, licenses, update cadence, user install base, required vs optional.

## Open questions for Oded

1. **Node bundling decision:** Is V1 willing to ship a Node runtime + adapter tree for Claude/Codex/Pi (~50–100MB+, licenses, update cadence), or is "install Node" + loud error the acceptable compromise?

2. **Named preset prioritization:** Which of goose / copilot / antigravity-acp / junie / kimi deserve named `launch()` rows + landing buttons vs stay in custom argv escape hatch?

3. **Failure UX details:** Toast vs inline error below Connect button? Dismiss landing on failure or keep it visible? Retry without restart or prompt user to restart ai-buddy?

4. **npx alternatives:** If Node install is unacceptable, explore first-party Claude/Codex ACP binaries (if/when Anthropic/OpenAI ship them) to replace Zed npm adapters? Or is that a post-V1 conversation?

## Claims and evidence labels

**Measured** (from code/registry/live tests):
- Registry v1.0.0 has 41 agents: 19 binary, 20 npx, 2 uvx.
- ai-buddy launch() lines 101-134: 3 npx presets (claude/codex/pi), 4 PATH presets (cursor-agent/grok/hermes/opencode), 1 custom.
- #726 repro: npx missing → stderr `` `npx` is not installed ``, Chat header `codex - not running`, no error surface.
- Architect comments dated 2026-09-17 on #735 (npx vs acpx, deep pass of registry agents, Gemini sunset correction).
- antigravity-acp v1.1.1 in registry with binary distribution (agy_acp_server.par/.exe on dl.google.com).

**Inferred** (from code patterns/docs):
- Registry npx means "install via npx," not "must run under npx forever." Many become PATH binaries after install (grok, opencode, gemini, copilot observed).
- Vendoring acpx does not remove npx dep (acpx still spawns npx for Claude/Codex/Pi adapters per docs).
- Bundling Node is 50–100MB+ (typical Node runtime + node_modules for 3 packages).
- WebView2 bootstrapper on Windows is Tauri's choice; ai-buddy inherits that strategy.

**Guess** (stated assumptions):
- User install base for Node is "many users already have it" (no hard data; common dev tool).
- GStreamer is "common desktop dep" on Linux (no hard data; ships with many distros for media playback).
- First-party Claude/Codex ACP binaries may exist someday (Anthropic/OpenAI have not announced them; Zed npm adapters are current path).

## Summary

ai-buddy runtime deps beyond the binary: Node+npx (for 3 presets), Harness CLIs on PATH (per preset chosen), platform WebView (Linux declares, macOS/Windows self-contain), GStreamer (Linux optional audio), libfuse (Linux AppImage). Each assessed for self-contain vs declare.

**Recommendation:** Declare Node+npx and Harness CLIs. Fix #726 loud failure first. Optional spike: bundle Node if install is unacceptable. Prefer first-party ACP (zero-npx) in Settings/landing copy. Evaluate named presets for goose, copilot, antigravity-acp. Platform WebView / GStreamer / FUSE already documented; no change.

Self-contained where sane (Windows WebView2 bootstrapper, macOS system WebKit). Declared where bundling costs more than clarity (Node for 3 agents, per-harness CLIs). Silent failures eliminated (never `*- not running` without explanation).

---

Dated: 2026-09-23. Anchor: Issue #735, architect comments 2026-09-17, motivating bug #726. All claims labeled Fact/Measured/Inferred/Guess. URLs verified 2026-09-23 (registry.json resolves, antigravity-acp v1.1.1 present). Do-not-claim: this note does not claim ai-buddy should bundle all 19 registry binary agents, or that acpx is the npx fix (explicitly rejected), or that Gemini CLI is a V1 path (sunset, per architect correction). Follow-ups listed as out-of-scope next steps (implementation issues to file). Open questions for Oded named explicitly.
