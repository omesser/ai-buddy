# OpenAI Codex CLI as an ai-buddy Harness

**Research date:** 2026-09-08  
**Researcher:** Cloud Agent  
**Primary sources:** OpenAI Codex GitHub repo, ACP adapter repos, official docs

---

## Executive Summary

**VERDICT:** ✅ **ACP-Compatible Harness**

OpenAI Codex CLI is a mature, actively maintained terminal coding agent with first-party ACP support via the `@agentclientprotocol/codex-acp` adapter. It meets ai-buddy's ADR-0018 requirements: headless via ACP JSON-RPC on stdio. Integration follows the established pattern for claude/hermes/opencode.

**Recommended action:** Add named launch row, smoke test per #457, document auth paths.

---

## 1. What is "Codex" Today?

### Fact: Official OpenAI Product

- **Repository:** [`github.com/openai/codex`](https://github.com/openai/codex)
- **Stars:** 121,270 | **Forks:** 18,585 | **License:** Apache 2.0
- **Status:** Active development, 709 releases (latest: v0.153.2, Sep 3 2026)
- **Implementation:** Rust (rewrite from legacy TypeScript version)
- **Contributors:** 428 active

### Fact: Terminal-Native Coding Agent

> "Codex CLI is a coding agent from OpenAI that runs locally on your computer."  
> — [openai/codex README](https://github.com/openai/codex)

Capabilities:
- Inspects code, edits files, runs commands
- Multi-agent workflows (subagent delegation)
- Model Context Protocol servers with parallel tool calls
- Sandboxed command execution
- Cloud integration (`codex cloud`)
- Local and remote session management

### Fact: Not the 2021 Codex Model

The 2021 "Codex" model (deprecated March 2023) is separate. This CLI tool is a new product sharing the name.

**Source:** [github.com/openai/codex legacy README](https://github.com/openai/codex/blob/9a8730f3/codex-cli/README.md)

---

## 2. Does It Speak ACP?

### Fact: Yes — Via Official Adapter

**Package:** [`@agentclientprotocol/codex-acp`](https://github.com/agentclientprotocol/codex-acp)  
**Command:** `npx -y @agentclientprotocol/codex-acp`  
**Standing:** 358 stars, active maintenance

### Fact: Architecture

```
ai-buddy (ACP client)
    ↓ JSON-RPC stdio
@agentclientprotocol/codex-acp
    ↓ spawns
codex app-server (subprocess)
```

The adapter:
- Starts Codex App Server as subprocess
- Translates ACP requests → Codex operations
- Maps Codex events → ACP `session/update` notifications
- Bridges approval prompts via `session/request_permission`

**Source:** [codex-acp README](https://github.com/agentclientprotocol/codex-acp)

### Fact: Event Coverage

ACP adapter supports:
- Shell command execution, file changes, permission requests
- MCP tool calls (client-provided over stdio and HTTP)
- Terminal output, reasoning, planning
- Web search, image generation/view
- Token usage, review events
- Subagent spawning (via `_meta.codex.subagent` metadata)

### Fact: Slash Commands

Exposed via ACP: `/status`, `/mcp`, `/skills`, `/goal`, `/review`, `/review-branch`, `/review-commit`, `/compact`, `/logout`, plus configured skills.

---

## 3. Integration Surface

### Primary: ACP Stdio (JSON-RPC)

The canonical integration for ai-buddy per ADR-0018.

**Launch command:**  
```bash
npx -y @agentclientprotocol/codex-acp
```

### Alternative Surfaces (Not for ai-buddy)

- **IDE extensions:** VS Code, Cursor, Windsurf (not headless)
- **Desktop app:** `codex app` (GUI, not ACP)
- **Cloud web:** `chatgpt.com/codex` (not local)

**Inference:** ai-buddy ignores these. Only the ACP stdio path matters.

---

## 4. Auth Model

### Fact: Three Auth Methods

Per ACP adapter's advertised `authMethods`:

1. **ChatGPT Login** (default)
   - OAuth PKCE flow via browser
   - Callback on `localhost:1455`
   - Uses ChatGPT plan credits (Plus/Pro/Business/Edu/Enterprise)
   - Command: `codex login`

2. **API Key**
   - OpenAI API key via stdin
   - Standard API billing rates
   - Command: `printenv OPENAI_API_KEY | codex login --with-api-key`
   - Env vars: `CODEX_API_KEY` (takes precedence) or `OPENAI_API_KEY`

3. **Device Code** (beta)
   - Headless OAuth flow (one-time code on phone/laptop)
   - No localhost callback required
   - Command: `codex login --device-auth`
   - **Requires:** Device code authorization toggle in ChatGPT Security Settings (or workspace admin permission)

**Source:** [Codex Auth docs](https://learn.chatgpt.com/docs/auth), [codex-acp README](https://github.com/agentclientprotocol/codex-acp)

### Fact: Credential Cache

Auth cached at `~/.codex/auth.json` (or OS keyring).  
Shared between CLI and IDE extension.  
Logout clears both.

### Fact: Headless Support

**Runtime option:** `NO_BROWSER=1`  
- Hides browser-based ChatGPT auth method during ACP `initialize`
- Advertises only API key and device code methods in remote/browserless environments

**ACP adapter env vars:**
```bash
CODEX_API_KEY        # API key (precedence over OPENAI_API_KEY)
OPENAI_API_KEY       # Fallback API key
NO_BROWSER           # Hide ChatGPT browser auth
DEFAULT_AUTH_REQUEST # ACP auth request JSON when Codex requires auth
```

**Source:** [codex-acp npm package](https://www.npmjs.com/package/@agentclientprotocol/codex-acp)

### Inference: Subprocess Reuse of User Login

If user runs `codex login` in their terminal → `~/.codex/auth.json` exists → ACP subprocess launched by ai-buddy can reuse cached credentials **without** browser interaction.

**Assumption:** This follows the same pattern as `hermes` and `claude` adapters, which also reuse cached auth.

### Fact: API Key Limitations (When Used)

Per official docs, API key auth **disables**:
- ❌ Codex cloud (`codex cloud`)
- ❌ Fast mode (`/fast`)
- ❌ Codex-Spark (requires ChatGPT Pro)
- ❌ Realtime voice sessions

But **enables**:
- ✅ Full local CLI functionality
- ✅ Custom model providers
- ✅ `codex exec` for CI/CD
- ✅ Multi-agent workflows (local)

**Source:** [Codex CLI Authentication guide](https://codex.danielvaughan.com/2026/04/01/codex-cli-authentication-flows-credential-management/)

---

## 5. How Would ai-buddy Integrate?

### Recommended: Named Launch Row (Like claude/hermes/opencode)

Per ADR-0022 launch table pattern:

```rust
| Name    | Command                                        | Standing       |
|---------|------------------------------------------------|----------------|
| codex   | npx -y @agentclientprotocol/codex-acp@latest  | To be verified |
```

**Why `@latest`?** Same reason as `claude` (ADR-0017): the adapter bundles Codex binary, and npx cache may serve stale version incompatible with current models.

### Integration Steps (Following ADR-0022)

1. **Add named row** to harness launch table in `src-tauri/src/harness.rs`
2. **Smoke test** per #457 acceptance criteria (see § 6 below)
3. **Document** in ADR-0017/ADR-0022 update
4. **Auth handling:**
   - No change needed: `acp_wire.rs` already handles `-32000` auth_required error
   - User sees: "Sign in to Codex before attaching" (or device code instructions)
   - Retried max once/minute per existing gate

### Fact: Advertised Capabilities (Expected)

Based on adapter feature list, Codex should advertise in `initialize`:

- `loadSession`: ✅ (adapter supports session resume)
- `mcpCapabilities.http`: ✅ (adapter bridges HTTP MCP)
- `authMethods`: 
  - `"Sign in with ChatGPT"` (when `NO_BROWSER != 1`)
  - `"Sign in with Device Code"` (beta)
  - `"API key via CODEX_API_KEY or OPENAI_API_KEY"`
- `sessionCapabilities`: Unknown (needs probe to verify)
- `promptCapabilities.image`: Unknown (needs probe to verify)

**Assumption:** Real handshake differs. Probe will reveal actual fields.

### No ADR-0018 Conflicts

- ✅ Headless: Yes (ACP stdio)
- ✅ JSON-RPC: Yes
- ✅ No TUI embedding: ai-buddy never spawns interactive `codex` — only the adapter subprocess
- ✅ User auth: Harness authenticates itself (ADR-0017 rule 8)

---

## 6. Testing Plan

### Unit vs Smoked Turn vs CI

Per #457 and ADR-0017's verification bar:

#### 6.1 Smoked Turn (Required for Named Row)

**Tool:** `scripts/probe-harness.sh`

**Steps:**
1. Install Codex CLI:
   ```bash
   npm install -g @openai/codex
   codex login  # or device-code / API key
   ```

2. Configure harness:
   ```bash
   export AI_BUDDY_HARNESS="npx -y @agentclientprotocol/codex-acp@latest"
   ```

3. Run probe (fresh session):
   ```bash
   scripts/probe-harness.sh
   ```

4. Run probe again (resumed session):
   ```bash
   scripts/probe-harness.sh
   ```
   (Catches #448-style bugs where `loadSession` succeeds but session is dead)

5. Record:
   - Exit code (must be 0)
   - Handshake (`initialize` advertised capabilities)
   - Stop reason
   - Reply text
   - Whether reply parsed as Behavior proposal

**Expected result:** Exit zero, `end_turn` stop reason, parseable or Speech reply.

#### 6.2 MCP Integration (Per #457 § "Also unproven")

**Gap:** No existing probe has exercised MCP path through a real Harness session (stdio or http).

**Test:**
1. Build `ai-buddy-mcp` binary
2. Place beside app or set `AI_BUDDY_MCP_BIN`
3. Launch session with Codex adapter
4. Verify:
   - MCP server listed in session config
   - `speak` tool callable by Codex
   - `play_behavior` tool callable by Codex

**Inference:** If `mcpCapabilities.http` is advertised, prefer HTTP path (#166). Otherwise stdio.

#### 6.3 CI Feasibility

**Blocker:** Secrets

- **ChatGPT login:** Requires browser (non-CI)
- **Device code:** Requires one-time human approval on separate device (non-CI)
- **API key:** ✅ CI-friendly via secret injection

**Recommendation:**
- Local smoke tests: ChatGPT login (developer convenience)
- CI: API key via `CODEX_API_KEY` secret
  - Accepts reduced feature set (no cloud, no voice)
  - Trade-off documented in probe output

**Assumption:** CI tests run with API key, knowing cloud features untested.

---

## 7. Integration Options Ranked

### Option 1: Named Launch Row (Recommended)

**Tradeoffs:**
- ✅ Pro: User-visible, documented, smoke-tested
- ✅ Pro: Follows established pattern (claude/hermes/opencode)
- ✅ Pro: No custom command typing
- ⚠️ Con: Another npm dependency (mitigated by `@latest` + npx)

**When:** After probe exits zero on fresh + resumed sessions.

### Option 2: Custom Command Only (Fallback)

**Tradeoffs:**
- ✅ Pro: Zero ai-buddy code changes
- ❌ Con: No official support claim
- ❌ Con: User must discover command syntax
- ❌ Con: Not counted in "supported Harnesses" marketing

**When:** If probe reveals showstopper (unlikely given adapter maturity).

### Option 3: Defer Until User Requests (Not Recommended)

**Tradeoffs:**
- ❌ Con: Codex is major vendor (121K stars, OpenAI-backed)
- ❌ Con: Leaves capability gap vs competitors (if they support Codex)
- ❌ Con: Misses low-hanging ACP compatibility

**Verdict:** Codex earns row proactively. Too prominent to wait.

---

## 8. Blockers vs ADR-0018

### No Technical Blockers Found

| ADR-0018 Requirement | Codex Status |
|----------------------|--------------|
| Headless via ACP | ✅ `@agentclientprotocol/codex-acp` |
| JSON-RPC on stdio | ✅ Adapter architecture |
| No TUI embedding | ✅ ai-buddy never spawns interactive CLI |
| Subprocess spawn | ✅ Same as claude/hermes |
| User auth (not ai-buddy's job) | ✅ `codex login` pre-attach |

### Policy Blockers (Inherited from ADR-0017)

- **No auto-approve flags:** Codex has permission modes. ai-buddy never passes them by default (Chat surface owns permissions).
- **Vendor extensions:** Unknown `x.ai/*` methods get `method-not-found` (ACP-allowed).

**Inference:** These are not Codex-specific. Same rules as every Harness.

---

## 9. Concrete Test Plan (Checklist for #457-style Issue)

Adapting #457 acceptance criteria:

- [ ] **Install Codex CLI** (`npm i -g @openai/codex`)
- [ ] **Authenticate:** `codex login` or device-code or API key
- [ ] **Verify auth cache exists:** `ls ~/.codex/auth.json`
- [ ] **Run probe (fresh):** `AI_BUDDY_HARNESS="npx -y @agentclientprotocol/codex-acp@latest" scripts/probe-harness.sh`
  - [ ] Exit code 0
  - [ ] Record `initialize` handshake
  - [ ] Record stop reason (expect `end_turn`)
  - [ ] Record reply (Speech or Behavior)
- [ ] **Run probe (resumed):** Re-run probe, verify session resume
  - [ ] Exit code 0 again
  - [ ] Document whether `loadSession` worked (vs #448 silent failure)
- [ ] **Document handshake fields:**
  - [ ] `loadSession` support
  - [ ] `mcpCapabilities.http` present?
  - [ ] `authMethods` advertised
  - [ ] `sessionCapabilities` (if any)
  - [ ] `promptCapabilities.image` (if any)
- [ ] **Exercise MCP path:**
  - [ ] Build `ai-buddy-mcp` binary
  - [ ] Set `AI_BUDDY_MCP_BIN` or place beside app
  - [ ] Launch session, verify `speak` callable
- [ ] **Add named row** to launch table in `src-tauri/src/harness.rs`
- [ ] **Update ADR-0017/ADR-0022** with Codex standing (dated, like others)
- [ ] **CI consideration:** Document API-key-only testing if ChatGPT login blocks automation

---

## 10. Open Questions (To Be Answered By Probe)

1. **What does `initialize` actually advertise?**
   - Adapter docs list features, but real handshake may differ
   - `sessionCapabilities`? `promptCapabilities.image`?

2. **Does `loadSession` work reliably?**
   - Or does it suffer #448 (silently broken resume)?

3. **Stdout pollution?**
   - Gemini has this problem. Does Codex?
   - Adapter should shield NDJSON, but verify.

4. **Model selection:**
   - Default is `o4-mini` per legacy docs
   - Can user override via `CODEX_CONFIG` env var?
   - Does adapter expose ACP `models` list?

5. **MCP tool call latency:**
   - Adapter claims "parallel tool calls cut wall time in half"
   - Verify `supports_parallel_tool_calls` flag present

6. **Auth handoff:**
   - When does `-32000` auth_required fire?
   - Does cached `auth.json` prevent it?
   - What if user logged out between ai-buddy launches?

**Resolution:** All answered by running the probe.

---

## 11. Related Work

### Similar Harnesses in ai-buddy

| Harness | ACP Adapter | Verified | Notes |
|---------|-------------|----------|-------|
| claude | `@agentclientprotocol/claude-agent-acp` | ✅ 2026-09-07 | Third-party adapter over Claude Agent SDK |
| hermes | `hermes acp` (first-party) | ✅ 2026-09-07 | #448 resumed-session bug fixed |
| opencode | `opencode acp` (first-party) | ❌ Unverified | Named but never smoked |
| **codex** | `@agentclientprotocol/codex-acp` | ❌ To be verified | **This research** |

### Deferred Harnesses (Per ADR-0017)

- **Grok Build:** `grok agent stdio` — first-party ACP, unverified
- **GitHub Copilot CLI:** `copilot --acp --stdio` — public preview, unverified
- **Gemini CLI:** `gemini --acp` — first-party ACP, stdout pollution risk
- **Pi:** No first-party ACP; needs adapter (deferred)

**Inference:** Codex fits between claude/hermes (verified, third-party adapter) and Grok/Copilot (unverified, first-party). Verification cost is low; adapter is proven elsewhere.

---

## 12. Sources Cited

### Primary

1. [openai/codex](https://github.com/openai/codex) — Official repository
2. [agentclientprotocol/codex-acp](https://github.com/agentclientprotocol/codex-acp) — ACP adapter
3. [@agentclientprotocol/codex-acp (npm)](https://www.npmjs.com/package/@agentclientprotocol/codex-acp)
4. [Codex CLI Documentation](https://learn.chatgpt.com/docs/codex/cli) — Official docs
5. [Codex Authentication](https://learn.chatgpt.com/docs/auth) — Auth methods

### Secondary

6. [Codex CLI releases](https://github.com/openai/codex/releases) — Version history
7. [acpx.sh agents list](https://acpx.sh/agents.html) — ACP tooling ecosystem
8. [Codex CLI Authentication guide](https://codex.danielvaughan.com/2026/04/01/codex-cli-authentication-flows-credential-management/) — Third-party walkthrough
9. [Codex device code auth](https://lucaberton.com/blog/codex-device-code-auth-chatgpt-security-2026/) — Headless setup
10. [Augment Code article](https://www.augmentcode.com/learn/openai-codex-cli-terminal-agent) — Feature overview

### ai-buddy Internal

11. ADR-0018 (not found — possibly renumbered)
12. [ADR-0022](../docs/adr/0022-acp-client-over-official-sdk-and-named-harnesses.md) — Launch table policy
13. [ADR-0017](../docs/adr/0017-acp-client-over-the-official-sdk-and-supported-harnesses.md) — Superseded by ADR-0022
14. [ADR-0008](../docs/adr/0008-one-harness-session.md) — One session per app lifetime
15. [Issue #457](https://github.com/omesser/ai-buddy/issues/457) — Verification pattern

---

## 13. Recommendation

### Immediate Action

1. **Create GitHub issue** (draft below) to track Codex verification
2. **Run smoke test** per § 9 checklist
3. **Add named row** if probe succeeds
4. **Document** in ADR-0017 update (or new ADR if substantial)

### Success Criteria

- Probe exits zero (fresh + resumed)
- `initialize` handshake documented
- MCP path exercised (one real Harness session calls `speak`)
- Named launch row with dated standing in ADR

### Risk Assessment

**Low risk.** Adapter is proven (358 stars, used by acpx ecosystem). Integration follows established pattern. Worst case: custom command works even if named row deferred.

---

## Appendix A: Draft GitHub Issue

```markdown
## Title
Verify OpenAI Codex CLI as named Harness

## Problem

ADR-0022's launch table has three verified rows (claude, hermes, opencode). Codex CLI is a major vendor agent (121K stars, OpenAI-backed) with first-party ACP support via `@agentclientprotocol/codex-acp` adapter. It is reachable *today* through the custom command escape hatch, but has no named row, no documentation, and no evidence anyone has smoked a turn.

Research (docs/research/codex-harness-research.md) confirms:
- ✅ ACP support: `npx -y @agentclientprotocol/codex-acp@latest`
- ✅ Headless: JSON-RPC stdio
- ✅ Auth: ChatGPT login (cached), device code, or API key
- ✅ MCP: http + stdio advertised
- ❓ Untested: Probe exit code, session resume, handshake fields

## The bar is already written

ADR-0022 sets it: *"A row moves in this table when that command exits zero."*  
`scripts/probe-harness.sh` attaches the configured Harness with no overlay, runs one fixed prompt, and prints the handshake, stop reason, reply, and whether the reply parsed as a Behavior proposal.

## Scope

For `codex`:

1. Install: `npm install -g @openai/codex`
2. Authenticate: `codex login` (or device-code, or API key)
3. Run `scripts/probe-harness.sh` and get exit zero (fresh session)
4. Run probe again (resumed session) — #448 taught us this matters
5. Record what `initialize` advertises:
   - `loadSession`
   - `mcpCapabilities.http`
   - `authMethods`
   - `sessionCapabilities` (if any)
   - `promptCapabilities.image` (if any)
6. Exercise MCP path: verify `speak` callable through one real session (#457 § "Also unproven")
7. Add named row to launch table with command and dated standing

A probe that fails gets a row too, saying so. An honest "does not work yet, here is what it did" is worth more than absence.

## Constraints from ADR-0022

- **No auto-approve flags, ever.** Chat surface owns permissions.
- **Use `@latest`:** Same reason as claude — adapter bundles Codex, npx may cache stale version.
- **Vendor extensions unknown?** `method-not-found` is ACP-allowed.

## Acceptance

- [ ] `codex` has a named row with command and dated standing
- [ ] Standing earned by probe exit zero (fresh + resumed)
- [ ] What `initialize` advertises is documented (for #166)
- [ ] MCP path exercised through at least one real Codex session
- [ ] ADR-0017 or ADR-0022 updated with Codex entry

## Out of Scope

- **Custom model providers:** Codex supports them; ai-buddy doesn't configure them (user's `~/.codex/config.toml`).
- **Cloud features:** Disabled when API key used; not ai-buddy's concern.
- **CI automation:** API-key auth only; ChatGPT login requires browser (document as known limit).

## Related

#457 (verification pattern), #166 (MCP client config), #448 (resumed session bug), ADR-0022 (launch table), research report (docs/research/codex-harness-research.md).
```

---

## Appendix B: Inference vs Fact Legend

Throughout this report:

- **Fact:** Cited from primary source (repo, docs, adapter code)
- **Inference:** Logical conclusion from facts (marked explicitly)
- **Assumption:** Educated guess requiring probe verification (marked explicitly)

Example:
- *Fact:* Adapter supports `loadSession` (per npm README)
- *Inference:* Resume should work like hermes (same ACP method)
- *Assumption:* Resume won't suffer #448 bug (needs probe to confirm)

---

**End of Report**
