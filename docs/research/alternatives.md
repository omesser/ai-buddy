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

## User-language comparison

| Project | Target User | Core use case | Main strength | Main weakness | Evidence quality |
|---|---|---|---|---|---|
| ai-buddy | personality-driven desktop mascot fans; later attach own agent | personality-driven idle AI behavior + physics | personality-driven AI behavior via Director + authored personality.txt, plus Spatial (Perches, throw, hide, capture exclusion); Harness ACP Completer + MCP server + Summon chat shipped (2026-09-07/08) | Windows NSIS ships (some platform cells stub/degraded); eight Character Packages (black-mage, bmo, buddy-bot, cat, jotaro-kujo, nim, timber-wolf, trump); GitHub-only | high for own spec/ship split |
| Desktop Mate | licensed 3D fans (Miku, Sanrio, VTubers) | character catalog on Steam | Steam reach + 40+ licensed DLC | Mixed reviews (61%); DLC/mod revolt; no official Linux | 2M = vendor claim; reviews real |
| VPet | free care-sim + Workshop fans | feed/bathe/Workshop content | 51,678 reviews (98%), Workshop open | Windows-only official; Proton transparency issues | review proof strong |
| Shimeji-ee | classic 2D fan mascots (decades of packs) | my character via folklore (Java, img/) | 1000s free packs + throw/climb prior art | Windows+Java official; forks elsewhere; no agent | Android 500K+; desktop no central count |
| Desktop Pet | productivity + BYO OpenAI (vendor claim) | Pomodoro + AI chat | privacy-first vendor claims; free beta | no independent reviews found; unsigned / Run anyway | low (vendor-only) |
| OpenPets | developers, local agent sidekick | MCP + plugin SDK for coding agents | shipped MCP+SDK; 1,130 stars | Electron; Wayland overlay bugs; MCP is react/say not general harness; gravity ≠ Perch riding | GitHub stars + docs verifiable |
| MateEngine | VRM fans after Desktop Mate mod removal | my VRM on the desktop, free | 3,532 stars + 974 Steam reviews 97%; Workshop + VRM; free on GitHub | no physics; Windows-only official; no official Linux/macOS; AI is local LLM, not BYO | Steam + GitHub strong |

## How others use AI

Desktop pets use AI differently than ai-buddy's personality-driven idle Director:

- **Desktop Mate / VPet / Shimeji-ee**: No generative model for character behavior. MateEngine's comparison table: Desktop Mate AI Chat ❌. Shimeji-ee is deterministic XML behavior graphs. VPet is care-sim + Workshop animations, no personality prompt. Community mods (OpenVPet Active Chat, ShimejiEE-AI) are chat plugins, not idle Directors.

- **Desktop Pet** ([desktoppet.app](https://desktoppet.app/)): BYO OpenAI **chat/voice assistant**. User opens Assistant Mode; wake word "Hey Pet". Vendor "personality traits" = pet-type copy (cats curious, dogs loyal), not an idle Director that picks Behaviors + spoken lines from authored personality. Chat window product wearing a roaming sprite.

- **OpenPets**: Three different AI uses, none is idle Director: (1) **coding agent talks THROUGH the pet** via MCP `openpets_say` / `openpets_react` — agent-initiated, not idle; (2) **plugins use `ctx.ai`** gateway (Anthropic/OpenAI/Ollama keys) for plugin logic, not idle character speech; (3) host **Pet Assistant** chat/Talk loop ([#138](https://github.com/alvinunreal/openpets/issues/138), architecture.md) that injects owner-authored **personality profile as communication preferences** into conversation turns — profile is chat tone/style, not idle Director. Agent reactions via MCP `say` use validated **speech pools** (pre-approved phrases), not generative idle lines. OpenPets has personality (the profile); it's architecture is chat assistant + agent conduit, not idle personality-driven behavior.

- **MateEngine**: Built-in QWEN 2.5 1.5b LLM. Steam page ([3625270](https://store.steampowered.com/app/3625270/MateEngine/)) CHATTING section: "You can chat with your pet anytime! Just note that it's a small, local AI with 