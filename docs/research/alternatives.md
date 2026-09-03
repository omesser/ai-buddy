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
| Personality-driven idle AI (no chat window) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Authored personality file | ✅ (personality.txt) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Spoken lines / talk bubble | ✅ (Director with Completer) | ❌ | ❌ | ❌ | ❌ | ✅ (MCP say, plugin-driven) | ❌ |
| Idle life (model-free) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Reacts to user (Poke, Grab) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Window awareness | ✅ (app name, geometry) | ✅ | ❌ | ✅ (edges) | ❌ | ❌ | ✅ |

### Agent integrations

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Harness integrations (MCP attach) | ~ | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ |
| MCP server (buddy-side tools) | ~ | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ |
| AI chat integration | ~ (#17, #119) | ❌ | ❌ | ❌ | ✅ (OpenAI) | ✅ (plugin + ctx.ai) | ✅ (built-in LLM) |
| BYO model / API key | ✅ (env vars) | ❌ | ❌ | ❌ | ✅ (OpenAI) | ✅ (Anthropic/OpenAI/Ollama) | ❌ |

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
| Windows | ~ (stubbed deliberately) | ✅ | ✅ | ✅ | ✅ (10/11) | ✅ (signed) | ✅ (11) |
| Linux | ✅ (X11), ~ (Wayland) | ❌ | ❌ | ✅ (community forks) | ❌ | ✅ (AppImage, Wayland issues) | ❌ official (~unofficial port) |

### Pricing & distribution

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets | MateEngine |
|---|---|---|---|---|---|---|---|
| Base app price | free (OSS MIT) | free (Steam F2P) | free (OSS) | free | free (beta) | free (MIT) | free (GitHub) / $5.49 (Steam) |
| Character DLC | ❌ | $7.49–$14.99 each | free (Workshop) + 2 paid DLC | free (community) + ~$8.90 (Shimeji Shop) | free (beta) | free (catalog) | free (VRM + Workshop) |
| Subscription model | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Distribution | GitHub releases | Steam | Steam | web downloads, fan sites | desktoppet.app | GitHub releases | GitHub + Steam |

## Buyer-language comparison

| Project | Target customer | Core use case | Main strength | Main weakness | Evidence quality |
|---|---|---|---|---|---|
| ai-buddy | personality-driven desktop mascot fans; later attach own agent | personality-driven idle AI behavior + physics | personality-driven AI behavior via Director + authored personality.txt, plus Spatial (Perches, throw, hide, capture exclusion) | Harness not shipped; Windows stubbed; two characters; GitHub-only | high for own spec/ship split |
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

- **MateEngine**: Built-in QWEN 2.5 1.5b LLM. Steam page ([3625270](https://store.steampowered.com/app/3625270/MateEngine/)) CHATTING section: "You can chat with your pet anytime! Just note that it's a small, local AI with simple messages." README comparison table: AI Chat ✅. Steam-exclusive event-based "cute messages" on drag/dance/sit = interaction-triggered responses, not idle personality Director (unknown if those messages are LLM-generated or canned, vendor does not specify).

**Related projects** (not in the six-alternative comparison): **AI Desktop Pet** ([Steam 4227700](https://store.steampowered.com/app/4227700/AI_Desktop_Pet/)) is a **different product** targeting long-term RP/VN companion use case (wholly out of scope for ai-buddy). It has many capabilities ai-buddy does not: persona cards, world books, VN mode, voice clone, screen vision, idle-started conversations, Workshop cards. Built-in local LLM, GGUF import, plus optional ~30 cloud provider accounts. Phase Pal ([Steam 3655450](https://store.steampowered.com/app/3655450/Phase_Pal/)) AIGC disclosure = "real-time chatbot within a floating interface… guided by customizable prompts"; Pal Engine ([Steam 3868880](https://store.steampowered.com/app/3868880/Pal_Engine/)) = "The AI model is an agentic assistant" with personality+memory for chat, plus separate ambient animation behavior layer. Same pattern: chat assistant wearing a mascot overlay.

**ai-buddy's difference**: Authored `personality.txt` (who they are, fixations, sample lines) drives Director that picks idle Behaviors + spoken lines non-deterministically, in-character. No chat window (#17 Summon is specced). The Character talks while living on your windows, not when you open a chat.

## Per-project notes

### Desktop Mate

**What it is.** Free-to-play Steam app (App ID 3301060, launched Jan 7, 2025)
with 2M+ downloads claim by vendor Infinite Loop (June 2026 PR). 3D character
mascots that sit on windows, react to mouse, include voice lines.
([Steam](https://store.steampowered.com/app/3301060/Desktop_Mate/))

**Verified present.** Overlay with click-through. Character sits on window top
edges (same as ai-buddy Perches). Licensed characters (Hatsune Miku, Hello
Kitty, Sanrio, VTuber personas, Touhou Project). Multi-Character Mode officially
released (display up to two characters simultaneously, with special combo actions
for certain pairs). Built-in alarm feature; some DLC include mascot characters
that appear during alarms. Mac version (Apple Silicon, open beta) launched June
24, 2026. Multi-instance (via third-party methods or native multi-character
feature).

**Absent.** No ballistic throw / gravity integrator found in cited sources (sits
on windows Perch-like without documented physics model). No hide rules verified
(fullscreen, screen sharing). No capture exclusion verified. No agent
integrations, no MCP, no chat. Characters are 3D models purchased as DLC, not
user-authorable 2D sprite packages. Mod support removed Feb 2025; anti-modding
measures in place. No Steam Workshop. No animation set requirement (3D models,
not sprites). **Linux:** ❌ official (Steam `platforms.linux=false`); Proton /
community ports exist ("doesn't work on linux / black desktop" review cluster).

**Review sentiment.** Steam English reviews: Mixed, 61% positive of 5,278 (all
languages: 6,202 positive / 9,262 total as of 2026-09-03). Recurring complaints:
DLC pricing ($7.49–$14.99 per character, 40+ SKUs); mod removal. Verified
quotes: Kiraz!! 2026-08-29: customizability / $15 DLC / no Workshop
(https://steamcommunity.com/id/nyatoi/recommended/3301060/); pyonpyonbun
2025-03-13 attached character stripped after update; GameBrain cluster: "obvious
cash grab," "removed mod support to make you purchase the overpriced DLC,"
"exploitative"; Steam discussion: "I've just uninstalled Desktop Mate because of
this" (594014141938699644); VaporLens sentiment: 22% recommend MateEngine.

**Differences.** Desktop Mate is commercial DLC-driven (40+ licensed character
packs at $7.49–$14.99 each); ai-buddy has two shipped Characters, internal
package format (undocumented until v2), and import adapters for petdex / Pets
Codex and Shimeji-ee ecosystems. Buyer split: official SKUs vs *my* character.
Desktop Mate has no ballistic physics, no agent integrations, no BYO character
creation after mod removal. ai-buddy's Spatial Layer includes ballistic physics
and Perch riding/dropping under a gate, and its planned agent integrations (MCP
+ Harness attach) target capabilities Desktop Mate does not attempt.

### VPet-Simulator

**What it is.** Free and open source desktop pet on Steam (App ID 1920960,
launched Aug 13, 2023). 51,678 reviews, 98% positive, Overwhelmingly Positive
(August 2026). Steam Charts all-time peak 85,283 players. Windows only (official
Steam platforms: `mac: false`, `linux: false`). Built to promote VUP Simulator;
the desktop pet is extracted from that program.
([Steam](https://store.steampowered.com/app/1920960/VPetSimulator/);
[GitHub](https://github.com/LorisYounger/VPet))

**Verified present.** Overlay with click-through. Multi-instance (spawn multiple
pets). Extensive Steam Workshop support for community animations, interactions,
skins (stated in Steam description and community discussions). Two paid DLC
(ModMaker, Pancake Cat Skin package) plus free Workshop content. No purchase
price for base app. Cross-platform claim: Windows only official; GitHub
https://github.com/LorisYounger/VPet is WPF Windows. Open source on GitHub.
Animation assets require specific PNG sequence-frame structure:
`{status}/{type}/{name}_{action}_{time}.png`.

**Absent.** No ballistic physics (gravity, throw). No window top-edge Perches
(sprite appears to rest on desktop floor, not on windows). No agent
integrations, MCP, chat, or Harness. No Memory system. No Ambient Capture. No
evidence of screen-sharing exclusion. Hide rules not verified (no fullscreen
auto-hide or hotkey hide mentioned in Steam page or README). **macOS:** ❌
official Steam. **Linux:** ❌ official; Proton users report non-transparent /
black background (ValveSoftware/Proton#8471).

**Onboarding reality.** "starts in Chinese" (review language); off-screen after
DPI/monitor changes (issue #546).

**Differences.** VPet's Workshop ecosystem is live and massive (98% of 51,678
reviews), while ai-buddy's Character Package format is internal and undocumented
until v2. VPet has no window awareness or Perches, no ballistic physics, and no
agent integrations or MCP layer. ai-buddy ships Director + authored
personality-driven idle speech (VPet has no personality file or non-deterministic
speech; its behavior is Workshop-defined animations). Harness is specced (~), not
shipped. Buyer job: care-sim + Workshop-open vs ai-buddy's personality-driven
idle life + physics.

### Shimeji-ee ecosystem

**What it is.** Windows-first Java desktop mascot, originally by Yuki Yamada /
Group Finity (2009, zlib/libpng), forked and maintained as Shimeji-ee by
Kilkakon and others (New BSD). Distributed via
[kilkakon.com](https://kilkakon.com/shimeji/),
[SourceForge](https://sourceforge.net/app/shimeji-ee/), and fan sites. Android
port has 500K+ downloads. Character packs are community-created and shared on
DeviantArt, Tumblr, dedicated archives.

**Verified present.** Overlay (always-on-top sprite). Click-through. Window
edge awareness (sprites sit on window tops, similar to ai-buddy Perches).
Multi-instance (many Shimeji at once; Image Set Chooser lets users select which
character types to spawn). Multi-monitor support with toggle ("Move Between
Screens" setting). Character packages via community (1000s of free fan-made
image sets). Idle life (wander, fall, climb). Reacts to user (throw, interact).
"Boss mode" in DalekCraft2's fork (double middle-click tray icon to quickly hide
all Shimeji). Standard sprite set required (shime1.png - shime46.png). **Physics
(kind matters).** Required Fall/Thrown actions; gravity integrator in the Java
lineage (sprite kinematics + throw/climb; see DalekCraft2 Fall.java, kilkakon
required actions). Not the same as ai-buddy's ballistic integrator with
window-top Perches and acceleration gate.

**Absent.** No ballistic Perch riding model (ai-buddy's gravity/throw arcs with
Perch acceleration gate). No capture exclusion. No hide rules in base version
(no auto-hide on fullscreen); Boss mode is manual toggle. No agent integrations,
MCP, or chat. No Memory. Behavior system is XML graphs, not TOML declarative
Behaviors.

**Onboarding reality.** Folklore (Java, img/ folders, not in Downloads).

**Differences.** Shimeji-ee's community character ecosystem (1000s of free
packs) offers direct distribution; ai-buddy imports from Shimeji-ee via
`scripts/import-pet.py` adapter plus petdex / Pets Codex, translating into
Character Packages once (authoring-time, not live bridge). Shimeji-ee has
Fall/gravity (sprite kinematics) but not ai-buddy's ballistic Perch model; no
window awareness beyond edges; no agent integrations or AI capabilities.
ai-buddy adds ballistic physics (gravity arcs, throw, Perches under acceleration
gate), window app name tracking, planned MCP + Harness attach, Memory, and BYO
model. Shimeji-ee's XML graph behavior system versus ai-buddy's Director +
declarative Behaviors is a design difference in how liveliness is authored.

### Desktop Pet

**What it is.** Free beta desktop companion by independent developer, available
at desktoppet.app. Windows 10/11 (~150MB zip, v1.1.1, Oct 19, 2025) and macOS
10.15+ (~191MB dmg). Homepage (as of 2026-09-03): "0 Downloads / Free (Beta) / 1
Pet Type +2 Coming Soon."
([desktoppet.app](https://desktoppet.app/))

**Verified present (vendor claims only).** Overlay (desktop floater).
Click/drag to move pet, right-click menu, double-click interact, Tab key to
place. Idle life (pet roams screen). Reacts to user (responds to clicks, drag).
Pomodoro focus timer. Break and hydration reminders. Sound effects. AI Assistant
Mode (voice or text chat) with user's own OpenAI API key; wake word default "Hey
Pet"; key stored locally. Voice commands: wake word detection, timer, reminders,
weather. Privacy claims: data on device, no collection; conversations not saved
permanently. Multi-monitor inferred (desktop app). Windows 10/11 and macOS 10.15+
(Catalina) supported. Free beta. DirectX 11 / Metal graphics. Unsigned builds
(vendor copy: "More info" → "Run anyway"). Character packages (1 type shipped,
+2 coming soon per homepage).

**Absent.** No ballistic physics (gravity, throw). No Perches (window top
edges). No hide rules (fullscreen auto-hide). No capture exclusion. No
multi-instance verified (homepage shows singular "pet"). No agent runtime beyond
OpenAI chat (no MCP, no Harness). No MCP server. No Memory system
(conversations not saved permanently per privacy policy). No Ambient or
On-Demand Capture. No Linux version. No animation set requirement
(procedural/model-driven, not sprite sequences). No declarative Behaviors or
Director.

**Evidence quality.** Low. No independent reviews found (no Steam page, no
GitHub community, no public customer voice). Vendor-only homepage and feature
list.

**Differences.** Desktop Pet targets productivity (Pomodoro, reminders) with AI
chat via user's OpenAI key, while ai-buddy ships Director + authored
personality-driven idle speech (Character talks in-character while living on
windows, not a chat window). Desktop Pet is beta with limited character selection
(1 shipped, +2 soon) and no public validation; ai-buddy has two shipped
Characters, internal package format, and import adapters for petdex / Pets Codex
and Shimeji-ee. Desktop Pet has no ballistic physics, no Perches, no window
awareness. Desktop Pet's AI is an OpenAI chat window; ai-buddy's Director drives
idle speech from personality. Agent integrations (Harness/MCP) are specced (~)
for ai-buddy, not shipped.

### OpenPets

**What it is.** Open source (MIT) desktop companion platform by Boring Dystopia
Development, launched May 2026. 1,130 GitHub stars as of September 3, 2026.
Electron app: macOS arm64/x64 dmg, Windows signed exe, Linux AppImage. Animated
pets idle/wander/react out of the box; no agent required. Plugin SDK v3 for
extending functionality.
([GitHub](https://github.com/alvinunreal/openpets);
[openpets.dev](https://openpets.dev/))

**Verified present.** Overlay (transparent, always-on-top pet windows).
Click-through (per-pixel transparency). Multi-instance (multiple pet windows;
agent pets routed by lease). Multi-monitor (display geometry helpers, pets roam
across displays). Character packages via catalog (openpets.dev serves versioned
pet catalog; pets are ZIP downloads with spritesheet.webp + manifest). Official
plugins: Day Routine, Focus Buddy (Pomodoro), Fortune Cookie, Launch Buddy,
Magic 8 Ball, Mood Check-in, Reminders, Virtual Pet (Tamagotchi-style stats),
Water Reminder. Plugin SDK v3: sandboxed JS/TS runtime, permissions model
(explicit consent for sensitive APIs), schedules, storage, commands, panels,
audio, notifications, ctx.ai (Anthropic/OpenAI/Ollama with user-configured
keys). MCP server (stdio, tools: `openpets_status`, `openpets_react`,
`openpets_say`). Agent integrations: Claude Code, OpenCode, Cursor, Pi. Speech
sanitized (redacts paths, secrets, code). Local-first (no accounts, no cloud
required). BYO model via plugin SDK ctx.ai gateway. Consent-per-feature
(permissions declared in manifest, approved at install, flagged sensitive APIs
require explicit toggles). Memory via plugin storage (ctx.storage JSON key-value
with change subscriptions). Idle life (pets animate continuously). Reacts to
user (click, drag). Spritesheet.webp animation format. Declarative via plugins
(SDK v3 allows defining pet behaviors). **Physics (kind matters).** Gravity
overlay + Walkabout roam (motion-engine in desktop.md / docs/desktop.md); not
window-edge Perch riding.

**Absent.** No ballistic Perch riding model (ai-buddy's throw arcs + window-top
Perches with acceleration gate). No hide rules (fullscreen auto-hide, hotkey).
No capture exclusion verified. No window awareness (pets don't track app names
or geometry). No Ambient or On-Demand Capture. No Denylist (excluded apps).

**Linux reality.** AppImage available; Wayland overlay issues reported (focus
steal #32, invisible pet / tray-only #108/#141).

**Onboarding reality.** Unsigned macOS builds may trigger quarantine warning
(vendor docs: `xattr -dr com.apple.quarantine /Applications/OpenPets.app`).

**Differences.** OpenPets is the closest shipped agent-pet alternative to
ai-buddy on architecture: overlay pet + plugin/agent extensibility + MCP +
local-first + BYO model. OpenPets ships MCP (`openpets_status`, `openpets_react`,
`openpets_say`) and plugin SDK v3 today; ai-buddy's Harness + MCP is specced.
OpenPets has mature catalog (openpets.dev) and 9 official plugins; ai-buddy has
two Characters and no plugin system. ai-buddy adds ballistic physics (gravity
arcs, throw, Perches with acceleration gate) and window awareness; OpenPets has
gravity overlay but no Perch riding or window app name tracking. OpenPets plugin
runtime is sandboxed Electron BrowserWindows with permissions; ai-buddy's
Harness model is external (user attaches their own MCP-compatible agent). Both
MIT licensed, local-first, no accounts.

### MateEngine

**What it is.** Free and open source desktop companion (GitHub:
shinyflvre/Mate-Engine, 3,532 stars as of Sep 3 2026), also on Steam (App ID
3625270, launched April 16, 2025, $5.49). Positioned as free Desktop Mate
alternative after DM charged $10–$25 per model and disabled mods. 974 Steam
reviews, 97% positive, Overwhelmingly Positive (Steambase July 2026).
([Steam](https://store.steampowered.com/app/3625270/MateEngine/);
[GitHub](https://github.com/shinyflvre/Mate-Engine))

**Verified present.** Overlay (always-on-top). Click-through. Window sitting
(sits on window top edges, similar to ai-buddy Perches). Taskbar sitting. Idle
animations. Drag animations. Dance to music. Custom VRM avatar support (user's
own 3D VRM models). Steam Workshop support for mods, custom models, dances. Mod
support (.ME file format). Multi-instance (inferred from VRM support +
Workshop). Multi-monitor (inferred from desktop overlay). Always-on-top toggle.
AI integration: QWEN 2.5 1.5b LLM (Apache License, built-in). Timer, alarm
features. Screensaver mode. Touch regions. Avatar SFX. Particle effects. FPS
control. Head tracking, spine tracking, eye tracking, hand movement. Custom
shaders. Free on GitHub, $5.49 on Steam. Windows 11 official.

**Absent.** No ballistic physics (gravity, throw) found in README comparison
table. No hide rules (fullscreen auto-hide, hotkey, Boss mode). No capture
exclusion. No agent integrations beyond built-in AI (no MCP, no Harness attach,
no BYO model key — AI is local QWEN). No Memory system. No BYO OpenAI/Anthropic
key (uses local LLM). No required animation set (VRM rigged models, not sprite
sequences). **macOS:** ~ (PR #551 open as of May 2026, experimental). **Linux:**
❌ official (issue #85 closed wontfix, "does not make sense" due to .NET/audio
library incompatibilities); unofficial Linux port exists
([Marksonthegamer/Mate-Engine-Linux-Port](https://github.com/Marksonthegamer/Mate-Engine-Linux-Port),
269 stars, X11-only, Wayland transparency issues, window snapping/dock sitting
don't work on XWayland).

**Buyer language.** MateEngine README: "Desktop Mate charges $10–$25 USD for
single character models... modding and custom models were disabled in later
versions." VaporLens Desktop Mate sentiment: 22% recommend MateEngine; "fair and
reasonable" pricing vs Desktop Mate's "high DLC prices." Steam reviews: "just
buy mate engine" in Desktop Mate negative cluster.

**Onboarding reality.** Windows 11 only official. macOS experimental (PR open).
Linux unofficial port requires X11, has Wayland issues.

**Differences.** MateEngine is the user-owned VRM/Workshop answer to Desktop
Mate's SKU lock. ai-buddy ships authored personality + Director-driven idle
speech (Character talks from personality while living on windows); MateEngine has
built-in local LLM (QWEN 2.5 1.5b) but no authored personality file or
Director-driven idle speech. Both sit on windows (Perches), but MateEngine is
VRM-driven (user's 3D rigged models) and ai-buddy is 2D sprite + ballistic
physics. MateEngine has Steam Workshop + mods (the capability Desktop Mate
removed); ai-buddy has import adapters for petdex + Shimeji-ee (authoring-time,
not live Workshop). MateEngine has no ballistic physics (no gravity/throw arcs),
no hide rules, no capture exclusion, no MCP, no agent Harness. ai-buddy's Spatial
Layer ships ballistic Perch riding + acceleration gate + hide rules + capture
exclusion; ai-buddy's Functional Layer (MCP + Harness attach) is specced (~), not
shipped. Buyer job split: MateEngine is *my VRM* after Desktop Mate's mod
removal; ai-buddy is authored personality + idle speech + physics.

## Physics note (kind matters)

ai-buddy ships a ballistic integrator: gravity, throw arcs, and window-top
Perches that the sprite rides until an acceleration gate drops it. That is not
the same as (a) Shimeji-ee's required Fall/Thrown and gravity in Fall.java
(sprite kinematics + throw/climb), or (b) OpenPets' gravity overlay / Walkabout
roam (motion-engine). Desktop Mate and MateEngine sit on windows (Perch-like)
without a documented ballistic throw-physics model. Do not mark Shimeji-ee or
OpenPets as "no physics" — mark them ~ (partial) because they have gravity but
not ai-buddy's ballistic Perch gate. Desktop Mate / VPet / Desktop Pet /
MateEngine: ❌ (no ballistic throw / no gravity integrator found in cited
sources).

## Unique-combo reality check

**What ai-buddy ships differently:**

The mascot has an authored personality file (`personality.txt`) and a Director
that picks Behaviors and spoken lines non-deterministically. The Character talks
in-character while living on your windows — not Clippy (no claiming machine
abilities, no promising actions), not a chat app (no chat window, no
back-and-forth; #17 Summon is specced). Director proposes a Behavior name and
optional spoken line; Static weights when no Completer is configured, HTTP
Completer stand-in with API key/local server, Harness will replace the stand-in
(ADR-0008, specced). Each Instance has its own Director and seed — two of the
same Character don't move or speak in lockstep.

1. **Personality-driven AI behavior (shipped).** Authored `personality.txt` (who
   they are, fixations, sample lines; loader never interprets it) drives Director
   that picks idle Behaviors + spoken lines non-deterministically, in-character.
   Completer contract: Behavior name on one line, optional spoken line on the
   next. `talk` animation plays when a proposal includes it. Universal rules
   (stay in character, bubble length, no claiming machine abilities) injected in
   `character_prompt`, not in the file. Unparsable reply becomes speech; failed
   wake falls back to Static. Engine keeps the sprite alive while the model
   thinks. Static weights when no Completer configured; HTTP Completer stand-in
   with API key/local server; Harness will be another Completer that feeds the
   same Director loop (ADR-0008, specced). No other desktop pet ships authored
   personality + Director-driven non-deterministic idle speech.

2. **Spatial differentiators (shipped).** Ballistic physics (gravity arcs, throw,
   Perch acceleration gate). Capture exclusion (no screen share). Fullscreen
   fade + hotkey hide. Window app name tracking. Local idle life without model.

3. **Agent integrations (specced, not shipped).** MCP + BYO Harness attach.
   README/ADR-0008: HTTP Completer is a stand-in; Harness *will* replace it; no
   chat surface yet (#17). OpenPets *already ships* overlay pet + MCP
   (`openpets_status` / `openpets_react` / `openpets_say`) + plugin SDK. Closest
   *shipped* agent-pet is OpenPets; ai-buddy's shipped differentiators are
   personality-driven Director speech + Spatial (capture exclusion, fullscreen
   fade, hotkey hide, Perch acceleration-gate).

**What other projects have that ai-buddy doesn't (yet):**

1. **Character ecosystems (who controls the pack).** Desktop Mate: official SKUs
   vs *my* character; Mixed reviews + DLC/mod removal through 2026. VPet
   Workshop-open is the actual ecosystem strength (98% of 51,678 reviews).
   MateEngine: VRM + Workshop-open, free, the switching target after Desktop
   Mate disabled mods (3,532 stars + 974 Steam reviews 97%). Shimeji-ee: 1000s
   free packs, community folklore. OpenPets: openpets.dev catalog. ai-buddy: two
   shipped characters + `scripts/import-pet.py` (petdex + Shimeji-ee) =
   authoring-time import, not a live gallery or first-party store.

2. **Agent integrations shipped.** Desktop Pet has OpenAI chat (vendor-only
   evidence). OpenPets has MCP + plugin SDK v3 + 9 official plugins (1,130 stars,
   verifiable). MateEngine has built-in AI (QWEN 2.5 1.5b LLM), not BYO agent
   attach. ai-buddy's MCP + Harness is specced, not built.

3. **Distribution reach.** Desktop Mate, VPet, and MateEngine are on Steam;
   OpenPets has signed Windows builds and catalog. ai-buddy is GitHub releases
   with no store presence.

## Evidence footer

- **✅** = present in running app / cited source (Steam page, GitHub README,
  review/issue citation, vendor homepage).
- **~** = documented not shipped (ai-buddy MCP/Harness per ADR-0008; MateEngine
  macOS PR #551 open) OR partial (Shimeji-ee/OpenPets physics kind: gravity but
  not ballistic Perch riding).
- **❌** = not found in cited sources as of 2026-09-03.
- Alternative columns are vendor claims unless a review/issue/Steam page is
  cited. Desktop Pet has vendor-only evidence (no independent reviews). MateEngine
  has Steam 974 reviews 97% + GitHub 3,532 stars.

## Sources

Capabilities marked ✅, ~, or ❌ for ai-buddy are verified against docs/SPEC.md,
DESIGN.md, README.md, ADR-0008, and `git log` on main as of September 3, 2026.
Similar projects verified against Steam pages (Desktop Mate [App ID
3301060](https://store.steampowered.com/app/3301060/Desktop_Mate/) English
reviews Mixed 61% of 5,278; VPet-Simulator [App ID
1920960](https://store.steampowered.com/app/1920960/VPetSimulator/) 51,678
reviews 98% positive, SteamPulse metadata platforms `mac: false`, `linux: false`;
MateEngine [App ID 3625270](https://store.steampowered.com/app/3625270/MateEngine/)
974 reviews 97% positive Steambase July 2026), official sites
([Kilkakon](https://kilkakon.com/shimeji/) for Shimeji-ee,
[desktoppet.app](https://desktoppet.app/) for Desktop Pet), GitHub repositories
([alvinunreal/openpets](https://github.com/alvinunreal/openpets) 1,130 stars as
of Sep 3 2026, [LorisYounger/VPet](https://github.com/LorisYounger/VPet),
[DalekCraft2/Shimeji-Desktop](https://github.com/DalekCraft2/Shimeji-Desktop),
[shinyflvre/Mate-Engine](https://github.com/shinyflvre/Mate-Engine) 3,532 stars
as of Sep 3 2026), OpenPets documentation
([docs/architecture.md](https://github.com/alvinunreal/openpets/blob/main/docs/architecture.md),
[docs/desktop.md](https://github.com/alvinunreal/openpets/blob/main/docs/desktop.md)),
MateEngine unofficial Linux port
([Marksonthegamer/Mate-Engine-Linux-Port](https://github.com/Marksonthegamer/Mate-Engine-Linux-Port)
269 stars, issue #85 wontfix), Steam review sentiment (GameBrain, VaporLens
analysis), and third-party coverage. No fabricated features.
