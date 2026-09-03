# ai-buddy versus five well-known alternatives — feature comparison

Comparison against five widely used projects in similar categories (see
[market.md](./market.md)): Desktop Mate, VPet-Simulator, Shimeji-ee ecosystem,
ChatGPT desktop, Microsoft Copilot. Matrix rows are capabilities that matter for
this product, using CONTEXT.md vocabulary. Columns mark each feature as
**shipped** (in production today), **spec** (documented in docs/SPEC.md or a
closed issue, not shipped), or **absent** (no evidence of it existing or being
planned). ai-buddy column is honest about what is and is not built.

## Feature matrix

| Capability | ai-buddy | Desktop Mate | VPet-Simulator | Shimeji-ee | ChatGPT desktop | MS Copilot |
|---|---|---|---|---|---|---|
| **Spatial Layer** | | | | | | |
| Overlay (always-on-top) | shipped | shipped | shipped | shipped | absent | shipped (taskbar) |
| Click-through (per-pixel) | shipped | shipped | shipped | shipped | absent | absent |
| Physics (gravity, throw) | shipped | absent | absent | absent | absent | absent |
| Perches (window top edges) | shipped | shipped | absent | absent | absent | absent |
| Hide rules (fullscreen, hotkey) | shipped | absent | spec | absent | absent | absent |
| Capture exclusion (no screen share) | shipped | absent | absent | absent | absent | absent |
| Multi-instance (several buddies) | shipped | shipped | shipped | shipped | absent | absent |
| Multi-monitor | shipped | shipped | shipped | shipped | shipped | shipped |
| **Character & Art** | | | | | | |
| Character Packages (first-class) | shipped | shipped (DLC) | shipped (Workshop) | shipped (community) | absent | absent |
| Art ecosystem / gallery | absent | 40+ official DLC | Steam Workshop | 1000s fan-made | absent | absent |
| Required Animation Set | 9 animations | unknown | unknown | standard set | absent | absent |
| Declarative Behaviors | shipped (TOML) | absent | absent | absent (XML graphs) | absent | absent |
| **Liveliness** | | | | | | |
| Director (proposes Behaviors) | shipped (Static + HTTP stand-in) | absent | absent | absent (XML-driven) | absent | absent |
| Idle life (model-free) | shipped | shipped | shipped | shipped | absent | absent |
| Reacts to user (Poke, Grab) | shipped | shipped | shipped | shipped | absent | absent |
| Window awareness | shipped (app name, geometry) | shipped | absent | shipped (edges) | absent | absent |
| **Functional Layer** | | | | | | |
| Harness / agent runtime | spec (MCP attach) | absent | absent | absent | shipped | shipped |
| MCP server (buddy-side tools) | spec | absent | absent | absent | absent | absent |
| Executor (synthetic input) | absent (Harness owns) | absent | absent | absent | shipped (Cowork, computer use) | shipped (Agent 365) |
| Chat surface | spec (#17, #119) | absent | absent | absent | shipped | shipped |
| **Memory & Context** | | | | | | |
| Memory (shared, user-editable) | shipped (Markdown file) | absent | absent | absent | shipped (conversation history) | shipped (M365 graph) |
| Ambient Capture | spec (deferred v1) | absent | absent | absent | absent | shipped (Windows screenshots) |
| On-Demand Capture | spec (deferred v1) | absent | absent | absent | absent | shipped |
| **Privacy & Control** | | | | | | |
| Local-first (no cloud required) | shipped (Spatial) | shipped | shipped | shipped | absent | absent |
| BYO model / API key | shipped (env vars) | absent | absent | absent | absent (OpenAI only) | absent (MS only) |
| Consent-per-feature (opt-in gates) | shipped (Settings UI) | absent | absent | absent | OS-level TCC | OS-level TCC |
| Denylist (excluded apps) | spec | absent | absent | absent | absent | absent |
| **Platforms** | | | | | | |
| macOS | shipped | shipped (beta, June 2026) | shipped | shipped (patched forks) | shipped | shipped (11+) |
| Windows | spec (stubbed deliberately) | shipped | shipped | shipped | shipped | shipped (11 bundled) |
| Linux | shipped (X11), degraded (Wayland) | shipped | shipped | shipped (community forks) | absent | absent |
| **Pricing & Distribution** | | | | | | |
| Base app price | free (OSS MIT) | free (Steam F2P) | free (OSS) | free | free tier + $20/mo Plus | free (bundled) + $30/mo M365 Copilot |
| Character DLC | absent | $7.49–$14.99 each | free (Workshop) + 2 paid DLC | free (community) + ~$8.90 (Shimeji Shop) | absent | absent |
| Subscription model | absent | absent | absent | absent | $20/mo Plus, $200/mo Pro | $30/mo M365 Copilot (enterprise volume) |
| Distribution | GitHub releases | Steam | Steam | web downloads, fan sites | web + desktop app (July 2026) | Windows 11 OS bundle + M365 |

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
Project). Multi-instance not confirmed but implied by "add a wide variety of
cute characters" language. Built-in alarm feature; some DLC include mascot
characters that appear during alarms
([SNOW MIKU 2026 Ver. DLC](https://store.steampowered.com/app/4018720/Desktop_Mate_SNOW_MIKU_2026_Ver_DLC/)).
Mac version (Apple Silicon, open beta) launched June 24, 2026
([HolidayTravel](https://www.haveagood-holiday.com/en/articles/desktop-mate-2-million-downloads-mac-beta-steam-sale)).

**Absent.** No physics (gravity, throw). No hide rules mentioned (fullscreen,
screen sharing). No capture exclusion verified. No agent capabilities, no MCP,
no chat. Characters are purchased DLC, not user-authorable packages. Mod support
was removed before or during Steam launch, which "upset the community" and is
"widely criticized as an expensive, aggressive cash grab"
([GameBrain](https://gamebrain.co/game/desktop-mate): user reviews call it
"obvious cash grab," "removed mod support to make you purchase the overpriced
DLC," "exploitative"). No evidence of character behavior authoring (director,
personality prompts, declarative Behaviors).

**Differences.** Desktop Mate monetizes licensed character DLC ($14.99
each, 40+ available); ai-buddy has two shipped Characters and an internal
package format (undocumented until v2). Desktop Mate has no physics, no
functional layer, and no BYO character creation. ai-buddy's Spatial Layer
includes physics and Perch riding/dropping under a gate, and its planned
Functional Layer (MCP + Harness attach) targets agent capabilities Desktop Mate
does not attempt.

### VPet-Simulator

**What it is.** Free and open source desktop pet on Steam (App ID 1920960,
launched Aug 13, 2023). 50,795 reviews, 98% positive. 6,900 current players,
22,071 tracked peak (August 2026). Windows, Mac, Linux, Steam Deck. Built to
promote VUP Simulator; the desktop pet is extracted from that program.
([Steam](https://store.steampowered.com/app/1920960/VPetSimulator/);
[GitHub](https://github.com/LorisYounger/VPet))

**Verified present.** Overlay. Multi-instance implied ("spawn multiple pets").
Extensive Steam Workshop support for community animations, interactions, skins
(stated in Steam description and community discussions). Two paid DLC (ModMaker,
Pancake Cat Skin package) plus free Workshop content. No purchase price for base
app. Cross-platform (Windows, Mac, Linux, Steam Deck). Open source on GitHub
([LorisYounger/VPet](https://github.com/LorisYounger/VPet)).

**Absent.** No physics (gravity, throw) mentioned. No window top-edge Perches
(sprite appears to rest on desktop floor, not on windows). No agent
capabilities, MCP, chat, or Harness. No Memory system. No Ambient Capture. No
evidence of screen-sharing exclusion or hide rules.

**Differences.** VPet's Workshop ecosystem is live and massive (open source
+ Steam Workshop), while ai-buddy's Character Package format is internal and
undocumented until v2. VPet has no window awareness or Perches, no physics, and
no agent / MCP layer. ai-buddy's Director + Harness attach targets capabilities
VPet does not have.

### Shimeji-ee ecosystem

**What it is.** Windows-first Java desktop mascot, originally by Yuki Yamada /
Group Finity (2009, zlib/libpng), forked and maintained as Shimeji-ee by
Kilkakon and others (New BSD). Distributed via
[kilkakon.com](https://kilkakon.com/shimeji/),
[SourceForge](https://sourceforge.net/app/shimeji-ee/), and fan sites. Android
port has 500K+ downloads. Character packs are community-created and shared on
DeviantArt, Tumblr, dedicated archives.
([Kilkakon](https://kilkakon.com/shimeji/))

**Verified present.** Overlay. Click-through (shimeji walk on windows, fall off
edges). Multi-instance (spawn many shimeji). Window edge awareness (shimeji sit
on window tops and move between them). Huge community art ecosystem (1000s of
fan-made packs). Cross-platform via forks (macOS via shimeji4mac, Linux via
linux-shimeji). Per-character XML behavior graphs declaring pose sequences.
([Kilkakon site](https://kilkakon.com/shimeji/);
[DESIGN.md](../../DESIGN.md) cites it as "per-character XML behavior graphs")

**Absent.** No physics (gravity is present but no ballistic throw). No capture
exclusion or hide rules documented. No agent capabilities, no MCP, no Harness.
No Memory. No Ambient Capture. No personality prompts or model-driven Director.
Behavior graphs are XML data but hard to author; DESIGN.md: "after fifteen
years the overwhelming majority of community packages are art reskins of the
default XML, because the graph was too hard to author."

**Differences.** Shimeji-ee has decades of community content and proven
longevity, but behavior authoring is difficult (XML graphs, not declarative
Primitives). ai-buddy's engine-owned Primitives and Character-declared Behaviors
keep authoring simpler. Shimeji-ee has no functional layer, no MCP, no agent.
ai-buddy's planned Harness attach is a category ai-buddy introduces.

### ChatGPT desktop

**What it is.** Unified desktop app for macOS and Windows, launched July 9,
2026. Merges chat, ChatGPT Work (agent tasks), and Codex (coding assistant).
Free tier, $20/mo Plus, $200/mo Pro. Global keyboard shortcut (Option+Space on
macOS). Desktop-specific download counts not disclosed; 1B MAU total across all
platforms (June 2026).
([Archynewsy](https://www.archynewsy.com/openai-merges-codex-into-unified-chatgpt-desktop-app/);
[QRCodePress](https://www.qrcodepress.com/one-billion-people-use-chatgpt-weekly/8545876/))

**Verified present.** Desktop app (native, not Electron per
[Archynewsy](https://www.archynewsy.com/openai-merges-codex-into-unified-chatgpt-desktop-app/)).
Multi-monitor (inferred from "macOS and Windows" support). Harness / agent
runtime (ChatGPT Work mode supports long-running agent tasks, Codex for coding).
Executor (computer use via native integration, announced March 23–24, 2026;
Cowork does computer use via Claude desktop per Microsoft Q4 2026 earnings).
Chat surface (core feature). Memory (conversation history, synced across
devices per July 16–17 update). Ambient and On-Demand Capture (computer use
requires screen access). macOS and Windows.
([CryptoBriefing](https://cryptobriefing.com/openai-chatgpt-desktop-sync-update/))

**Absent.** No spatial layer (overlay, physics, Perches). No Character Packages.
No idle life or window awareness (the app is a chat window, not a sprite). No
local-first mode (requires OpenAI account and API). No BYO model (OpenAI models
only). No click-through (it is a window, not a transparent overlay). No
multi-instance in the spatial sense (one chat window).

**Differences.** ChatGPT desktop is capability-first with no spatial layer.
ai-buddy's Spatial Layer (physics, Perches, idle life without a model) is a
category ChatGPT does not enter. ChatGPT's Functional Layer is already shipped;
ai-buddy's is specced (MCP server + BYO Harness attach). ChatGPT is cloud-only;
ai-buddy works offline. ChatGPT is a window; ai-buddy is an overlay sprite.

### Microsoft Copilot

**What it is.** AI assistant bundled into Windows 11, Microsoft 365, Edge, Bing.
420M MAU (Q1 2026), 160M enterprise licensed users. Rendered via WebView2 in
Windows 11, consuming 200MB–1GB RAM while active. Free tier (bundled) plus
$30/mo M365 Copilot for enterprise.
([Stackmatix](https://www.stackmatix.com/blog/microsoft-copilot-adoption-statistics-2026);
[MakeUseOf](https://www.makeuseof.com/i-disabled-copilot-in-windows-11/))

**Verified present.** Desktop integration (Windows 11 taskbar icon, Copilot
sidebar). Multi-monitor (inferred from OS-level integration). Harness / agent
runtime (Copilot in M365 apps, Windows). Executor (Agent 365 announced;
Microsoft Q4 2026 earnings: nearly 40 million agents across 10K+ companies).
Chat surface (sidebar and standalone window). Memory (Microsoft 365 graph,
chat history). Ambient Capture (Windows 11 Recall feature takes periodic
screenshots; Copilot Vision reads screen during sessions). macOS 11+ and Windows
11.
([Stackmatix](https://www.stackmatix.com/blog/microsoft-copilot-adoption-statistics-2026);
[Yahoo Tech](https://tech.yahoo.com/ai/copilot/articles/microsoft-says-workers-now-using-190007425.html))

**Absent.** No spatial layer (overlay, physics, Perches). Copilot is a sidebar
or window, not a sprite. No Character Packages. No idle life as a mascot. No
local-first mode (cloud-required). No BYO model (Microsoft models only). No
click-through (window-based). No multi-instance in the mascot sense.

**Differences.** Copilot is bundled OS assistant with agent capabilities but
no spatial layer. ai-buddy's Spatial Layer is presence-first; Copilot is
capability-first with no sprite, no physics, no window-edge awareness. Copilot
is cloud-required; ai-buddy works offline. ai-buddy's BYO Harness model (MCP
attach) vs Copilot's closed Microsoft runtime is the other divide.

## Differences from similar projects

What ai-buddy does differently (as specced):

1. **Spatial Layer + Functional Layer together.** Desktop pets (Desktop Mate,
   VPet, Shimeji-ee) have presence and no agent. AI assistants (ChatGPT,
   Copilot) have agent capabilities and no spatial layer. ai-buddy combines
   both.
2. **Physics.** Gravity + ballistic throw + Perch riding under an acceleration
   gate.
3. **BYO Harness.** MCP server + user-attached agent runtime. ChatGPT and
   Copilot are closed to their own models. Desktop pets have no agent layer.
4. **Local-first Spatial Layer.** Works offline, no permissions, no cloud, no
   API key required. ChatGPT and Copilot require accounts and cloud. Desktop
   pets are local but have no functional layer.
5. **Director that proposes, never animates.** Static weights or session model
   proposes a Behavior; engine plays it locally. Character stays visibly alive
   while model thinks or is absent. Desktop pets have XML graphs (Shimeji-ee) or
   no liveliness system (Desktop Mate, VPet). AI assistants have no idle life.

What other projects have that ai-buddy doesn't (yet):

1. **Character ecosystems.** Desktop Mate has 40+ licensed DLC, VPet has Steam
   Workshop, Shimeji-ee has 1000s of fan packs. ai-buddy's package format is
   internal and undocumented until v2, with two shipped Characters.
2. **Functional Layer shipped.** ChatGPT and Copilot have agent runtimes, chat
   surfaces, and computer use today. ai-buddy's is specced, not built.
3. **Distribution reach.** Desktop Mate and VPet are on Steam, Copilot is
   bundled in Windows, ChatGPT has 1B MAU. ai-buddy is GitHub releases with no
   store presence.

## Sources

Capabilities marked "shipped," "spec," or "absent" for ai-buddy are verified
against docs/SPEC.md, DESIGN.md, README.md, and `git log` on main as of
September 3, 2026. Similar projects verified against Steam pages (Desktop Mate
[App ID 3301060](https://store.steampowered.com/app/3301060/Desktop_Mate/),
VPet-Simulator [App ID
1920960](https://store.steampowered.com/app/1920960/VPetSimulator/)), official
sites ([Kilkakon](https://kilkakon.com/shimeji/) for Shimeji-ee), press
releases and company disclosures (Microsoft Q4 2026 earnings via
[Yahoo Tech](https://tech.yahoo.com/ai/copilot/articles/microsoft-says-workers-now-using-190007425.html);
OpenAI July 2026 desktop launch via
[Archynewsy](https://www.archynewsy.com/openai-merges-codex-into-unified-chatgpt-desktop-app/)
and
[CryptoBriefing](https://cryptobriefing.com/openai-chatgpt-desktop-sync-update/)),
and third-party technical breakdowns
([MakeUseOf](https://www.makeuseof.com/i-disabled-copilot-in-windows-11/) for
Copilot RAM usage). No fabricated features.
