# What a Harness keeps under an ai-buddy ACP attach, measured

Anchor: `39fd012c` on `main`, 2026-09-16. Every ai-buddy citation below is
read against that tree. Probes ran on macOS 25.6 (Apple silicon), signed in
to claude.ai on a Team plan, with these versions on `PATH` or fetched by
`npx -y ...@latest` at run time:

| Component | Version |
|---|---|
| `@agentclientprotocol/claude-agent-acp` | 0.78.0 |
| `@anthropic-ai/claude-agent-sdk` (bundled by the adapter) | 0.3.270, spawning its own `claude` 2.1.270 |
| `claude` on `PATH` (not what the adapter runs) | 2.1.273 |
| `opencode` | 1.18.30 |
| `hermes` | 0.18.2 |
| `grok` | 1.0.30 |
| `pi-acp` over `pi` | 0.0.33 over 0.85.1 |

This note answers #668 alongside
[`harness-native-tools-under-acp.md`](./harness-native-tools-under-acp.md)
(#669), which did the desk research from adapter source and vendor docs at
anchor `e070216` and left five items open because it ran nothing. This note
runs the cheap ones and records what came back. Every measurement here
agrees with that note's Facts. The one number it got wrong had changed on
`main` between its anchor and this one. The contradictions section below
names it.

## How claims are marked

**Fact** is something read off a live ACP session, a shipped file, or a
vendor document quoted here. **Inference** is a deduction from Facts.
**Assumption** is a claim nothing here checked. Predictions and unseen
causes are Assumptions.

## Answers to the eight spike questions

1. **What tool list does ACP expose for `claude`?** Fact, measured off the
   Agent SDK's `init` message: 33 built-in tools, the whole `claude_code`
   preset. `Task`, `Bash`, `Read`, `Write`, `Edit`, `NotebookEdit`, `LSP`,
   `Skill`, `ToolSearch`, `WebSearch`, `WebFetch`, `Monitor`, `Workflow`,
   `EnterWorktree`, plan mode, cron and scheduling tools, MCP resource
   tools. The list also carries every tool of every MCP server the SDK
   connected. Two things are absent: `AskUserQuestion`, and anything named `computer-use`.
2. **Computer use under ACP.** Fact, from Anthropic's page: "Computer use is a
   research preview on macOS that requires a Pro or Max plan. It is not
   available on Team or Enterprise plans. It requires an interactive session,
   so it is not available in non-interactive mode with the `-p` flag."
   Fact, from `sdk.mjs` 0.3.270: the SDK launches `claude` with
   `--output-format stream-json --input-format stream-json
   --permission-prompt-tool stdio`, the print-mode class. Fact, measured: the
   session's `mcp_servers` list carries no `computer-use` entry, with or
   without the undocumented `ALLOW_ANT_COMPUTER_USE_MCP=1` in the child's
   environment. Assumption: which gate fires first on this machine is not
   observable, because the Team plan trips the plan gate before the
   interactive gate can. The blocker is Anthropic's platform (two documented
   gates), not the adapter and not ai-buddy's client capabilities.
3. **Web search and fetch.** Fact, measured: `WebSearch` and `WebFetch` are in
   the tool list in every Claude probe. Fact, measured: Grok's session lists
   `web_search`, `web_fetch`, `open_page` and the `x_*` search tools.
4. **User's existing MCP servers.** Fact, measured: with `cwd` set to
   ai-buddy's data folder the session connected the user-scope plugin server
   (`plugin:context-mode:context-mode`, `connected`) and listed five claude.ai
   connectors (`needs-auth`). With `cwd` set to `$HOME`, where the user once
   ran `claude mcp add` at the default local scope, one more server appeared
   (`caveman-shrink`, `failed`, a broken command but loaded). The built-in
   tool list was byte-identical across both runs. Fact, from the Agent SDK
   docs: "`~/.claude.json` global config: Always read", and "The `cwd` option
   determines where the SDK looks for project-level inputs." Inference: the
   only MCP loss under attach is the local and project scope. Its cause is
   the `cwd` ai-buddy chooses (`ai_buddy_core::memory::data_dir()`, passed
   at `harness.rs:1714`). ACP, the adapter, and the one MCP entry ai-buddy
   hands over on `session/new` play no part in it.
5. **Permissions.** Fact, measured: the tools are in the list. A native tool
   call that does nothing under attach is therefore a permission problem or
   a turn-budget problem, not a missing tool. Fact, at anchor:
   `harness::TURN_TIMEOUT` defaults to 120 seconds
   (`an_unset_timeout_gives_a_harness_turn_minutes_not_the_http_hop`,
   `harness.rs:71`), since #695 (#690). #669's "20s default turn budget" was
   `model::TIMEOUT` at its anchor `e070216` and no longer applies to a
   Harness turn. Not measured: whether Chat answers a
   `session/request_permission` inside that budget in practice.
6. **Client capabilities.** Fact, from the ACP initialization page: the
   client capabilities are `fs.readTextFile`, `fs.writeTextFile`,
   `terminal`, `elicitation`, and session config options. `fs/*` and
   `terminal/*` are methods the agent calls on the client. No client
   capability names computer use. Fact, from `acp-agent.js` 0.78.0:
   `AskUserQuestion` is routed only when
   `this.clientCapabilities?.elicitation?.form` is set, which matches the
   tool's absence in every probe run with `clientCapabilities: {}`.
   Inference: advertising `fs` or `terminal` switches on no agent-side tool.
   Advertising `elicitation.form` switches on exactly one.
7. **Other Harnesses.** See the matrix. Grok was measured by self-report.
   opencode and hermes could not be measured on this machine because both
   are configured against a local model server that was not running
   (`http://127.0.0.1:8000/v1`). The handshake and `session/new` succeeded.
   The prompt failed on the provider. pi answered the tool-list prompt with the
   string `pi v0.85.1` and nothing else, so its list was not obtained.
8. **Escape hatches.** Ranked below. The evidence rules out two of the issue's
   five. There is no `claude-agent-acp --cli` mode (the adapter's `cli`
   handling is argument parsing for `--version`), and passing user MCP
   config through `session/new` is unnecessary because the SDK already
   reads `~/.claude.json` and merges.

## The load-bearing claims, labelled

| # | Claim | Label | Source |
|---|---|---|---|
| 1 | The `claude` row runs Zed's adapter over the Agent SDK, which spawns its own bundled `claude` in stream-json print mode | Fact | `harness.rs:144`; `sdk.mjs` 0.3.270 argv; `init.claude_code_version` = 2.1.270 while `PATH` has 2.1.273 |
| 2 | ai-buddy attaches with `cwd` = `data_dir()` and advertises no client capabilities beyond `clientInfo` | Fact | `harness.rs:1714`; `acp_wire.rs:461`; `crates/core/src/memory.rs:51` |
| 3 | The attach keeps 33 built-in tools including `WebSearch`, `WebFetch`, `Bash`, `Task`, `Skill` | Fact | measured, `init.tools`, two `cwd` values, identical |
| 4 | `AskUserQuestion` is absent under `clientCapabilities: {}` and returns with `elicitation.form` | Fact for the first half, Inference for the second | measured; `acp-agent.js` 0.78.0 `toolName === "AskUserQuestion" && this.clientCapabilities?.elicitation?.form` |
| 5 | User-scope MCP servers and claude.ai connectors load regardless of `cwd` | Fact | measured, `init.mcp_servers` in both runs; Agent SDK docs "Always read" |
| 6 | Local-scope MCP servers load only when `cwd` is the directory they were added in | Fact | measured, `caveman-shrink` present only with `cwd=$HOME`; MCP docs scope table |
| 7 | Project-scope `.mcp.json` loads relative to `cwd` | Inference | MCP docs scope table; not exercised, no `.mcp.json` was on hand |
| 8 | Computer use needs macOS, Pro or Max (not Team or Enterprise), claude.ai auth, and an interactive session | Fact | code.claude.com/docs/en/computer-use |
| 9 | The computer-use opt-in is `enabledMcpServers`, recorded per project in `~/.claude.json` | Fact | code.claude.com/docs/en/mcp, "an opt-in list for built-in servers that default to off, such as `computer-use`" |
| 10 | No `computer-use` server appears in an ACP session on this machine, with or without `ALLOW_ANT_COMPUTER_USE_MCP=1` | Fact | measured, `init.mcp_servers` |
| 11 | On a Pro or Max machine with the opt-in recorded for ai-buddy's `cwd`, the interactive gate would still keep `computer-use` out of an ACP session | Assumption | follows from claim 8's fourth clause; not observable on a Team plan |
| 12 | The Harness turn budget is 120 seconds by default | Fact | `harness.rs` tests for #690 at anchor |
| 13 | ACP has no computer-use client capability; `fs/*` and `terminal/*` are methods the agent calls on the client | Fact | agentclientprotocol.com/protocol/initialization |
| 14 | Grok's ACP session carries web, shell, file, subagent, scheduler and media tools | Fact about the self-report, Inference about the actual list | measured `AGENT_TEXT`, 33 names |
| 15 | opencode and hermes carry their documented toolsets under an ai-buddy attach | Assumption | vendor docs only; provider offline here |
| 16 | Hermes' browser tools are gated at start-up on a CDP check and were unavailable in this run | Fact | measured stderr, `check_fn _browser_cdp_check returned False; dependent tools will be unavailable this turn` |
| 17 | pi's own tools are intact under `pi-acp` | Assumption | `pi-acp` README; the probe got only a version string back |

## The matrix

Cells describe what the agent has inside a session ai-buddy opened, with
ai-buddy's current `cwd` and empty client capabilities. Bold marks a
measured cell.

| Harness | Computer use (user's desktop) | Shell | Web search / fetch | User's own MCP servers | Filesystem / edit | Ask the user a question |
|---|---|---|---|---|---|---|
| `claude` | **no** (two platform gates; Assumption that the interactive gate holds on Pro/Max) | **yes** | **yes** | **conditional**: user scope and claude.ai connectors yes; local and project scope only when `cwd` matches | **yes** | **no** without `elicitation.form` |
| `opencode` | no (none exists) | unknown (provider offline here; vendor says yes) | unknown (vendor: `webfetch` yes, `websearch` conditional) | unknown (vendor says yes) | unknown (vendor says yes) | unknown |
| `hermes` | no (browser automation instead, opt-in) | unknown (provider offline here; vendor says yes) | unknown (vendor says yes); **browser tools no** until `--setup-browser` and a CDP check pass | unknown (vendor says yes) | unknown (vendor says yes) | unknown |
| `grok` | no (Grok Bot runs on a cloud computer) | **yes** (`run_terminal_command`) | **yes** (`web_search`, `web_fetch`, `open_page`) | conditional: `_x.ai/mcp/servers_updated` came back empty on a machine with none configured; project scope keys off `cwd` per vendor docs | **yes** | **yes** (`ask_user_question` listed) |
| `pi` | no (none exists) | unknown | no (no built-in web tool per vendor) | n/a ("No MCP" per vendor; `pi-acp` drops ours) | unknown | unknown |

Inference, unchanged from #669 and now with one measured row behind it: no
Harness of the five brings desktop control to an ACP session ai-buddy
opens. ADR-0003's "not portable across Harnesses" understates it. Desktop
control is available under no Harness on this path.

## Ranked options

Ordered by tools recovered per unit of architectural cost.

### 1. Attach in a directory the user's own configuration keys on

Fact: this recovers local-scope MCP servers (measured). Inference: it
recovers project-scope `.mcp.json`, Grok's project `.grok/config.toml`
walk, and opencode's project config, all of which key off `cwd` per vendor
docs. The candidate directory is a user-chosen project folder, or `$HOME`
as a default that matches where `claude mcp add` lands when run outside a
project. ADR-0003 is untouched. ADR-0018 is untouched, since it already
accepts that the Harness runs its own tools headless. ADR-0023 is
untouched, since the MCP entry ai-buddy hands over on `session/new` merges
with whatever `cwd` brings in rather than replacing it. Cost: the session file and Action Log
currently live beside `cwd` in the data folder (`harness.rs:1226` comment),
so the two paths have to be separated first.

### 2. Advertise `elicitation.form`

Fact: it is the one client capability that switches a tool on (claim 4).
Grok already lists `ask_user_question` regardless. ADR-0018 gives ai-buddy
the chat surface, so a multiple-choice prompt drawn by Chat is inside the
decision. ADR-0003 and ADR-0023 are untouched. Cost: a
form renderer in Chat.

### 3. Say what survives, per row, in the README

Fact: `README.md:159` reads "Chat-only (no MCP)" for `pi`, which is true of
ai-buddy's MCP and reads as a claim about pi's own tools. The Harness
Support tables carry no tool-class column at all. No ADR consequence.

### 4. The adapter's `_meta.claudeCode.options` hatch

Fact: `session/new` `_meta.claudeCode.options` reaches the SDK's `Options`
(`acp-agent.d.ts:637`), and `emitRawSDKMessages` beside it is what these
probes used. Nothing measured here needs it. The preset is already whole
and the MCP merge already happens. Its only remaining use would be
`options.env`, which can carry a provider key into a Harness ai-buddy
spawns, an ADR-0010 hazard. Keep it for diagnostics such as the probe and
out of the product.

### 5. Host an interactive session for a full-tool lane (#508)

Assumption: an interactive `claude` on a Pro or Max plan would offer
computer use, since that is the documented shape. It is the only route to
that cell in the matrix and it costs the most. ADR-0018 forbids it
outright ("We never launch, embed, or wrap the Harness's TUI"), so it needs
a supersession. ADR-0023 is strained. An interactive CLI takes MCP from its
own config, so ai-buddy's endpoint would have to be registered rather than
handed over on `session/new`. ADR-0003 is the one it serves. Not filed
here. #508 already carries the question, and this note adds that the
payoff is one tool class on one Harness on one plan tier.

### 6. Rejected: an ai-buddy Executor

Out of scope by the issue and by ADR-0003. Listed so the ranking is
exhaustive.

## Recommendation, and the smallest probe that would falsify the top option

Option 1 ranks first. It claims that changing `cwd` restores the user's
local and project MCP servers to the attach. The cheap half is already
measured and held. What remains is the probe that would show `cwd` does not
also restore computer use. Its result sizes the option.

**Run this on a Mac signed in to claude.ai on a Pro or Max plan.** Record
`enabledMcpServers: ["computer-use"]` under
`projects["<DIR>"]` in `~/.claude.json` for the directory you will pass, the
way `/mcp` writes it. Then run the probe client below:

```sh
PROBE_META='{"claudeCode":{"emitRawSDKMessages":[{"type":"system","subtype":"init"}]}}' \
node acp-probe.mjs "<DIR>" 90 'Reply with the single word ok.' \
  -- npx -y @agentclientprotocol/claude-agent-acp@latest
```

Success signal, read off the one `_claude/sdkMessage` notification:

| `init.mcp_servers` contains `computer-use` | Meaning |
|---|---|
| yes, `connected` | The interactive gate does not bind the SDK path. Option #1 plus the opt-in restores computer use. Reopen this note and #508 loses its reason. |
| no | The interactive gate holds (claim 11 confirmed). Option #1 restores MCP scope only. #508 is the only remaining route to that cell. |

Any machine on any plan can rerun the cheap half. Run the same command
twice with two `cwd` values, one of them a directory where
`claude mcp add <name> ...` was run at the default scope, and diff
`init.mcp_servers`.

`scripts/probe-harness.sh` is not a substitute. It prints the handshake
and the turn outcome, not the agent's tool list, and ACP has no method that
returns one. The probe client used for every measurement in this note is
below. It matches ai-buddy's client on the points that matter: empty
client capabilities, `mcpServers: []`, and no answer to
`session/request_permission`.

```js
import { spawn } from "node:child_process";

const [cwd, secs, prompt, dashdash, ...argv] = process.argv.slice(2);
if (dashdash !== "--") throw new Error("usage: cwd secs prompt -- argv...");
const child = spawn(argv[0], argv.slice(1), { cwd, stdio: ["pipe", "pipe", "pipe"], env: process.env });
let id = 0;
const send = (method, params) =>
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: id++, method, params }) + "\n");
const pending = {};
const request = (method, params) =>
  new Promise((resolve) => { pending[id] = resolve; send(method, params); });

let text = "";
let buf = "";
const stderr = [];
child.stderr.on("data", (d) => stderr.push(String(d)));
child.stdout.on("data", (d) => {
  buf += d;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i);
    buf = buf.slice(i + 1);
    if (!line.trim()) continue;
    let m;
    try { m = JSON.parse(line); } catch { console.log("non-json:", line.slice(0, 200)); continue; }
    if (m.id !== undefined && pending[m.id]) { pending[m.id](m); delete pending[m.id]; continue; }
    if (m.method === "_claude/sdkMessage") {
      const s = m.params?.message ?? m.params;
      if (s?.type === "system" && s?.subtype === "init") {
        console.log("INIT tools:", JSON.stringify(s.tools));
        console.log("INIT mcp_servers:", JSON.stringify(s.mcp_servers));
        for (const k of Object.keys(s)) if (!["tools", "mcp_servers"].includes(k)) console.log("INIT", k, "=", JSON.stringify(s[k]).slice(0, 200));
      }
      continue;
    }
    if (m.method === "session/update") {
      const u = m.params.update;
      if (u.sessionUpdate === "agent_message_chunk" && u.content?.type === "text") text += u.content.text;
      else if (u.sessionUpdate === "tool_call") console.log("TOOL_CALL", u.title, u.kind);
      else if (u.sessionUpdate === "available_commands_update") console.log("COMMANDS", (u.availableCommands || []).map((c) => c.name).join(","));
      else if (u.sessionUpdate !== "agent_thought_chunk") console.log("UPDATE", u.sessionUpdate);
      continue;
    }
    if (m.method === "session/request_permission") {
      console.log("PERMISSION_REQUEST", JSON.stringify(m.params.toolCall?.title), JSON.stringify(m.params.options?.map((o) => o.kind)));
      continue;
    }
    if (m.method) {
      console.log("AGENT_REQ", m.method, JSON.stringify(m.params).slice(0, 200));
      if (m.id !== undefined) child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "Method not found" } }) + "\n");
    }
  }
});

const init = await request("initialize", { protocolVersion: 1, clientCapabilities: {}, clientInfo: { name: "probe-668", version: "0" } });
console.log("INITIALIZE agentCapabilities:", JSON.stringify(init.result?.agentCapabilities), "agentInfo:", JSON.stringify(init.result?.agentInfo));
console.log("INITIALIZE authMethods:", JSON.stringify((init.result?.authMethods || []).map((a) => a.id)));
const meta = process.env.PROBE_META ? JSON.parse(process.env.PROBE_META) : undefined;
const ses = await request("session/new", { cwd, mcpServers: [], ...(meta ? { _meta: meta } : {}) });
if (ses.error) {
  console.log("SESSION_NEW error:", JSON.stringify(ses.error).slice(0, 400));
} else {
  console.log("SESSION_NEW ok, modes:", JSON.stringify(ses.result?.modes?.availableModes?.map((x) => x.id)));
  const p = request("session/prompt", { sessionId: ses.result.sessionId, prompt: [{ type: "text", text: prompt }] });
  const timeout = new Promise((r) => setTimeout(() => r({ timeout: true }), Number(secs) * 1000));
  const done = await Promise.race([p, timeout]);
  console.log("PROMPT result:", JSON.stringify(done.result ?? done.error ?? done).slice(0, 300));
}
console.log("AGENT_TEXT:\n" + text);
const err = stderr.join("");
if (err.trim()) console.log("STDERR (first 1200):\n" + err.slice(0, 1200));
child.kill();
process.exit(0);
```

## Where this contradicts what the repository already says

- **ADR-0003, Consequences.** "The capability is a research preview gated
  behind a Pro or Max subscription" names one of four gates. Anthropic's
  page adds Team and Enterprise as excluded, claude.ai auth as required, and
  an interactive session as required. The fourth is the one no subscription
  clears and the one that bears on ai-buddy's attach path. The decision
  still holds on its other grounds. The consequence is incomplete.
- **`README.md:159`.** "Chat-only (no MCP)" on the `pi` row. See option 3.
- **`harness-native-tools-under-acp.md`, claim 14.** The 20-second turn
  budget was true at its anchor `e070216` and is 120 seconds at this one
  (`harness::TURN_TIMEOUT`, #695 for #690). That note now carries a
  "since shipped" preamble line saying so, added in the same PR as this
  note per `docs/agents/docs.md`.
- **`docs/research/buddy-harness-two-way.md:14`.** "a local agent that keeps
  its own tools, permissions, and memory" holds for the tool preset and for
  user-scope configuration. It does not hold for computer use, and it holds
  for local-scope configuration only with a `cwd` change.

## Open, not resolved here

- Claim 11 needs a Pro or Max Mac.
- Claims 15 and 17 need a machine where opencode and hermes have a live
  provider, and a prompt pi will answer. The probe client works unchanged.
- Whether the claude.ai connectors that showed `needs-auth` here would show
  `connected` for a user who authorized them in the CLI. Nothing here
  exercised that.
- Whether Chat answers a permission request inside 120 seconds in practice
  for a `Bash` or `WebFetch` call under attach. That is a different spike.
