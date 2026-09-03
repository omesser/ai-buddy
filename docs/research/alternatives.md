# ai-buddy versus five desktop pet alternatives — feature comparison

Comparison against five widely used desktop pet projects (see
[market.md](./market.md)): Desktop Mate, VPet-Simulator, Shimeji-ee ecosystem,
Desktop Pet (desktoppet.app), OpenPets. Matrix rows are capabilities that matter
for this product, using CONTEXT.md vocabulary. ai-buddy column is honest about
what is and is not built.

**Legend:** ✓ present, ~ documented not shipped, ❌ absent

## Feature matrix

### Spatial capabilities

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| Overlay (always-on-top) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Click-through (per-pixel) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Physics (gravity, throw) | ✓ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Perches (window top edges) | ✓ | ✓ | ❌ | ✓ | ❌ | ❌ |
| Hide rules (fullscreen, hotkey) | ✓ | ❌ | ❌ | ✓ (Boss mode) | ❌ | ❌ |
| Capture exclusion (no screen share) | ✓ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Multi-instance (several buddies) | ✓ | ✓ | ✓ | ✓ | ❌ | ✓ |
| Multi-monitor | ✓ | ✓ | ✓ | ✓ (toggle) | ✓ | ✓ |

### Character & art

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| Character Packages (first-class) | ✓ | ✓ (DLC) | ✓ (Workshop) | ✓ (community) | ✓ (beta, +2 soon) | ✓ (catalog) |
| Art ecosystem / gallery | ✓ (import petdex + Shimeji-ee) | 40+ official DLC | Steam Workshop | 1000s fan-made | 1 shipped, +2 soon | openpets.dev catalog |
| Required Animation Set | 9 animations | ❌ (3D models) | PNG sequences | sprite set | ❌ (procedural) | spritesheet.webp |
| Declarative Behaviors | ✓ (TOML) | ❌ | ❌ | ❌ (XML graphs) | ❌ | ✓ (plugins) |

### Behavior & personality

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| AI-powered behavior | ✓ (Director + personality.txt) | ❌ | ❌ | ❌ (deterministic XML) | ✓ (OpenAI chat) | ✓ (plugin SDK + MCP) |
| Idle life (model-free) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Reacts to user (Poke, Grab) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Window awareness | ✓ (app name, geometry) | ✓ | ❌ | ✓ (edges) | ❌ | ❌ |

### Agent integrations

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| Harness integrations (MCP attach) | ~ | ❌ | ❌ | ❌ | ❌ | ✓ |
| MCP server (buddy-side tools) | ~ | ❌ | ❌ | ❌ | ❌ | ✓ |
| AI chat integration | ~ (#17, #119) | ❌ | ❌ | ❌ | ✓ (OpenAI) | ✓ (plugin + ctx.ai) |
| BYO model / API key | ✓ (env vars) | ❌ | ❌ | ❌ | ✓ (OpenAI) | ✓ (Anthropic/OpenAI/Ollama) |

### Memory & privacy

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| Memory (shared, user-editable) | ✓ (Markdown file) | ❌ | ❌ | ❌ | ❌ | ✓ (plugin storage) |
| Ambient Capture | ~ (deferred v1) | ❌ | ❌ | ❌ | ❌ | ❌ |
| On-Demand Capture | ~ (deferred v1) | ❌ | ❌ | ❌ | ❌ | ❌ |
| Local-first (no cloud required) | ✓ (Spatial) | ✓ | ✓ | ✓ | ✓ (Spatial) | ✓ |
| Consent-per-feature (opt-in gates) | ✓ (Settings UI) | ❌ | ❌ | ❌ | ❌ | ✓ (permissions) |
| Denylist (excluded apps) | ~ | ❌ | ❌ | ❌ | ❌ | ❌ |

### Platforms

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| macOS | ✓ | ✓ (beta, June 2026) | ✓ | ✓ (patched forks) | ✓ (10.15+) | ✓ (arm64/x64) |
| Windows | ~ (stubbed deliberately) | ✓ | ✓ | ✓ | ✓ (10/11) | ✓ (signed) |
| Linux | ✓ (X11), ~ (Wayland) | ✓ | ✓ | ✓ (community forks) | ❌ | ✓ (AppImage) |

### Pricing & distribution

| Capability | ai-buddy | Desktop Mate | VPet | Shimeji-ee | Desktop Pet | OpenPets |
|---|---|---|---|---|---|---|
| Base app price | free (OSS MIT) | free (Steam F2P) | free (OSS) | free | free (beta) | free (MIT) |
| Character DLC | ❌ | $7.49–$14.99 each | free (Workshop) + 2 paid DLC | free (community) + ~$8.90 (Shimeji Shop) | free (beta) | free (catalog) |
| Subscription model | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Distribution | GitHub releases | Steam | Steam | web downloads, fan sites | desktoppet.app | GitHub releases |

## Per-project notes

### Desktop Mate

**What it is.** Free-to-play Steam app (App ID 3301060, launched Jan 7, 2025)
with 2M+ downloads by June 2026. 3D character mascots that sit on window edges,
react to mouse, include voice lines. Developed by Infinite Loop (Sapporo).
([Steam](https://store.steampowered.com/app/3301060/Desktop_Mate/))

**Verified present.** Overlay with click-through
([Steam page](https://store.steampowered.com/app/3301060/Desktop_Mate/):
"sitting on windows, jumping between them, and playfully interacting with your
mouse"). Character sits on window top edges (same as ai-buddy Perches). Licensed
characters (Hatsune Miku, Hello Kitty, Sanrio, VTuber personas, Touhou
Project). Multi-Character Mode officially released (display up to two characters
simultaneously, with special combo actions for certain pairs)
([Steam Community](https://steamcommunity.com/app/3301060/allnews/)).
Multi-instance (via third-party methods or native multi-character feature).
Built-in alarm feature; some DLC include mascot characters that appear during
alarms
([SNOW MIKU 2026 Ver. DLC](https://store.steampowered.com/app/4018720/Desktop_Mate_SNOW_MIKU_2026_Ver_DLC/)).
Mac version (Apple Silicon, open beta) launched June 24, 2026
([HolidayTravel](https://www.haveagood-holiday.com/en/articles/desktop-mate-2-million-downloads-mac-beta-steam-sale)).
Linux support verified (Steam lists Linux).

**Absent.** No physics (gravity, throw). No hide rules verified (fullscreen,
screen sharing). No capture exclusion verified. No agent capabilities, no MCP,
no chat. Characters are 3D models purchased as DLC, not user-authorable 2D
sprite packages. Mod support was removed before or during Steam launch, which
"upset the community" and is "widely criticized as an expensive, aggressive cash
grab"
([GameBrain](https://gamebrain.co/game/desktop-mate): user reviews call it
"obvious cash grab," "removed mod support to make you purchase the overpriced
DLC," "exploitative"). No evidence of character behavior authoring (director,
personality prompts, declarative Behaviors). No animation set requirement (3D
models, not sprites).

**Differences.** Desktop Mate is commercial DLC-driven (40+ licensed character
packs at $7.49–$14.99 each); ai-buddy has two shipped Characters, an internal
package format (undocumented until v2), and import adapters for petdex / Pets
Codex and Shimeji-ee ecosystems. Desktop Mate has no physics, no agent
integrations, and no BYO character creation. ai-buddy's Spatial Layer includes
physics and Perch riding/dropping under a gate, and its planned agent
integrations (MCP + Harness attach) target capabilities Desktop Mate does not
attempt.

### VPet-Simulator

**What it is.** Free and open source desktop pet on Steam (App ID 1920960,
launched Aug 13, 2023). 50,795 reviews, 98% positive. 6,900 current players,
22,071 tracked peak (August 2026). Windows, Mac, Linux, Steam Deck. Built to
promote VUP Simulator; the desktop pet is extracted from that program.
([Steam](https://store.steampowered.com/app/1920960/VPetSimulator/);
[GitHub](https://github.com/LorisYounger/VPet))

**Verified present.** Overlay with click-through. Multi-instance (spawn multiple
pets). Extensive Steam Workshop support for community animations, interactions,
skins (stated in Steam description and community discussions). Two paid DLC
(ModMaker, Pancake Cat Skin package) plus free Workshop content. No purchase
price for base app. Cross-platform (Windows, Mac, Linux, Steam Deck). Open
source on GitHub ([LorisYounger/VPet](https://github.com/LorisYounger/VPet)).
Animation assets require specific PNG sequence-frame structure:
`{status}/{type}/{name}_{action}_{time}.png` where status is happy/nomal/poorcondition/ill,
action is a (start), b (loop), c (end), and time is frame duration in
milliseconds (100-250ms typical). Each mod requires `info.lps` configuration
file.

**Absent.** No physics (gravity, throw). No window top-edge Perches (sprite
appears to rest on desktop floor, not on windows). No agent capabilities, MCP,
chat, or Harness. No Memory system. No Ambient Capture. No evidence of
screen-sharing exclusion. Hide rules not verified (no fullscreen auto-hide or
hotkey hide mentioned in Steam page or README).

**Differences.** VPet's Workshop ecosystem is live and massive (open source +
Steam Workshop), while ai-buddy's Character Package format is internal and
undocumented until v2. VPet has no window awareness or Perches, no physics, and
no agent integrations or MCP layer. ai-buddy's Director + Harness attach targets
capabilities VPet does not have.

### Shimeji-ee ecosystem

**What it is.** Windows-first Java desktop mascot, originally by Yuki Yamada /
Group Finity (2009, zlib/libpng), forked and maintained as Shimeji-ee by
Kilkakon and others (New BSD). Distributed via
[kilkakon.com](https://kilkakon.com/shimeji/),
[SourceForge](https://sourceforge.net/app/shimeji-ee/), and fan sites. Android
port has 500K+ downloads. Character packs are community-created and shared on
DeviantArt, Tumblr, dedicated archives.
([Kilkakon](https://kilkakon.com/shimeji/))

**Verified present.** Overlay (always-on-top sprite). Click-through. Window
edge awareness (sprites sit on window tops, similar to ai-buddy Perches).
Multi-instance (many Shimeji at once; Image Set Chooser lets users select which
character types to spawn). Multi-monitor support with toggle ("Move Between
Screens" setting to prevent Shimeji changing screens unexpectedly)
([DalekCraft2/Shimeji-Desktop](https://github.com/DalekCraft2/Shimeji-Desktop)).
Character packages via community (1000s of free fan-made image sets). Idle life
(wander, fall, climb). Reacts to user (throw, interact). "Boss mode" in
DalekCraft2's fork (double middle-click tray icon to quickly hide all Shimeji)
([kilkakon.txt](https://github.com/DalekCraft2/Shimeji-Desktop/blob/main/kilkakon.txt)).
Standard sprite set required (shime1.png - shime46.png).

**Absent.** No physics (ai-buddy's gravity/ballistic throw model). No capture
exclusion. No hide rules in base version (no auto-hide on fullscreen); Boss mode
is manual toggle. No agent capabilities, MCP, or chat. No Memory. No agent
integrations. Behavior system is XML graphs, not TOML declarative Behaviors.

**Differences.** Shimeji-ee's community character ecosystem (1000s of free
packs) offers direct distribution; ai-buddy imports from Shimeji-ee via
`scripts/import-pet.py` adapter plus petdex / Pets Codex, translating into
Character Packages once (authoring-time, not live bridge). Shimeji-ee has no
window awareness beyond edges, no physics, no agent integrations, and no AI
capabilities. ai-buddy adds physics (gravity, throw, Perches under acceleration
gate), window app name tracking, planned MCP + Harness attach, Memory, and BYO
model. Shimeji-ee's XML graph behavior system versus ai-buddy's Director +
declarative Behaviors is a design difference in how liveliness is authored.

### Desktop Pet

**What it is.** Free beta desktop companion by independent developer, available
at desktoppet.app. Windows 10/11 (~150MB zip, v1.1.1, Oct 19, 2025) and macOS
10.15+ (~191MB dmg). Cats/dogs/bunnies with unique animations and personality; 1
type shipped, +2 coming soon. Software overlay that roams the screen.
([desktoppet.app](https://desktoppet.app/))

**Verified present.** Overlay (desktop floater). Click/drag to move pet,
right-click menu, double-click interact, Tab key to place. Idle life (pet roams
screen). Reacts to user (responds to clicks, drag). Pomodoro focus timer.
Break and hydration reminders. Sound effects. AI Assistant Mode (voice or text
chat) with user's own OpenAI API key; wake word default "Hey Pet"; key stored
locally. Voice commands: wake word detection, timer, reminders, weather. Privacy
claims: data on device, no collection; conversations not saved permanently.
Multi-monitor inferred (desktop app). Windows 10/11 and macOS 10.15+ (Catalina)
supported. Free beta. DirectX 11 / Metal graphics. Unsigned builds. Character
packages (1 type shipped, +2 coming soon per homepage).

**Absent.** No physics (gravity, throw). No Perches (window top edges). No hide
rules (fullscreen auto-hide). No capture exclusion. No multi-instance verified
(homepage shows singular "pet"). No agent runtime beyond OpenAI chat (no MCP, no
Harness). No MCP server. No Executor. No Memory system (conversations not saved
permanently per privacy policy). No Ambient or On-Demand Capture. No Linux
version. No animation set requirement (procedural/model-driven, not sprite
sequences). No declarative Behaviors or Director.

**Differences.** Desktop Pet targets productivity (Pomodoro, reminders) with AI
chat via user's OpenAI key, while ai-buddy separates Spatial (local, no model)
from Functional (BYO Harness). Desktop Pet is beta with limited character
selection (1 shipped, +2 soon); ai-buddy has two shipped Characters, internal
package format, and import adapters for petdex / Pets Codex and Shimeji-ee.
Desktop Pet has no physics, no Perches, no window awareness. ai-buddy's MCP +
Harness model targets general agent capabilities; Desktop Pet's OpenAI
integration is chat-only.

### OpenPets

**What it is.** Open source (MIT) desktop companion platform by Boring Dystopia
Development, launched May 2026. 782 GitHub stars as of Sep 3, 2026. Electron
app: macOS arm64/x64 dmg, Windows signed exe, Linux AppImage. Animated pets
idle/wander/react out of the box; no agent required. Plugin SDK v3 for extending
functionality.
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
(SDK v3 allows defining pet behaviors).

**Absent.** No physics (gravity, throw). No Perches (window top edges). No hide
rules (fullscreen auto-hide, hotkey). No capture exclusion verified. No window
awareness (pets don't track app names or geometry). No Executor (synthetic
input). Chat surface exists via plugin but not core. No Ambient or On-Demand
Capture. No Denylist (excluded apps).

**Differences.** OpenPets is the closest software alternative to ai-buddy on
architecture: overlay pet + plugin/agent extensibility + MCP + local-first + BYO
model. Both separate spatial presence from functional capabilities. OpenPets
ships MCP and plugin SDK v3 today; ai-buddy's Harness + MCP is specced. OpenPets
has mature catalog (openpets.dev) and 9 official plugins; ai-buddy has two
Characters and no plugin system. ai-buddy adds physics (gravity, throw, Perches)
and window awareness; OpenPets has neither. OpenPets plugin runtime is
sandboxed Electron BrowserWindows with permissions; ai-buddy's Harness model is
external (user attaches their own MCP-compatible agent). OpenPets ctx.ai is
provider gateway; ai-buddy env vars are direct keys. Both MIT licensed,
local-first, no accounts.

## Differences from similar projects

What ai-buddy does differently (as specced):

1. **Spatial capabilities + agent integrations together.** Desktop pets (Desktop
   Mate, VPet, Shimeji-ee) have presence and idle life; Desktop Pet and OpenPets
   add AI chat or plugins. None combine spatial physics with window awareness and
   a BYO general-purpose agent runtime. ai-buddy's planned MCP + Harness attach
   lets users bring their own agent.
2. **Physics.** Gravity + ballistic throw + Perch riding under an acceleration
   gate. No other desktop pet has physics.
3. **BYO Harness.** MCP server + user-attached agent runtime. Desktop pets have
   no agent layer (Desktop Mate, VPet, Shimeji-ee), are chat-specific (Desktop
   Pet with OpenAI), or use a plugin SDK (OpenPets). ai-buddy's Harness model:
   user attaches their own MCP-compatible agent, not a sandboxed plugin
   platform.
4. **Local-first Spatial Layer.** Works offline, no permissions, no cloud, no
   API key required for idle life. Desktop pets are local but ai-buddy
   explicitly separates the Spatial Layer (always local) from the Functional
   Layer (BYO cloud or local model).
5. **Director that proposes, never animates.** Static weights or session model
   proposes a Behavior; engine plays it locally. Character stays visibly alive
   while model thinks or is absent. Desktop pets have XML graphs (Shimeji-ee),
   no visible liveliness system (Desktop Mate, VPet), or plugin-driven (OpenPets).

What other projects have that ai-buddy doesn't (yet):

1. **Character ecosystems.** Desktop Mate has 40+ licensed DLC, VPet has Steam
   Workshop, Shimeji-ee has 1000s of fan packs, OpenPets has openpets.dev
   catalog. ai-buddy has two shipped Characters and import adapters
   (`scripts/import-pet.py`) for petdex / Pets Codex and Shimeji-ee ecosystems
   (authoring-time translation into Character Packages, not live bridge or
   first-party gallery). Package format is internal and undocumented until v2.
2. **Agent integrations shipped.** Desktop Pet has OpenAI chat, OpenPets has MCP
   + plugin SDK v3 + 9 official plugins. ai-buddy's MCP + Harness is specced,
   not built.
3. **Distribution reach.** Desktop Mate and VPet are on Steam, OpenPets has
   signed Windows builds and catalog. ai-buddy is GitHub releases with no store
   presence.

## Sources

Capabilities marked ✓, ~, or ❌ for ai-buddy are verified against docs/SPEC.md,
DESIGN.md, README.md, and `git log` on main as of September 3, 2026. Similar
projects verified against Steam pages (Desktop Mate [App ID
3301060](https://store.steampowered.com/app/3301060/Desktop_Mate/),
VPet-Simulator [App ID
1920960](https://store.steampowered.com/app/1920960/VPetSimulator/)), official
sites ([Kilkakon](https://kilkakon.com/shimeji/) for Shimeji-ee,
[desktoppet.app](https://desktoppet.app/) for Desktop Pet), GitHub repositories
([alvinunreal/openpets](https://github.com/alvinunreal/openpets),
[LorisYounger/VPet](https://github.com/LorisYounger/VPet),
[DalekCraft2/Shimeji-Desktop](https://github.com/DalekCraft2/Shimeji-Desktop)),
OpenPets documentation
([docs/architecture.md](https://github.com/alvinunreal/openpets/blob/main/docs/architecture.md),
[docs/desktop.md](https://github.com/alvinunreal/openpets/blob/main/docs/desktop.md)),
and third-party coverage. No fabricated features.
