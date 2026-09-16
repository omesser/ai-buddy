# The Harness's own tools under ACP attach

Research for #668. Question: when ai-buddy attaches a Harness over ACP, does
that Harness keep its *own* full toolbox — desktop control, web, the user's
existing MCP integrations — with ai-buddy's MCP added on top? Or does attach
demote it to a chat brain that can only call `speak`?

**Answer.** The premise is half wrong, and the half that survives is the
expensive one. A `claude` attach keeps the whole `claude_code` tool preset —
`Bash`, `Read`/`Write`/`Edit`, `WebSearch`, `WebFetch`, subagents, skills,
`CLAUDE.md` — because Zed's adapter hands the Agent SDK
`tools: { type: "preset", preset: "claude_code" }` and
`settingSources: ["user", "project", "local"]`, and it *merges* ai-buddy's MCP
server into the user's rather than replacing it. Attach loses exactly three
things. **Computer use**, which Anthropic gates on an interactive session and
on an opt-in recorded per project directory. **The user's project- and
local-scoped MCP servers**, because every one of the five Harnesses keys
project configuration off `cwd`, and ai-buddy's `cwd` is its own app-data
folder rather than anywhere the user has ever configured. **`AskUserQuestion`**,
which the adapter puts in `disallowedTools` because ai-buddy advertises no
`elicitation` client capability. So "Harness attach loses the main reason to
prefer it over Model API" does not hold: full coding tools, web, user-scope
MCP and the claude.ai connectors all survive. What does hold is narrower and
sharper — the one capability ADR-0003 leaned on when it declined to ship an
Executor is the one capability this attach shape cannot reach. The cheapest
lever by far is `cwd`, which ai-buddy already owns and currently picks badly;
the probe below settles whether `cwd` also buys computer use back, or whether
only a hosted interactive session can.

`file:line` citations are against `e0702166`.

---

## How claims are marked

Following `reasoning-versus-the-final-answer.md` and
`director-in-flight-and-latency.md`, every load-bearing claim carries a label.

- **[Fact]** — quoted from a primary source: vendor documentation, adapter or
  SDK source at a named version, ai-buddy's own code, or a string read out of
  a shipped binary.
- **[Inference]** — my reasoning from those facts. Sound or not, it is mine.
- **[Assumption]** — not verified. Each one names what would verify it.

Vocabulary is `CONTEXT.md`: Harness, Completer, Director, Functional Layer,
Executor, Sensing, Instance, Behavior, Speech.

`file:line` citations are against `e070216`. Third-party source read on
2026-09-13: `@agentclientprotocol/claude-agent-acp` **0.76.0** at commit
`4deace4` (2026-09-11), `@anthropic-ai/claude-agent-sdk` **0.3.257**,
`@anthropic-ai/claude-code` **2.1.270** (`linux-x64` binary), `pi-acp` at
`main`. Vendor documentation fetched the same day.

## The load-bearing claims, labelled

| # | Claim | Label | Source |
|---|---|---|---|
| 1 | ai-buddy's `claude` row launches Zed's ACP adapter, not the interactive CLI | Fact | `harness.rs:120-123` |
| 2 | ai-buddy attaches with `cwd` = the app data folder, and the child inherits the environment untouched | Fact | `harness.rs:161-171`, `1492-1501`; `crates/core/src/memory.rs:51-55` |
| 3 | ai-buddy advertises no `fs`, `terminal`, or `elicitation` client capability | Fact | `acp_wire.rs:393-397`, `421-432` |
| 4 | ai-buddy passes exactly zero or one MCP server, named `ai-buddy`, and no `_meta` | Fact | `acp_wire.rs:567-585`, `591-610` |
| 5 | The adapter defaults to the full `claude_code` built-in tool preset | Fact | `acp-agent.ts:7907-7914` |
| 6 | The adapter passes `settingSources: ["user","project","local"]` and *merges* client MCP servers over user-configured ones | Fact | `acp-agent.ts:8003-8020` |
| 7 | The Agent SDK spawns the real `claude` binary in stream-json print mode, with `--permission-prompt-tool stdio` | Fact | `sdk.mjs` 0.3.257 argv assembly; `cli-reference` flag definitions |
| 8 | Claude computer use requires macOS, Pro or Max, a claude.ai login, and an interactive session | Fact | `code.claude.com/docs/en/computer-use` |
| 9 | The computer-use opt-in is `enabledMcpServers`, recorded **per project** in `~/.claude.json`, and is not a settings key | Fact | `docs/en/mcp`; absent from `settings-reference`; `claude` 2.1.270 binary |
| 10 | `~/.claude.json` and claude.ai MCP connectors are read regardless of `settingSources` | Fact | `docs/en/agent-sdk/claude-code-features` |
| 11 | Claude local-scope MCP servers are keyed to the directory they were added in | Fact | `docs/en/mcp-quickstart` |
| 12 | The adapter disables `AskUserQuestion` unless the client advertises form elicitation | Fact | `acp-agent.ts:7901-7905` |
| 13 | ACP has no computer-use client capability; `fs/*` and `terminal/*` are the *client's* filesystem and terminal | Fact | ACP v1 initialization, file-system, terminals |
| 14 | An unanswered `session/request_permission` has no timeout of its own; the turn's 20s default budget expires and the ask is cancelled, not denied | Fact | `acp_wire.rs:306-313`, `801-814`; `src-tauri/src/model.rs:28` |
| 15 | The tool and MCP-server loss under attach is caused by ai-buddy's `cwd` choice, not by ACP or by the adapter | Inference | from 2, 6, 9, 10, 11 |
| 16 | No Harness of the five brings desktop control to an ACP session ai-buddy opens | Inference | matrix below |
| 17 | Whether the computer-use gate that actually bites is the interactive check or the per-project opt-in | Assumption | settled by the probe below |
| 18 | Whether `opencode`, `hermes` and `grok` sessions ai-buddy opens really carry the tools their vendors document | Assumption | never read off a live attach; ACP exposes no tool-list method |
| 19 | Whether `ALLOW_ANT_COMPUTER_USE_MCP` gates registration of the built-in server | Assumption | name found in the 2.1.270 binary; use site not resolvable from strings |

## What "full tool stack" has to mean

The issue asks about a stack, so the note answers per tool class. Five classes
cover everything #668 names:

| Class | What it is | Why the product needs it |
|---|---|---|
| Computer use | Synthetic mouse and keyboard on the user's desktop, plus screen capture | The Functional Layer. ADR-0003 gave this to the Harness and shipped no Executor |
| Shell | `Bash`, `terminal`, `execute_code` | Most real work, and the fallback route to anything with a CLI |
| Web | Search and fetch | "Look it up" is the commonest ask a mascot gets |
| User MCP | The servers the user already added — mail, calendar, issue trackers | The user's existing integrations, which is the whole bring-your-own-agent promise |
| Filesystem / edit | Read, write, edit, glob, grep | Table stakes, and the only class ACP has client-side methods for |

ai-buddy's own seven tools (`speak`, `play_behavior`, `list_windows`,
`describe_screen`, `recall`, `remember`, `list_instances`) are a sixth class
and are not at risk: they arrive as an MCP server on `session/new`, and every
Harness that takes MCP at all takes them. **[Fact]** — `README.md:186-197`,
`acp_wire.rs:567-585`.

## What ai-buddy actually launches, and in which directory

**[Fact]** The `claude` row launches Zed's adapter, not the interactive CLI —
`harness.rs:108-123`, with the arm's long comment about `@latest` elided:

```rust
pub fn launch(value: Option<&str>) -> Option<Launch> {
    let value = value?.trim();
    let (name, argv): (&str, Vec<&str>) = match value {
        "" => return None,
        // …
        "claude" => (
            value,
            vec!["npx", "-y", "@agentclientprotocol/claude-agent-acp@latest"],
        ),
```

**[Fact]** The child inherits the environment untouched, sets nothing, and is
spawned in a directory we choose — `harness.rs:161-171`:

```rust
    /// The child, inheriting our environment untouched. ADR-0010 rules 4 and
    /// 5: no provider key, no `CLAUDE_CONFIG_DIR`, no `--bare`. A test pins it.
    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]).current_dir(cwd);
        isolate_from_interrupt(&mut command);
        command
    }
```

**[Fact]** That directory is the app data folder — `harness.rs:1492-1501`:

```rust
fn open(launch: Option<Launch>, forward: Arc<Forward>) -> Option<Arc<Session>> {
    launch.map(|launch| {
        Arc::new(Session::new(
            launch,
            ai_buddy_core::memory::data_dir(),
            forward,
        ))
    })
}
```

which resolves to `~/.local/share/ai-buddy`,
`~/Library/Application Support/ai-buddy`, or `%APPDATA%\ai-buddy`
(`crates/core/src/memory.rs:51-55`), and to the same path plus `/probe` under
`scripts/probe-harness.sh`. **[Inference]** This one choice is the largest
single determinant of how many tools an attached Harness has, and nothing in
the repository says so. Every section below returns to it.

**[Fact]** `initialize` advertises a name and version and nothing else — no
`fs`, no `terminal`, no `elicitation`, no `_meta` capability
(`acp_wire.rs:421-432`). The file states the consequence in a comment at
`393-397`: "Anything else the Harness asks — `fs/*`, `terminal/*`,
capabilities we never advertised — the SDK answers with method-not-found on
its own."

**[Fact]** `session/new` carries `cwd` and zero or one MCP server, always
named `ai-buddy`, and no `_meta` (`acp_wire.rs:567-585`, `591-610`). We never
read, merge, or forward the user's own MCP configuration; ADR-0026 states the
intent — "The MCP server entry is already the app's own to fill — this sets
nothing of the user's."

## The Claude path, in detail

### Interactive CLI versus ACP adapter versus Agent SDK

These are three surfaces, and the adapter is a client of the third, which is a
client of the first.

**[Fact]** The adapter drives the Agent SDK, and the Agent SDK drives the real
`claude` binary as a subprocess — `acp-agent.ts:8045`:

```ts
pathToClaudeCodeExecutable: process.env.CLAUDE_CODE_EXECUTABLE ?? (await claudeCliPath()),
```

**[Fact]** The argv the SDK builds for that subprocess, read out of `sdk.mjs`
0.3.257, begins

```js
let Z = ["--output-format", "stream-json", "--verbose", "--input-format", "stream-json"];
```

and then appends, among others, `--tools default`, `--mcp-config <json>`,
`--setting-sources=user,project,local`, `--permission-prompt-tool stdio`
(whenever a `canUseTool` callback is set), `--include-partial-messages`, and
`--replay-user-messages`. It never appends `--print`, `--strict-mcp-config`,
or `--bare` unless asked.

**[Fact]** Anthropic's CLI reference defines those as print-mode flags:
`--print, -p` is "Print response without interactive mode"; `--input-format`
is "Specify input format for print mode"; `--permission-prompt-tool` is
"Specify an MCP tool to handle permission prompts in non-interactive mode";
and `--replay-user-messages` "Requires `--input-format stream-json` and
`--output-format stream-json`".
<https://code.claude.com/docs/en/cli-reference>

**[Inference]** An ACP session is therefore in the non-interactive class:
stdin is a JSON stream on a pipe, there is no TTY, and permission prompts are
routed to a tool rather than to a terminal. Whatever Anthropic means by
"interactive session", this is the other thing.

### What the adapter configures, quoted

The whole `Options` object is worth reading, because four lines in it answer
four of the issue's questions at once. **[Fact]** — `acp-agent.ts:8003-8024`:

```ts
    const options: Options = {
      systemPrompt,
      settingSources: ["user", "project", "local"],
      ...(thinking !== undefined && { thinking }),
      ...userProvidedOptions,
      ...(settings && { settings }),
      env,
      // Override certain fields that must be controlled by ACP
      cwd: params.cwd,
      includePartialMessages: true,
      forwardSubagentText,
      mcpServers: {
        ...(userProvidedOptions?.mcpServers || {}),
        ...mcpServers,
        ...(fileChangeAuditSupport
          ? { [FILE_CHANGE_AUDIT_SERVER_NAME]: fileChangeAuditSupport.mcpServer }
          : {}),
      },
      allowDangerouslySkipPermissions: ALLOW_BYPASS,
      permissionMode: initialPermissionMode,
      canUseTool: this.canUseTool(sessionId),
```

and the built-in tool set defaults to the full preset. **[Fact]** —
`acp-agent.ts:7907-7914`:

```ts
    // Resolve which built-in tools to expose.
    // Explicit tools array from _meta.claudeCode.options takes precedence.
    // disableBuiltInTools is a legacy shorthand for tools: [] — kept for
    // backward compatibility but callers should prefer the tools array.
    const tools: Options["tools"] =
      userProvidedOptions?.tools ??
      (params._meta?.disableBuiltInTools === true ? [] : { type: "preset", preset: "claude_code" });
```

**[Inference]** So by default an ACP Claude session has the same built-in tool
set as the CLI, reads the same user and project settings the CLI reads, and
gets ai-buddy's MCP server layered *on top of* whatever the user already
configured — not instead of it. None of the collapse the issue describes is in
this code.

### Computer use

**[Fact]** Computer use in the CLI is not a built-in tool. It is a built-in
MCP server, off by default, and Anthropic gates it four ways: "Computer use is
a research preview on macOS that requires a Pro or Max plan. It is not
available on Team or Enterprise plans. It requires an interactive session, so
it is not available in non-interactive mode with the `-p` flag." The
troubleshooting list repeats each gate, including "You're in an interactive
session. Computer use is not available in non-interactive mode with the `-p`
flag" and "You're authenticated through claude.ai. Computer use is not
available with third-party providers".
<https://code.claude.com/docs/en/computer-use>

**[Fact]** Enabling it is a per-project record, not a settings key. The MCP
reference: "Claude Code records your choice per project in `~/.claude.json`,
in one of two lists… `enabledMcpServers`: an opt-in list for built-in servers
that default to off, such as `computer-use`. Claude Code connects to a
default-off server only when you list it here."
<https://code.claude.com/docs/en/mcp> The key does not appear in the settings
reference at all, so `settingSources` cannot carry it.
<https://code.claude.com/docs/en/settings-reference>

**[Fact]** The shipped binary agrees. In `claude` 2.1.270 the server name is a
constant and the enablement check is a special case of the disabled check:

```js
var vX = "computer-use", /* … */
function yNe(e){ return e === vX }
function es(e){ let n = ts();
  if (yNe(e)) return !bNe(n.enabledMcpServers).includes(e);
  return bNe(n.disabledMcpServers).includes(e) }
```

Its tool names are
`["request_access","screenshot","zoom","left_click","double_click",…,
"open_application","switch_display","read_clipboard","write_clipboard",…]`,
its platform configuration is
`{screenshotFiltering:"native",platform:"darwin",…}`, and the native module
refuses elsewhere: `@ant/computer-use-swift is only available on macOS`.

**[Fact]** `~/.claude.json` is read no matter what `settingSources` says: "the
global `~/.claude.json` config… Always read", relocatable only with
`CLAUDE_CONFIG_DIR`.
<https://code.claude.com/docs/en/agent-sdk/claude-code-features>

**[Inference]** Chain those four and the failure is fully explained without
any bug. ai-buddy attaches with `cwd` = its own data folder, which is a
project entry in `~/.claude.json` that no user has ever opened `/mcp` in, so
`enabledMcpServers` for that entry is empty and the `computer-use` server is
never connected — before the interactive gate is even consulted. A user who
enabled computer use in their repository enabled it for *that* directory's
entry. This is a scoping accident stacked on a platform gate, and the two need
separating because they have different fixes.

**[Fact]** There is counter-evidence worth taking seriously, which is why the
probe exists. Zed's adapter already has first-class handling for computer-use
tool names arriving over ACP — a prefix constant, a permission-option builder,
and tests named for it. `permissions/options/tools.ts:151-170`:

```ts
const COMPUTER_USE_MCP_TOOL_PREFIX = "mcp__computer-use__";

export function isComputerUseMcpTool(toolName: string): boolean {
  return toolName.startsWith(COMPUTER_USE_MCP_TOOL_PREFIX);
}

export function buildComputerUseMcpPermissionOptions(
  context: PermissionOptionContext,
): PermissionOption[] {
```

with `it("recognizes real Computer Use MCP tools and preserves their exact
durable suggestion")` and `it("reuses the Computer Use MCP tool-call title")`
in `src/tests/permission-options.test.ts:532-574`.

**[Assumption]** Either the adapter authors saw `mcp__computer-use__*` tool
calls in a live ACP session, or they wrote defensively for a client that
passes the server in itself. Source alone cannot tell which. Settled by the
probe below: if the session's `init` message lists
`mcp__computer-use__screenshot` once the opt-in is recorded for ai-buddy's
`cwd`, the first reading is right and the interactive rule is softer than the
documentation says; if it does not, the gate is absolute for this path.

**[Fact]** One more gate sits on the same lever, and it is commercial rather
than technical. Computer use requires a claude.ai Pro or Max login, and the
Agent SDK overview says: "Unless previously approved, Anthropic does not allow
third party developers to offer claude.ai login or rate limits for their
products, including agents built on the Claude Agent SDK. Use the API key
authentication methods described in the Quickstart instead."
<https://code.claude.com/docs/en/agent-sdk/overview>

**[Inference]** ai-buddy is not caught by that as written, because ADR-0018
already puts authentication on the user's side of the line and ADR-0010 rule 6
forbids proxying a vendor login — the user runs `claude /login` in their own
terminal and the subprocess inherits it. But it does mean "ask Anthropic to
lift the interactive gate for SDK sessions" is a request against a policy, not
a bug report, and should not be planned as though a release will fix it.

**[Fact]** The shipped CLI carries an undocumented environment variable named
`ALLOW_ANT_COMPUTER_USE_MCP` in its environment-variable registry (2.1.270).
**[Assumption]** that it gates registration of the `computer-use` server; its
use site is not resolvable from the binary's string table. Setting an
undocumented vendor flag into a Harness we spawn sits close enough to ADR-0010
rules 4 and 5 that this is recorded as a finding, not proposed as a plan.

### Web search and fetch

**[Fact]** `WebSearch` ("Performs web searches") and `WebFetch` ("Fetches
content from a specified URL") are ordinary entries in the Claude Code tools
reference, with no interactivity, plan, or platform condition attached — the
only conditions are permission prompts and provider availability. The string
"computer" does not appear anywhere in that reference, which is consistent
with computer use being an MCP server rather than a tool.
<https://code.claude.com/docs/en/tools-reference>

**[Fact]** The SDK's own tool-schema declarations include `WebSearchInput`,
`WebFetchInput`, `BashInput`, `FileReadInput`, `FileWriteInput`,
`FileEditInput`, `GlobInput`, `GrepInput`, `AgentInput` and `TodoWriteInput`,
and declare no computer-use tool type (`sdk-tools.d.ts`, 0.3.257).

**[Inference]** Web is not part of the problem. It ships with the preset the
adapter already passes.

### The user's existing MCP servers

This is the question with the most interesting answer, because two of three
scopes are inherited and the third is lost for a reason ai-buddy controls.

**[Fact]** Claude Code stores MCP servers in three scopes:

| Scope | File | Available to |
|---|---|---|
| `local` (the default) | `~/.claude.json`, under the entry for this project | Only you, only this project |
| `project` | `.mcp.json` in your project root | Everyone who clones the project |
| `user` | `~/.claude.json`, under the top-level `mcpServers` key | Only you, all projects |

and the troubleshooting section describes exactly ai-buddy's situation: "You
ran `claude mcp add` from a different project. Local-scoped servers are tied
to the project where you added them… or add it with `--scope user` so it isn't
tied to a project." <https://code.claude.com/docs/en/mcp-quickstart>

**[Fact]** `.mcp.json` loads through the `project` setting source — "The file
is picked up when the `project` setting source is enabled"
<https://code.claude.com/docs/en/agent-sdk/mcp> — and the adapter enables that
source. **[Fact]** `--strict-mcp-config` is the flag that would make
`options.mcpServers` exclusive, and neither the adapter nor the SDK passes it
by default.

**[Fact]** The mail-and-calendar case specifically is inherited, and cannot be
suppressed by passing servers: "claude.ai MCP connectors… Loaded when the
session authenticates with your claude.ai login… Passing `mcpServers: {}` does
not suppress the connectors."
<https://code.claude.com/docs/en/agent-sdk/claude-code-features> The CLI page
says the same from the other side: "connectors you add at
claude.ai/customize/connectors load automatically in the CLI when you sign in
with that account." <https://code.claude.com/docs/en/mcp-quickstart>

**[Inference]** So under a `claude` attach the user keeps user-scope MCP
servers, claude.ai connectors, managed and enterprise servers, and ai-buddy's
server. The user loses local-scope servers — the default scope, tied to the
project directory they were added in — and project-scope `.mcp.json`, read
relative to `cwd`. Both losses are `cwd`: not ACP, not the adapter, not
ADR-0026. A user whose Gmail integration is a claude.ai connector already has
it under attach today; a user who ran `claude mcp add gmail …` inside
`~/code/thing` does not, and one `--scope user` re-add fixes them.

**[Fact]** Project-scope servers also carry an approval gate — an unapproved
server shows `⏸ Pending approval (run claude to approve)`, cleared in an
interactive session or by `enableAllProjectMcpServers` /
`enabledMcpjsonServers` in settings.
<https://code.claude.com/docs/en/mcp-quickstart>,
<https://code.claude.com/docs/en/settings-reference>
**[Inference]** So even pointing `cwd` at the user's project does not
automatically light up `.mcp.json`; they have to have approved it there once,
which they will have if they use Claude in that repository.

### Permissions: which of the two failures is actually happening

The issue asks whether native tool calls die because Chat never answers
`session/request_permission`, or because the tools never appear in the agent's
list. **[Fact]** ai-buddy forwards the request, never answers it, and never
auto-approves — `harness.rs:771-781`:

```rust
    /// The user's answer to a forwarded permission request. Never chosen here.
    pub fn answer_permission(&self, request: &str, option: &str) {
```

**[Fact]** An unanswered ask has no timeout of its own. The *turn* times out —
20s by default from `model::TIMEOUT`, overridable with
`AI_BUDDY_DIRECTOR_TIMEOUT_SECS` — then `session/cancel` goes out and every
open ask is answered `Cancelled` rather than denied (`acp_wire.rs:306-313`,
`801-814`). **[Fact]** The probe path prints asks and never answers them, so
they always time out there (`docs/DEVELOPMENT.md:332-371`).

**[Inference]** Both failures are real, they are different failures, and they
present identically as "the Harness did nothing". A 20-second default turn
budget is short for any tool call a human has to approve, and a computer-use
sequence — which asks for per-application access before it starts — cannot fit
in it at all. So even if the probe shows computer-use tools present, the
Completer timeout is a second blocker standing in front of them. Worth saying,
because it is cheap to fix and would otherwise be misread as the gate.

### Client capabilities: what advertising `fs` and `terminal` would buy

**[Fact]** ACP's client capabilities are `terminalAuth`, `fs.readTextFile`,
`fs.writeTextFile`, `terminal`, `elicitation` (with `form` and `url` modes),
and `booleanConfigOptions`. There is no computer-use capability and no
extension point for one other than `_meta`: "Implementations can also
advertise custom capabilities using the `_meta` field to indicate support for
protocol extensions."
<https://agentclientprotocol.com/protocol/v1/initialization>

**[Fact]** `fs/*` and `terminal/*` are methods the *agent* calls on the
*client*: "The filesystem methods allow Agents to read and write text files
within the Client's environment… including unsaved changes in the editor"
<https://agentclientprotocol.com/protocol/v1/file-system> and "The terminal
methods allow Agents to execute shell commands within the Client's
environment" <https://agentclientprotocol.com/protocol/v1/terminals>.

**[Inference]** Advertising them does not unlock agent-side tools; it moves
work *to us*. Advertising `terminal` in particular would have ai-buddy running
shell commands on the agent's behalf, which is an Executor for shell instead
of for mouse and keyboard — the thing ADR-0003 refuses, one input class over.
Do not advertise `terminal`.

**[Fact]** One capability does buy a named tool back. The adapter disables
`AskUserQuestion` unless the client advertises form elicitation —
`acp-agent.ts:7901-7905`:

```ts
    // AskUserQuestion surfaces as a `permission_ask_user_question` dialog that
    // we render as a form elicitation. Without form-elicitation support there
    // is no way to present it over ACP, so keep it disabled in that case.
    const disallowedTools = elicitationSupport.form ? [] : ["AskUserQuestion"];
```

**[Inference]** This is the clearest worked example of the mechanism the issue
suspects — a client capability we omit removing a tool from the agent's list —
and it is the only instance of it found anywhere in the adapter. It is also
the only one of these levers that is spec-level, and therefore portable.

## The matrix

Rows are the five named Harnesses. Columns are the tool classes. Each cell is
what is available **to the agent, inside a session ai-buddy opened**, not what
the vendor's CLI can do in general.

| | Computer use (user's desktop) | Shell | Web search / fetch | User's own MCP servers | Filesystem / edit |
|---|---|---|---|---|---|
| `claude` | **no** — conditional at best, see #17 | **yes** | **yes** | **conditional** — user scope and claude.ai connectors yes; local and project scope no | **yes** |
| `opencode` | **no** — none exists | **yes** | **conditional** — `webfetch` yes; `websearch` needs the OpenCode provider or `OPENCODE_ENABLE_EXA` / `OPENCODE_ENABLE_PARALLEL` | **yes** | **yes** |
| `hermes` | **no** — headless browser automation instead, opt-in | **yes** | **conditional** — web tools are in the ACP toolset; browser tools need `hermes acp --setup-browser` | **yes** | **yes** |
| `grok` | **no** — Grok Bot runs on a cloud computer, not yours | **yes** | **yes** — on by default (`--disable-web-search` turns it off) | **conditional** — user scope yes; project scope keyed off `cwd` | **yes** |
| `pi` | **no** — none exists | **yes** | **no** — no built-in web tool | **n/a** — "No MCP, by design", and `pi-acp` drops ours | **yes** |

Every `claude` cell is **[Inference]** from the adapter's `Options` and the
computer-use gates above; its computer-use cell is the one genuinely
**unknown** value in the table and the probe targets it. It reads "no" because
that is both the observed behaviour and the documented rule, not because it is
proven impossible. The other four rows rest on vendor documentation, which is
**[Fact]** about what the vendor claims and **[Assumption]** about what an
ai-buddy attach delivers (claim #18):

- `opencode` — "OpenCode works the same via ACP as it does in the terminal.
  All features are supported", listing "Built-in tools (file operations,
  terminal commands, etc.)", "MCP servers configured in your OpenCode config",
  and `AGENTS.md` rules. <https://opencode.ai/docs/acp/> The built-in tools are
  `bash`, `edit`, `write`, `read`, `grep`, `glob`, `apply_patch`, `skill`,
  `todowrite`, `webfetch`, `websearch`, `question`, with the `websearch`
  condition quoted in the cell. <https://opencode.ai/docs/tools/>
- `hermes` — "Hermes runs with a curated `hermes-acp` toolset designed for
  editor workflows": file tools, "terminal tools: `terminal`, `process`",
  "web/browser tools", memory, todo, skills, "execute_code and delegate_task",
  vision. Configured MCP starts by default, and
  `HERMES_ACP_SKIP_CONFIGURED_MCP=1` is the host's opt-out with the promise
  that "MCP servers supplied by the ACP session through `session/new` are still
  registered, so a host loses no capability it asked for."
  <https://hermes-agent.nousresearch.com/docs/user-guide/features/acp>
- `grok` — MCP servers from `~/.grok/config.toml` are "available alongside the
  built-in ones"; project scope "walks from the current directory up to the
  git root reading each `.grok/config.toml`"; Grok also loads `~/.claude.json`,
  `.cursor/mcp.json` and `.mcp.json`.
  <https://docs.x.ai/build/features/mcp-servers> `--disable-web-search` and
  `--tools` are in the CLI reference. <https://docs.x.ai/build/cli/reference>
  ACP is documented in xAI's own repository, including `session/new` `_meta`
  options (`rules`, `systemPromptOverride`, `agentProfile`, `yoloMode`,
  `autoMode`) and extension families `x.ai/fs/*`, `x.ai/terminal/*`,
  `x.ai/git/*`.
  <https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/15-agent-mode.md>
- `pi` — "By default, pi gives the model four tools: `read`, `write`, `edit`,
  and `bash`", with `grep`, `find`, `ls` and `powershell` also built in, and
  "**No MCP.** Build CLI tools with READMEs… or build an extension that adds
  MCP support."
  <https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent>
  `pi-acp`'s own Limitations settle two cells outright: "No ACP filesystem
  delegation (`fs/*`) and no ACP terminal delegation (`terminal/*`). pi
  reads/writes and executes locally." and "MCP servers are accepted in ACP
  params and stored in session state, but not wired through to pi in this
  adapter." <https://github.com/svkozak/pi-acp>

Two conclusions the matrix makes visible that the issue did not assume.

**[Inference]** No Harness of the five brings desktop control to an ACP
session. Claude is the only one that has the capability at all, and Hermes'
browser tools and Grok's cloud computer are different products rather than
this one. ADR-0003's "It is not portable across Harnesses" was right, and the
current reading is stronger than what it wrote: desktop control is available
under *no* Harness on this attach shape.

**[Inference]** `pi-acp` not wiring `mcpServers` through is why `pi` "gets
nothing forwarded today" (`README.md:172`), and it is a third-party adapter
gap rather than anything about pi or about ai-buddy. It is also the one row
where README wording oversells the loss: "Chat-only (no MCP)"
(`README.md:157`) reads as "pi has no tools", when pi keeps `read`, `write`,
`edit`, `bash`, `grep`, `find` and `ls`, and loses only *ours*.

## Ranked options

Ranked by expected tool surface recovered per unit of architectural damage.

### 1. Say what actually survives, per row, in the README and the product copy

**[Inference]** Free, and a prerequisite for everything else. Today the
Harness Support table describes session mechanics — `loadSession`, MCP
transport, auth methods — and says nothing about tool classes, while the
product story says "full agent toolbox". A tool-class column, with an honest
"desktop control: none, on any row" line, turns a silent product gap into a
stated one.

ADR consequences: none. This is `README.md`, and possibly a `DESIGN.md`
sentence. ADR-0003's Consequences already carry the warning in the abstract —
"A Harness without an executor can still chat and sense" — and this makes it
concrete.

### 2. Stop attaching in the app data folder: make `cwd` a directory the user's own configuration keys on

**[Inference]** The highest-value change in this note. `cwd` is already ours
to choose, it is currently chosen for tidiness, and it silently governs
Claude's local-scope MCP servers, Claude's `.mcp.json`, Claude's
`enabledMcpServers` computer-use opt-in, Grok's `.grok/config.toml` walk to
the git root, opencode's project `AGENTS.md` and project configuration, and
Hermes' file and terminal tool roots. Every one of those is a tool the user
configured and does not get.

What it costs: a Settings row and one honest consent question, because the
directory a Harness attaches in is the directory its `Bash` tool starts in.
ADR-0018 already accepts that the attached Harness has its own tools and runs
its own permission prompts, so this changes *where* those tools act rather
than whether they exist. A sensible default is a working folder the user
picked, with the data folder as the fallback, and never a silent `$HOME`.

ADR consequences: **ADR-0010 rules 4 and 5** are untouched — they forbid a
provider key, `CLAUDE_CONFIG_DIR`, and `--bare`, and say nothing about `cwd`;
the pinning test, `child_command_sets_no_env_and_passes_no_bare` in
`harness.rs`, asserts *that* a `cwd` is passed, not which one. **ADR-0023 / ADR-0026** untouched: the MCP entry we hand over
does not change. **ADR-0008** untouched: one session, one conversation, now
rooted somewhere useful. **[Inference]** It probably wants a short ADR of its
own, because "ai-buddy's `cwd` is the Harness's project scope" is exactly the
kind of non-obvious, hard-to-reverse consequence ADR gate 2 exists for.

### 3. Advertise the client capabilities we can honestly serve, `elicitation.form` first

**[Fact]** Buys `AskUserQuestion` back on the `claude` row, per the adapter's
`disallowedTools` line. **[Inference]** It is spec-level, so it is the only
option here that also helps a Harness nobody has thought about yet; Hermes and
Grok both document richer dialog and permission flows than a three-button
prompt can carry.

ADR consequences: **ADR-0018** already gives ai-buddy the chat surface and the
permission prompt, and a form dialog is that same surface doing that same job,
so no supersession. **ADR-0003** unaffected. Explicitly *not* in scope:
advertising `terminal`, for the Executor-by-another-name reason above.

### 4. Use the adapter's documented `_meta.claudeCode.options` hatch, narrowly

**[Fact]** The adapter documents a per-session escape hatch with defined merge
semantics — `acp-agent.ts:1156-1178`:

```ts
export type NewSessionMeta = {
  claudeCode?: {
    /**
     * Options forwarded to Claude Code when starting a new session.
     * Those parameters will not be forwarded because they are managed by ACP:
     *   - cwd
     *   - includePartialMessages
     *   - allowDangerouslySkipPermissions
     *   - permissionMode
     *   - canUseTool
     *   - executable
     * …
     * Those parameters will be used and updated to work with ACP:
     *   - hooks (merged with ACP's hooks)
     *   - mcpServers (merged with ACP's mcpServers)
     *   - disallowedTools (merged with ACP's disallowedTools)
     *   - tools (passed through; defaults to claude_code preset if not provided)
     */
    options?: Options;
```

**[Inference]** Useful for exactly one thing in the near term:
`emitRawSDKMessages`, which makes the session's real tool list observable and
is what the probe below uses. Beyond that it is a per-vendor configuration
axis — #166's axis — and it is a live ADR-0010 hazard, because `options.env`
is a way to set a provider key into the child, which rule 4 forbids. Any use
must be narrow, credential-free, and per-row rather than global.

ADR consequences: **ADR-0022**'s launch table grows a per-row configuration
notion. **ADR-0010 rule 4** has to be restated as covering `_meta` and not
only argv and process environment, or the rule has a hole in it.

### 5. Host an interactive Harness session for a full-tool lane (#508)

**[Inference]** The only route to Claude computer use that the documentation
endorses, because the gate is literally "interactive session". It is also the
most expensive thing in this note. ADR-0018 forbids it in terms — "We never
launch, embed, or wrap the Harness's TUI" — and gives the reason: "ACP mode
and interactive mode are mutually exclusive in one process. Running both costs
a second process, which is a second conversation — the split brain ADR-0008
forbids." Reversing it means writing a terminal host and giving up the
`stopReason` the Completer is built on.

A second cost the ADRs do not mention: a hosted interactive session has no
`session/new`, so ai-buddy's MCP cannot be client-supplied. It would have to
be a server the *user* installed — `claude mcp add --scope user ai-buddy …`
pointed at ADR-0026's shim — which inverts ADR-0023's "the client hands the
session its own tool endpoint".

Rank it last, and reach for it only if the probe shows the interactive gate is
absolute *and* desktop control is judged worth an ADR-0018 supersession.

### 6. Rejected: an ai-buddy Executor

ADR-0003, `docs/SPEC.md:591-594` ("An ai-buddy Executor. Desktop control
belongs to the Harness."), and out of scope per #668. Listed only so the
ranking is exhaustive.

## The smallest falsifying probe

**Top option:** #2, `cwd`. **The claim it rests on that might be false:** that
`cwd` and the per-project opt-in, rather than the Agent-SDK architecture, are
what remove computer use from a `claude` attach.

**Falsifier:** on macOS, on a Pro or Max plan, signed in through claude.ai,
with `enabledMcpServers: ["computer-use"]` recorded for ai-buddy's data folder
in `~/.claude.json`, the ACP session's own tool list still contains no
`mcp__computer-use__*` entry.

**Why this is the smallest probe:** it needs no ai-buddy code and no guessing
from behaviour, because the adapter can be asked for the Agent SDK's `init`
message directly, and **[Fact]** that message carries the authoritative lists
`tools: string[]` and `mcp_servers: { name, status }[]` (`sdk.d.ts` 0.3.257,
`SDKSystemMessage`, documented as carrying "session_id, model, working
directory, tools, MCP servers, slash commands, permission mode, and the
capabilities list for feature detection"). **[Fact]** The adapter forwards it
on request: "When set, raw SDK messages are emitted as
`extNotification("_claude/sdkMessage", message)`" (`acp-agent.ts:1179-1186`,
emitted at `3964-3967`).

**What to run** — two ACP frames, with stdin held open so the adapter does not
shut down on EOF (`index.ts:97-106`):

```sh
# macOS. Pro or Max. `claude auth status` must show a claude.ai login.
DIR="$HOME/Library/Application Support/ai-buddy"

# Precondition, done once by hand: in ~/.claude.json, under
# projects["$DIR"], set "enabledMcpServers": ["computer-use"].
# That is what /mcp -> Enable writes, for that directory's project entry.

{
  printf '%s\n' \
   '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"probe-668","version":"0"}}}' \
   "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session/new\",\"params\":{\"cwd\":\"$DIR\",\"mcpServers\":[],\"_meta\":{\"claudeCode\":{\"emitRawSDKMessages\":[{\"type\":\"system\",\"subtype\":\"init\"}]}}}}"
  sleep 45
} | npx -y @agentclientprotocol/claude-agent-acp@latest 2>/dev/null \
  | grep -o '"method":"_claude/sdkMessage".*'
```

**Success signals**, read off the one `_claude/sdkMessage` notification:

| What `tools` and `mcp_servers` show | What it means | Which option wins |
|---|---|---|
| `tools` includes `mcp__computer-use__screenshot` | The gate was scope, not architecture; the interactive rule is softer than the docs | #2 recovers desktop control — then raise the turn timeout |
| `tools` includes `Bash`, `WebSearch`, `WebFetch` but no `mcp__computer-use__*` | The interactive gate is absolute for the adapter path | #2 still recovers user MCP; desktop control needs #5 |
| `mcp_servers` lists the user's connectors or user-scope servers | Issue question 4 is answered yes for those scopes | #1 must stop implying otherwise |
| `tools` is short, or missing the preset | Something in the chain is narrowing tools, which no source read here does | Reopen this note |

**The half that needs no subscription and no macOS:** run the same two frames
twice — once with `cwd` set to the data folder, once with `cwd` set to a
directory where the user has actually run `claude mcp add` — and diff
`mcp_servers`. That isolates the `cwd` effect on MCP inheritance on any
platform, on any plan, and is enough on its own to justify or kill option #2.

**[Inference]** Run that second half first. It is cheap, it settles the larger
of the two tool-surface losses, and the first half needs hardware and a
subscription this repository cannot assume a contributor has.

No probe script landed with this note, deliberately. The recipe above is two
JSON-RPC frames and needs no product code, and a `scripts/` entry that has
never been run on the one platform that could run it would be a standing
in-tree claim about behaviour nobody has observed.

## Where this contradicts what the repository already says

Per `docs/agents/docs.md`, a note that disagrees with an ADR says so rather
than routing around it.

**ADR-0003 — worth reopening, not superseding.** It records: "At the product
level this changed on 23–24 March 2026: Claude Code and Claude Cowork perform
computer use natively on macOS… The Harness now genuinely brings its own
executor, so we do not write one." **[Inference]** That is true of the
interactive `claude` CLI and false of the ACP attach ai-buddy actually ships,
which is the Agent SDK behind Zed's adapter. The decision — ship no Executor —
still stands on its other legs, and nothing here argues for building one. But
the Consequences section is now incomplete: the reason the Functional Layer is
unavailable is not only the Pro/Max gate it names, it is also an
interactive-session gate that no subscription clears. Add the second gate to
that list when #668 is accepted.

**`README.md:157` — the `pi` row oversells the loss.** "Chat-only (no MCP)" is
accurate about *our* MCP and misleading about pi's own tools, which are intact
and local. **[Fact]** per `pi-acp`'s Limitations, quoted above.

**`docs/research/buddy-harness-two-way.md` — still correct, and incomplete.**
Its central claim holds: ACP "is the only protocol that carries 'inject a user
turn into a local agent that keeps its own tools, permissions, and memory'".
What it did not anticipate is that "its own tools" is scoped by the `cwd` the
client picks, so a client can satisfy the protocol perfectly and still hand
the agent a directory in which none of the user's configuration applies.

## Proposed follow-up issues

Titles only. Filed once Architect and Oded accept the spike, not before, and
only for the options that survive it.

- `docs(readme): Say which tool classes survive a Harness attach, per row`
- `spike(harness): Read the Claude ACP session's own tool and MCP list off the adapter's raw init message`
- `feat(harness): Attach the Harness in a directory the user's own configuration keys on`
- `docs(adr): Record that ai-buddy's cwd is the Harness's project scope`
- `feat(chat): Advertise ACP form elicitation so the Harness can ask a multiple-choice question`
- `fix(harness): A turn budget a human can answer a permission prompt inside`
- `spike(harness): Whether a hosted interactive session is the only route to Harness desktop control (#508)`
- `docs(adr): Extend ADR-0003's consequences with the interactive-session gate`

## Open, not resolved here

- **[Assumption]** Whether the `claude` computer-use gate that actually bites
  is the interactive check or the per-project opt-in. The probe answers it,
  and everything in option #5's ranking depends on which.
- **[Assumption]** Whether `opencode`, `hermes` and `grok` sessions opened by
  ai-buddy really carry the tools their vendors document. All four vendor
  claims above are documentation rather than observation; none has been read
  off a live ai-buddy attach. `scripts/probe-harness.sh` prints the handshake
  but not the agent's tool list, and ACP has no method that returns one — the
  Claude probe above works only because one adapter offers a vendor extension.
- **[Assumption]** Whether a 20-second default turn budget is survivable for
  any permission-gated native tool call, computer use or not. Cheap to
  measure, and it changes how the probe's first outcome should be read.
- **[Inference, untested]** Whether Hermes' browser tools and Grok's
  browser-shaped web automation are close enough to the Functional Layer to
  earn a product row of their own. They are not desktop control, but "the
  Harness opened a page and clicked something" may be most of what users ask
  for.
- **[Fact, unexplored]** Claude Code reserves the built-in server names
  `workspace`, `claude-in-chrome`, `computer-use`, `Claude Preview` and
  `Claude Browser`, and `claude mcp add` rejects a reserved name
  (<https://code.claude.com/docs/en/mcp>). `claude-in-chrome` is a second
  browser-automation surface with its own `--chrome` flag, and this note did
  not investigate its interactive requirements.
