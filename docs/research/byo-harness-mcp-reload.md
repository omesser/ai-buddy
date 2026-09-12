# BYO Harness MCP Reconfiguration and Reload

Research on how each Harness handles MCP server **reconfiguration** and **reload** after the URL or bearer token changes — complementing #580's initial registration research.

## Context

Draft PR #599 implements Settings UI that generates MCP registration snippets for BYO (Bring Your Own) harnesses. The ai-buddy MCP endpoint binds a random port and rotates its bearer token every launch, so users need to **reconfigure** their harness each time ai-buddy restarts.

This research answers:
1. How to **add** an HTTP MCP server with custom `Authorization` bearer (CLI / config)
2. How to **reload** after config changes (without full process restart if possible)
3. Does re-adding an existing server name keep stale URL/token?
4. What should ai-buddy generate: CLI snippet, config fragment, or both?

**Product constraint (Oded)**: Prefer MCP **reconfiguration** methods over harness **launch** CLIs. ai-buddy tells users how to point their running Harness at ai-buddy's MCP, not how to launch the Harness itself.

## Summary Table

| Harness | Add with Auth | Reload Without Restart | Re-add Trap | Generate |
|---------|---------------|------------------------|-------------|----------|
| **Claude Code** | `claude mcp add --header "Authorization: Bearer <token>"` | **No** — must exit and restart session | **Yes** — silently keeps stale | CLI with `remove` then `add` |
| **Codex** | Config: `~/.codex/config.toml` with `headers` | **Yes** — `mcpServer/refresh` command | Unknown | Config snippet + instructions |
| **Hermes** | Config: `~/.hermes/config.yaml` with `headers` | **Yes** — `/reload-mcp` | Unknown | Config snippet + `/reload-mcp` |
| **OpenCode** | Config: `mcp.servers.<name>` with `headers` | **Yes** — `/reload` | Unknown | Config snippet + `/reload` |
| **Grok** | `grok mcp add` with `--header`, or config TOML | **Yes** — `/mcps` then press `r` | Unknown | CLI or config + `/mcps` + `r` |
| **Pi** | Config: `.mcp.json` or `~/.pi/agent/mcp.json` | **Yes** — `/reload` + `/mcp reconnect` | Unknown | Config snippet + `/reload` |

### Legend
- **Fact**: Verified by documentation or execution
- **Inference**: Reasonable deduction from documented behavior
- **Assumption**: Unverified; needs testing with real CLI

## Per-Harness Findings

### Claude Code

**Add with bearer token**: (**Fact**)
```bash
claude mcp add --transport http \
  --header "Authorization: Bearer <token>" \
  ai-buddy "http://127.0.0.1:<port>/mcp"
```

Config lives in:
- `--scope local` (default): `.claude/config/local.json` in project
- `--scope user`: `~/.claude.json` under `mcpServers`
- `--scope project`: `.mcp.json` in project root

**Reload mechanism**: (**Fact**)
- **No native reload without restart**. Must exit session and start new one.
- Config files are read at session startup only.
- `/mcp reconnect` only re-establishes connections for already-configured servers; does **not** pick up new servers or changed URLs/tokens.
- Feature requests exist (#34893, #46426) but not implemented as of 2026-09.

**Re-add trap**: (**Fact**)
```bash
# First add
claude mcp add --transport http --scope user \
  --header "Authorization: Bearer token1" \
  ai-buddy "http://127.0.0.1:58819/mcp"

# ai-buddy restarts, new port and token
# Re-add with same name
claude mcp add --transport http --scope user \
  --header "Authorization: Bearer token2" \
  ai-buddy "http://127.0.0.1:61234/mcp"

# Result: EXIT 0, prints "Server already exists"
# Config KEEPS OLD VALUES (port 58819, token1)
# Must remove first to actually update
```

Issue #580 confirmed this trap: Claude Code silently ignores re-add of an existing name, keeping the stale entry.

**What to generate**: (**Inference**)
```bash
# Remove stale entry, then add fresh one
claude mcp remove -s user ai-buddy 2>/dev/null
claude mcp add -s user --transport http ai-buddy \
  "http://127.0.0.1:<port>/mcp" \
  --header "Authorization: Bearer <token>"
```

Instructions: "Exit your Claude session and start a new one (`claude`) to connect."

---

### Codex

**Add with bearer token**: (**Fact** from docs)

Config: `~/.codex/config.toml`
```toml
[mcp_servers.ai-buddy]
url = "http://127.0.0.1:<port>/mcp"
headers = { "Authorization" = "Bearer <token>" }
```

**Reload mechanism**: (**Fact** from source)
- **Yes** — `mcpServer/refresh` command triggers hot reload
- Reloads config for active threads, preserves thread-local overrides
- Pull request #8957 added this feature
- Best-effort refresh available; strict mode fails loudly

**Re-add trap**: (**Assumption**)
Unknown. Config file editing would naturally overwrite existing values, but behavior after `mcpServer/refresh` is not documented for URL/token changes specifically.

**What to generate**: (**Inference**)
TOML config fragment:
```toml
[mcp_servers.ai-buddy]
url = "http://127.0.0.1:<port>/mcp"
headers = { "Authorization" = "Bearer <token>" }
```

Instructions: "Add or update this in `~/.codex/config.toml`, then run `mcpServer/refresh` from your Codex session."

**Note**: Codex CLI availability varies. If `mcpServer/refresh` is unavailable, fallback: "Restart your Codex session."

---

### Hermes

**Add with bearer token**: (**Fact** from docs)

Config: `~/.hermes/config.yaml`
```yaml
mcp_servers:
  ai-buddy:
    url: "http://127.0.0.1:<port>/mcp"
    headers:
      Authorization: "Bearer <token>"
```

**Reload mechanism**: (**Fact**)
- **Yes** — `/reload-mcp` command
- Also: auto-reload watcher detects config changes (30s timeout for interactive flows)
- Pull request #1474 added auto-reload
- Known issue #14716: failed reloads may not retry until config changes again

**Re-add trap**: (**Assumption**)
Unknown. YAML file editing would overwrite existing values. No trap documented similar to Claude's silent ignore.

**What to generate**: (**Inference**)
YAML config fragment:
```yaml
mcp_servers:
  ai-buddy:
    url: "http://127.0.0.1:<port>/mcp"
    headers:
      Authorization: "Bearer <token>"
```

Instructions: "Add or update this in `~/.hermes/config.yaml`, then run `/reload-mcp` in your Hermes session."

**Alternative**: "Or just edit the config; Hermes will auto-reload within 30 seconds."

---

### OpenCode

**Add with bearer token**: (**Fact** from docs)

Config: `opencode.jsonc` (project or global `~/.config/opencode/opencode.jsonc`)
```jsonc
{
  "mcp": {
    "servers": {
      "ai-buddy": {
        "type": "http",
        "url": "http://127.0.0.1:<port>/mcp",
        "headers": {
          "Authorization": "Bearer <token>"
        }
      }
    }
  }
}
```

Note: V2 uses `mcp.servers.<name>`, not top-level `mcp.<name>`. Uses `disabled: true/false`, not `enabled`.

**Reload mechanism**: (**Fact**)
- **Yes** — `/reload` command
- Reloads entire config (global + project), disposes/recreates instances including MCP
- Issue #6719 implemented this in 2026
- Also: runtime API `PUT /api/mcp/{server}` can add/replace servers

**Re-add trap**: (**Assumption**)
Unknown. JSON file editing would overwrite existing values. No documented trap.

**What to generate**: (**Inference**)
JSON config fragment (with trailing comma tolerance):
```jsonc
{
  "mcp": {
    "servers": {
      "ai-buddy": {
        "type": "http",
        "url": "http://127.0.0.1:<port>/mcp",
        "headers": {
          "Authorization": "Bearer <token>"
        }
      }
    }
  }
}
```

Instructions: "Add or update this in your `opencode.jsonc`, then run `/reload` in your OpenCode session."

---

### Grok

**Add with bearer token**: (**Fact** from docs)

CLI:
```bash
grok mcp add --transport http \
  --header "Authorization: Bearer <token>" \
  ai-buddy "http://127.0.0.1:<port>/mcp"
```

Config: `~/.grok/config.toml` or `.grok/config.toml` (project-scoped)
```toml
[mcp_servers.ai-buddy]
url = "http://127.0.0.1:<port>/mcp"
headers = { "Authorization" = "Bearer <token>" }
```

**Reload mechanism**: (**Fact**)
- **Yes** — `/mcps` opens MCP management modal
  - Press `r` to refresh config changes
  - Press `Space` to toggle server on/off
  - Press `i` to authenticate OAuth
  - Press `a`/`x` to add/remove
- Also: CLI `grok mcp enable/disable <name>`
- Reload reconnects existing servers and picks up new ones

**Re-add trap**: (**Assumption**)
Unknown. Config editing or `grok mcp add` (if it overwrites) would replace values. No documented trap like Claude's.

**What to generate**: (**Inference**)
Option 1 — CLI:
```bash
# Remove if exists, then add
grok mcp remove ai-buddy 2>/dev/null
grok mcp add --transport http \
  --header "Authorization: Bearer <token>" \
  ai-buddy "http://127.0.0.1:<port>/mcp"
```

Option 2 — Config:
```toml
[mcp_servers.ai-buddy]
url = "http://127.0.0.1:<port>/mcp"
headers = { "Authorization" = "Bearer <token>" }
```

Instructions: "Run `/mcps` in your Grok session, then press `r` to reload."

---

### Pi

**Add with bearer token**: (**Fact** from docs)

Config: `.mcp.json` (project) or `~/.pi/agent/mcp.json` (agent dir)
```json
{
  "mcpServers": {
    "ai-buddy": {
      "url": "http://127.0.0.1:<port>/mcp",
      "headers": {
        "Authorization": "Bearer <token>"
      }
    }
  }
}
```

Pi uses standard `.mcp.json` immediately if present.

**Reload mechanism**: (**Fact**)
- **Yes** — `/reload` picks up config changes
- **Also** — `/mcp reconnect <server>` or `/mcp reconnect` (all) refreshes tool metadata
- `/mcp enable/disable <server>` toggles without editing config
- Changes take effect on next `/reload`

**Re-add trap**: (**Assumption**)
Unknown. JSON file editing would overwrite existing values. No documented trap.

**What to generate**: (**Inference**)
JSON config fragment:
```json
{
  "mcpServers": {
    "ai-buddy": {
      "url": "http://127.0.0.1:<port>/mcp",
      "headers": {
        "Authorization": "Bearer <token>"
      }
    }
  }
}
```

Instructions: "Add or update this in `.mcp.json` (project) or `~/.pi/agent/mcp.json`, then run `/reload` followed by `/mcp reconnect ai-buddy` in your Pi session."

---

## Cross-Cutting Findings

### 1. All Support Bearer Token in Config

**Fact**: Every harness supports custom `Authorization` headers, either via:
- CLI flag: `--header "Authorization: Bearer <token>"`
- Config field: `headers` map/object with `Authorization` key

This confirms #580's finding: the stdio shim is never required for bearer auth.

### 2. Reload Support Varies Widely

| Has Native Reload | Harness | Method |
|-------------------|---------|--------|
| ✅ Yes | Hermes | `/reload-mcp` |
| ✅ Yes | OpenCode | `/reload` |
| ✅ Yes | Grok | `/mcps` + `r` |
| ✅ Yes | Pi | `/reload` + `/mcp reconnect` |
| ✅ Yes | Codex | `mcpServer/refresh` |
| ❌ **No** | **Claude Code** | Must restart session |

**Inference**: Five of six harnesses support mid-session reload. Only Claude Code requires full session restart.

### 3. The Claude Code Re-Add Trap

**Fact** (#580): `claude mcp add` with an existing name exits 0 but **silently keeps the old URL and token**. This is unique to Claude Code; other harnesses either:
- Overwrite via config file edit (Hermes, OpenCode, Grok, Pi, Codex)
- Or behavior is unknown but no trap documented

**Implication for ai-buddy**: Generated snippet for Claude Code **must** include `claude mcp remove` first.

### 4. Config File Locations

| Harness | User/Global Config | Project Config |
|---------|-------------------|----------------|
| Claude Code | `~/.claude.json` | `.mcp.json` or `.claude/config/local.json` |
| Codex | `~/.codex/config.toml` | — |
| Hermes | `~/.hermes/config.yaml` | — |
| OpenCode | `~/.config/opencode/opencode.jsonc` | `opencode.jsonc` |
| Grok | `~/.grok/config.toml` | `.grok/config.toml` |
| Pi | `~/.pi/agent/mcp.json` | `.mcp.json` or `.pi/mcp.json` |

### 5. Session Lifecycle vs Process Lifecycle

**Key distinction**:
- **Claude Code session** = one `claude` invocation. Exiting and re-running `claude` is a "restart." No window/app to close.
- **Other harnesses**: Similar — terminal-based sessions where "restart" means exit and re-invoke the command.

This means "restart" is less disruptive than it sounds: the user types the command again, doesn't lose unsaved work in an app, and conversation history may be preserved depending on the harness.

---

## Recommendations for PR #599 Settings UI

### 1. Generate Appropriate Snippet Per Harness

| Harness | Generate | Format |
|---------|----------|--------|
| Claude Code | CLI with `remove` + `add` | Bash |
| Codex | Config TOML fragment | TOML |
| Hermes | Config YAML fragment | YAML |
| OpenCode | Config JSON fragment | JSON |
| Grok | CLI **or** config TOML | Bash or TOML |
| Pi | Config JSON fragment | JSON |

**Rationale**:
- CLI is best when it's the primary method and config files are opaque (Claude Code, optionally Grok)
- Config snippets are best when files are user-edited and location is standard (all others)

### 2. Include Reload Instructions Per Harness

| Harness | Reload Instruction |
|---------|-------------------|
| Claude Code | "Exit your Claude session and start a new one with `claude`." |
| Codex | "Run `mcpServer/refresh` from your Codex session." (or "Restart your Codex session" if refresh unavailable) |
| Hermes | "Run `/reload-mcp` in your Hermes session." |
| OpenCode | "Run `/reload` in your OpenCode session." |
| Grok | "Run `/mcps`, then press `r` to reload." |
| Pi | "Run `/reload`, then `/mcp reconnect ai-buddy`." |

### 3. Tooltip: "Token changes every launch"

**Fact**: ai-buddy's MCP endpoint binds `127.0.0.1:0` (random port) and generates a fresh 32-byte token per run. Decided by @omesser on #577: surface this in a tooltip.

Suggested text: "The URL and token change every time ai-buddy launches. Re-run this command/snippet each time."

### 4. Claude Code Snippet Must Remove First

**Critical**: Because `claude mcp add` silently keeps stale values when the name exists, the generated snippet **must** be:

```bash
claude mcp remove -s user ai-buddy 2>/dev/null
claude mcp add -s user --transport http ai-buddy \
  "http://127.0.0.1:<port>/mcp" \
  --header "Authorization: Bearer <token>"
```

The `2>/dev/null` suppresses "not found" errors when the entry doesn't exist yet.

### 5. Scope Selection (Claude Code)

Claude Code has three scopes: `local` (project-only), `user` (all projects), `project` (shared with teammates).

**Recommendation**: Default to `--scope user` for BYO scenarios. Reasoning:
- `local`: Requires re-add per project directory
- `user`: Works across all projects for this user
- `project`: Writes `.mcp.json` teammates would commit, but the token is per-run and won't work for them

If scope selector is added to UI, offer `user` and `local`; `project` is inappropriate for per-run credentials.

### 6. Config Snippet Formatting

For harnesses that take config files, show:
1. The **minimal addition** — just the `ai-buddy` server entry
2. Path where it goes: "`~/.hermes/config.yaml` under `mcp_servers:`"
3. Merge instructions: "Add or replace the `ai-buddy` entry."

Example for Hermes:
```yaml
# Add or replace this in ~/.hermes/config.yaml under mcp_servers:
ai-buddy:
  url: "http://127.0.0.1:<port>/mcp"
  headers:
    Authorization: "Bearer <token>"
```

### 7. Consider "Custom" Fallback

For harnesses not in the picker (future-proofing), or if a user runs a different CLI:

**Generate**: Generic HTTP MCP registration shape
```
URL: http://127.0.0.1:<port>/mcp
Authorization: Bearer <token>
```

Instructions: "Configure your harness to connect to this HTTP MCP server with the Authorization header."

---

## Comparison to #580 (Initial Registration Research)

#580 answered: "How to **initially register** an MCP server with bearer auth?"

This research answers: "How to **reconfigure** after the URL/token changes?"

Key difference:
- **Registration** (one-time): Which config file, what syntax, does it work at all?
- **Reconfiguration** (every ai-buddy launch): Can you reload without restart? Do you need to remove first?

Findings that changed:
- **Reload is possible** on 5/6 harnesses (only Claude Code requires restart)
- **Claude Code re-add trap confirmed**: Must remove before add
- **Config snippets are viable** for most harnesses (not just CLI)

---

## Open Questions (For Future Testing)

1. **Codex `mcpServer/refresh`**: Does it pick up changed URL/token for an existing server name, or does it have a trap like Claude Code?
2. **Hermes auto-reload**: What happens if reload fails (issue #14716)? Does manual `/reload-mcp` recover?
3. **Grok `grok mcp add` re-add**: Does it overwrite or ignore like Claude Code?
4. **Pi `/mcp reconnect` necessity**: Is `/reload` alone sufficient, or is `/mcp reconnect` required for tool list refresh?
5. **All harnesses**: If the ai-buddy process dies while the harness is connected, how do they recover? Auto-reconnect vs manual?

These are answerable with execution once harness CLIs are installed.

---

## References

### Documentation Sources

- **Claude Code MCP docs**: https://code.claude.com/docs/en/mcp
- **Claude Code MCP quickstart**: https://code.claude.com/docs/en/mcp-quickstart
- **Claude Code issue #34893**: [Feature Request] Add `/restart` command to reload MCPs
- **Claude Code issue #46426**: Feature request: hot-reload MCP servers without restarting
- **Hermes MCP docs**: https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp
- **Hermes config reference**: https://hermes-agent.nousresearch.com/docs/reference/mcp-config-reference
- **Hermes PR #1474**: fix: auto-reload MCP tools when mcp_servers config changes
- **Hermes issue #14716**: MCP config watcher records failed reloads as applied
- **OpenCode MCP docs**: https://opencode.ai/v2/docs/mcp-servers/
- **OpenCode migrate V1**: https://opencode.ai/v2/docs/migrate-v1/
- **OpenCode issue #6719**: [FEATURE] slash command for reload
- **Grok MCP docs**: https://docs.x.ai/build/features/mcp-servers
- **Grok commands cheat sheet**: https://toolsbase.dev/en/reference/grok-build-commands
- **Codex source**: `codex-rs/app-server/src/mcp_refresh.rs`
- **Codex PR #8957**: feat: hot reload mcp servers
- **Codex issue #21055**: Preserve session MCP config on refresh
- **Pi MCP adapter**: https://pi.dev/packages/pi-mcp-adapter
- **Pi commands docs**: https://mintlify.wiki/nicobailon/pi-mcp-adapter/usage/commands

### Related ai-buddy Issues/PRs

- **#577**: Surface the MCP endpoint so a Harness you run yourself can reach ai-buddy
- **#580**: spike(harness): How each Harness registers a BYO MCP server
- **#599**: feat(settings): Generate the MCP registration for a Harness you run yourself (draft PR)
- **ADR-0010**: Credential rules, loopback-only MCP, token stays out of logs
- **ADR-0023**: Dispatch inside the running app, loopback HTTP MCP
- **ADR-0026**: stdio MCP binary is a relay shim

---

## Change Log

- **2026-09-12**: Initial research document created. Covers six harnesses (Claude Code, Codex, Hermes, OpenCode, Grok, Pi). Documented reload mechanisms, re-add trap (Claude Code), and config locations. Recommendations for PR #599 Settings UI.

_— Cursor agent (Coder), on [@omesser](https://github.com/omesser)'s behalf._
