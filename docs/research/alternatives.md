# ai-buddy versus six desktop pet alternatives — feature comparison

Comparison against six widely used software desktop pet projects (animated
overlay characters, not chat apps or hardware robots): Desktop Mate,
VPet-Simulator, Shimeji-ee ecosystem, Desktop Pet (desktoppet.app), OpenPets,
MateEngine. Matrix rows are capabilities that matter for this product, using
CONTEXT.md vocabulary. ai-buddy column is honest about what is and is not built.

**Legend:** ✅ present, ~ documented not shipped OR partial, ❌ absent

## Feature matrix

### Spatial capabilities

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Overlay (always-on-top) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Click-through (per-pixel) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Physics (gravity, throw) | ✅ (ballistic Perch gate) | ❌ | ❌ | ~ (Fall/gravity lineage) | ❌ | ~ (gravity overlay) | ❌ |
| Perches (window top edges) | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ | ✅ |
| Hide rules (fullscreen, hotkey) | ✅ | ❌ | ❌ | ✅ (Boss mode) | ❌ | ❌ | ❌ |
| Capture exclusion (no screen share) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Multi-instance (several buddies) | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| Multi-monitor | ✅ | ✅ | ✅ | ✅ (toggle) | ✅ | ✅ | ✅ |

### Character & art

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Character Packages (first-class) | ✅ | ✅ (DLC) | ✅ (Workshop) | ✅ (community) | ✅ (beta, +2 soon) | ✅ (catalog) | ✅ (VRM + Workshop) |
| Art ecosystem / gallery | ✅ (import petdex + Shimeji-ee) | 40+ official DLC | Steam Workshop | 1000s fan-made | 1 shipped, +2 soon | openpets.dev catalog | Steam Workshop + VRM |
| Required Animation Set | 9 animations | ❌ (3D models) | PNG sequences | sprite set | ❌ (procedural) | spritesheet.webp | ❌ (VRM rigged) |
| Declarative Behaviors | ✅ (TOML) | ❌ | ❌ | ❌ (XML graphs) | ❌ | ✅ (plugins) | ❌ |

### Behavior & personality

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| AI-powered behavior | ✅ (Director + personality.txt + spoken lines) | ❌ | ❌ | ❌ (deterministic XML) | ✅ (OpenAI chat window) | ✅ (plugin SDK + MCP say) | ✅ (QWEN 2.5 1.5b) |
| Personality-driven idle AI (unprompted) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Authored personality file | ✅ (personality.txt) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Spoken lines / talk bubble | ✅ (Director with Completer) | ❌ | ❌ | ❌ | ❌ | ✅ (MCP say, plugin-driven) | ❌ |
| Idle life (model-free) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Reacts to user (Poke, Grab) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Window awareness | ✅ (app name, geometry) | ✅ | ❌ | ✅ (edges) | ❌ | ❌ | ✅ |

### Agent integrations

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Harness integrations (ACP Completer) | ✅ (claude/hermes/opencode/grok named+verified; codex named unverified; + custom) | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ |
| MCP server (buddy-side tools) | ✅ (loopback HTTP + stdio fallback) | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ |
| AI chat integration | ✅ (Summon chat surface shipped; #17 tracks polish/bugs) | ❌ | ❌ | ❌ | ✅ (OpenAI) | ✅ (plugin + ctx.ai) | ✅ (built-in LLM) |
| BYO model / API key | ✅ (Settings + env vars) | ❌ | ❌ | ❌ | ✅ (OpenAI) | ✅ (Anthropic/OpenAI/Ollama) | ❌ |

### Memory & privacy

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Memory (shared, user-editable) | ✅ (Markdown file) | ❌ | ❌ | ❌ | ❌ | ✅ (plugin storage) | ❌ |
| Ambient Capture | ~ (deferred v1) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| On-Demand Capture | ~ (deferred v1) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Local-first (no cloud required) | ✅ (Spatial) | ✅ | ✅ | ✅ | ✅ (Spatial) | ✅ | ✅ |
| Consent-per-feature (opt-in gates) | ✅ (Settings UI) | ❌ | ❌ | ❌ | ❌ | ✅ (permissions) | ❌ |
| Denylist (excluded apps) | ~ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### Platforms

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| macOS | ✅ | ✅ (beta, June 2026) | ❌ | ✅ (patched forks) | ✅ (10.15+) | ✅ (arm64/x64) | ~ (PR #551 open) |
| Windows | ✅ (NSIS; some platform cells stub/degraded) | ✅ | ✅ | ✅ | ✅ (10/11) | ✅ (signed) | ✅ (11) |
| Linux | ✅ (X11), ~ (Wayland) | ❌ | ❌ | ✅ (community forks) | ❌ | ✅ (AppImage, Wayland issues) | ❌ official (~unofficial port) |

### Pricing & distribution

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Base app price | free (OSS MIT) | free (Steam F2P) | free (OSS) | free | free (beta) | free (MIT) | free (GitHub) / $5.49 (Steam) |
| Character DLC | ❌ | $7.49–$14.99 each | free (Workshop) + 2 paid DLC | free (community) + ~$8.90 (Shimeji Shop) | free (beta) | free (catalog) | free (VRM + Workshop) |
| Subscription model | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Distribution | GitHub releases | Steam | Steam | web downloads, fan sites | desktoppet.app | GitHub releases | GitHub + Steam |
