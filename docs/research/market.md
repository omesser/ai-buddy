# Desktop pet category — context and similar projects

Research for feature comparison. Question: what is the desktop pet / mascot
category, how many people use these programs, and which five projects are the
most widely known?

**Answer.** Desktop pets are animated characters that live on your screen as
overlays — they sit on windows, react to mouse, obey physics or wander freely.
The category ranges from hobbyist freeware to commercial Steam products,
estimated at $14–24M annually (software-only) with mid-single-digit growth.
Five widely used alternatives selected by documented user counts: Desktop Mate
(2M+ downloads, Steam F2P with paid DLC), VPet-Simulator (51K Steam reviews,
22K peak concurrent, OSS), Shimeji-ee ecosystem (500K+ Android, decades of fan
content), Desktop Pet (free beta, Windows/macOS, AI chat with BYO OpenAI key),
OpenPets (782 GitHub stars, MIT, Electron, plugin SDK + optional MCP). Selection
based on documented download counts, Steam reviews, GitHub stars, and software
availability, not by the prior-art list in DESIGN.md.

## The category

Desktop pets / mascots: animated sprites that live on your desktop as
always-on-top overlays. Windows 95-era nostalgia (Shimeji, BonziBuddy) meets
modern implementations (Steam desktop companions, coding-agent pets). Presence-first,
not productivity-first. Some are purely decorative (Desktop Goose, Shimeji-ee);
some add utility (alarms in Desktop Mate, coding-agent integration in petdex);
ai-buddy plans a functional layer (MCP + Harness) on top of spatial presence.

### Category size

Category sizing reports estimate software desktop pets at $14–590M USD in 2026,
with 5.2–10.9% projected CAGR to 2032–2034, varying by scope:

- **$142M** (2026) → $246M (2032), 9.4% CAGR
  ([360iResearch](https://www.360iresearch.com/library/intelligence/desktop-ai-robot-pets),
  August 16, 2026)
- **$16M** (2026) → $23M (2034), 5.6% CAGR
  ([IntelMarketResearch](https://www.intelmarketresearch.com/desktop-ai-robot-pets-market-40998))
- **$17M** (2025) → $24M (2032), 5.2% CAGR
  ([Valuates
  Reports](https://reports.valuates.com/market-reports/QYRE-Auto-9O18227/global-desktop-ai-robot-pets),
  QYRE-Auto-9O18227)
- **$480M** (2025) → $1,247M (2034), 10.9% CAGR
  ([DataInsightsReports](https://www.datainsightsreports.com/reports/desktop-ai-robot-pet-400016))
- **$488M** (2025) → $780M (2032), 7.1% CAGR
  ([LP Information / Market
  Research](https://www.marketresearch.com/LP-Information-Inc-v4134/Global-Desktop-AI-Robot-Pet-44165483/),
  ~2M units sold in 2024 at ~$241/unit)

The wide range reflects definitional ambiguity: the high estimates ($480–590M)
include physical desktop robots (Living.AI, Sony Aibo, Misty Robotics), while
the low estimates ($14–17M) track software-only desktop overlays. ai-buddy is
software, so the relevant band is the lower one: **$14–24M annually, mid-single-digit
growth**. A niche category. Demand drivers cited across reports: urbanization,
mental wellness / companionship, and "advancements in AI interaction"
([IntelMarketResearch](https://www.intelmarketresearch.com/desktop-ai-robot-pets-market-40998);
[DataInsightsReports](https://www.datainsightsreports.com/reports/desktop-ai-robot-pet-400016)).
North America and Asia-Pacific are the primary hubs.

## How the five were selected

## How the five were selected

Selected by **documented user counts for desktop pet software**, not by
similarity to ai-buddy or appearance on DESIGN.md's prior-art list. Criteria:

1. **Verifiable scale.** Download counts from official sources (Steam, GitHub
   releases, SourceForge), app store totals (Google Play), GitHub stars. No
   "popular on Reddit" without numbers.
2. **Actually used, not technical prior art.** desktop-homunculus (MIT/Apache,
   Bevy, MCP server, MOD system, early alpha) and UI-TARS-desktop (Apache-2.0,
   Electron, vision-only executor) are listed in DESIGN.md but have no published
   user base; they informed design. WindowPet (MIT, Tauri, 631 GitHub stars,
   ~8.2K downloads on v0.0.9 across all platforms
   [April 2025](https://github.com/SeakMengs/WindowPet/releases/tag/v0.0.9))
   is credited as the reference for ai-buddy's click-through implementation;
   included as honorable mention below but the five chosen have larger
   documented user bases.
3. **Embodied desktop pets, not chat windows.** Projects that render an animated
   character overlay on the desktop. Chat applications with AI assistants are
   different products — they are productivity tools or conversation apps accessed
   via window or taskbar, not spatial overlays. Not included in this comparison.

### The five

Listed in descending order by documented user counts:

1. **Desktop Mate** — 2M+ cumulative downloads as of June 2026. Free-to-play on
   Steam (App ID 3301060, launched Jan 7, 2025). Base app is free; 40+ DLC
   character packs at $7.49 or ¥2,200 each (some at $14.99 for licensed
   characters), 20% off during sales. Mac version (open beta) launched June 24,
   2026. Daily concurrent players fluctuate between 2,900 and 4,900 (August
   2026). Characters are 3D models that sit on windows, react to mouse, include
   voice lines and alarm integrations. Multi-Character Mode (official beta, now
   stable) lets users display two characters simultaneously. Developed by
   Infinite Loop (Sapporo).
   ([HolidayTravel](https://www.haveagood-holiday.com/en/articles/desktop-mate-2-million-downloads-mac-beta-steam-sale);
   [Steam](https://store.steampowered.com/app/3301060/Desktop_Mate/);
   [SteamPulse](https://steampulse.org/game/3301060))

2. **VPet-Simulator** — 50,795 Steam reviews (98% positive) as of mid-2026,
   6,900 current players, 22,071 tracked peak. Free and open source (GitHub:
   LorisYounger/VPet). Launched Aug 13, 2023. Windows, Mac, Linux, Steam Deck.
   No purchase price; two paid DLC (ModMaker, Pancake Cat Skin package).
   Supports Steam Workshop for community content (animations, interactions,
   custom characters). Built to promote VUP Simulator; the desktop pet is a
   standalone extraction of that program's mascot system. Animation assets
   require specific folder structure and PNG sequence-frame naming conventions
   (`{status}/{type}/{name}_{action}_{time}.png`).
   ([Steam](https://store.steampowered.com/app/1920960/VPetSimulator/);
   [SteamPulse](https://steampulse.org/game/1920960);
   [GG.deals](https://gg.deals/game/vpet-simulator/);
   [GitHub](https://github.com/LorisYounger/VPet))

3. **Shimeji-ee ecosystem** — Windows-first Java desktop mascot, forked and
   maintained across multiple repositories (Kilkakon's v1.0.13 is the reference
   build). No centralized download tracker. Android port ("Shimeji - desktop
   pet") reached 500K+ downloads on Google Play with ~790K total installations
   (mid-2025)
   ([Google Play](https://play.google.com/store/apps/details?hl=en&id=com.anbu.shimeji.desktoppet)).
   Desktop version distributed via
   [kilkakon.com](https://kilkakon.com/shimeji/),
   [SourceForge](https://sourceforge.net/app/shimeji-ee/), and fan sites; no
   aggregate count available. Character packs are community-created and shared
   free on DeviantArt, Tumblr, and dedicated archives. Shimeji Shop
   ([shimejishop.com](https://shimejishop.com/)) sells individual character
   packs at ~$8.90 each. The mobile app ("Shimeji: Screen Buddies" by Digital
   Cosmos) has 10M+ downloads on Android and includes Magic AI character
   generation. DalekCraft2's fork adds "Boss mode" (double-click tray icon to
   hide all mascots) and multi-monitor toggle.
   ([Kilkakon](https://kilkakon.com/shimeji/);
   [SourceForge](https://sourceforge.net/app/shimeji-ee/);
   [Google Play](https://play.google.com/store/apps/details?id=com.digitalcosmos.shimeji);
   [DalekCraft2/Shimeji-Desktop](https://github.com/DalekCraft2/Shimeji-Desktop))

4. **Desktop Pet** — Free beta desktop companion by independent developer
   (desktoppet.app). Windows 10/11 (~150MB zip, v1.1.1, Oct 19, 2025) and macOS
   10.15+ (~191MB dmg). Cats/dogs/bunnies with unique animations and
   personality; 1 type shipped, +2 coming soon. Software overlay that roams the
   screen; click/drag, right-click menu, double-click interact, Tab to place.
   Pomodoro focus timer, break and hydration reminders, sound effects. AI
   Assistant Mode (voice or text chat) with user's own OpenAI API key; wake word
   default "Hey Pet"; key stored locally. Privacy-first claims: data on device,
   no collection; conversations not saved permanently. AI chat still requires
   internet + OpenAI. Unsigned builds; DirectX 11 / Metal. Free beta. Google
   Form feedback. 0 downloads reported on homepage (beta, no published metrics).
   ([desktoppet.app](https://desktoppet.app/))

5. **OpenPets** — 782 GitHub stars (as of Sep 3, 2026). MIT licensed,
   Electron-based desktop companion platform. Launched May 2026. Animated pets
   idle/wander/react out of the box; no agent required. Official plugins: focus
   timer, reminders, mood check-in, mini games, launcher, hydration, virtual pet
   stats (Tamagotchi-style). Plugin SDK v3: sandboxed JS/TS runtime, permissions
   model, schedules, storage, audio, notifications, ctx.ai with user-configured
   Anthropic/OpenAI/Ollama keys. Optional MCP for Claude Code, OpenCode, Cursor,
   Pi: tools `openpets_status`, `openpets_react`, `openpets_say`. Speech
   sanitized (no paths/secrets). Catalog at openpets.dev is optional. Releases:
   macOS arm64/x64 dmg, Windows signed exe, Linux AppImage. Local-first, no
   accounts. Closest software alternative to ai-buddy on Harness/MCP + overlay
   pet architecture.
   ([GitHub](https://github.com/alvinunreal/openpets);
   [openpets.dev](https://openpets.dev/))

5. **Shimeji-ee ecosystem** — Windows-first Java desktop mascot, forked and
   maintained across multiple repositories (Kilkakon's v1.0.13 is the reference
   build). No centralized download tracker. Android port ("Shimeji - desktop
   pet") reached 500K+ downloads on Google Play with ~790K total installations
   (mid-2025)
   ([Google Play](https://play.google.com/store/apps/details?hl=en&id=com.anbu.shimeji.desktoppet)).
   Desktop version distributed via
   [kilkakon.com](https://kilkakon.com/shimeji/),
   [SourceForge](https://sourceforge.net/app/shimeji-ee/), and fan sites; no
   aggregate count available. Character packs are community-created and shared
   free on DeviantArt, Tumblr, and dedicated archives. Shimeji Shop
   ([shimejishop.com](https://shimejishop.com/)) sells individual character
   packs at ~$8.90 each. The mobile app ("Shimeji: Screen Buddies" by Digital
   Cosmos) has 10M+ downloads on Android and includes Magic AI character
   generation.
   ([Kilkakon](https://kilkakon.com/shimeji/);
   [SourceForge](https://sourceforge.net/app/shimeji-ee/);
   [Google Play](https://play.google.com/store/apps/details?id=com.digitalcosmos.shimeji))

**Not included:**

- **WindowPet** (MIT, Tauri, 631 GitHub stars, ~8.2K downloads v0.0.9) —
  Smaller scale than the five chosen, but cited in DESIGN.md as the reference
  for ai-buddy's click-through implementation. Honorable mention.
- **desktop-homunculus** (MIT/Apache, Bevy, MCP server, early alpha) and
  **UI-TARS-desktop** (Apache-2.0, Electron) — Technical references listed in
  DESIGN.md with no published user counts. Informed design, not alternatives to
  compare against.
- **Desktop Goose** (2.1M downloads, viral 2020, last update v0.3 Feb 2020) —
  Widely known from viral moment but dormant since 2020; excluded because the
  five chosen are actively maintained or have recent releases.
- **petdex** (17K CLI downloads, 4K GitHub stars, May 2026) — Coding-agent
  specific (reacts to agent events via hooks). Growing quickly but narrower
  focus than general desktop pets.
- **Chat applications** (ChatGPT desktop, Microsoft Copilot, Character.AI,
  Replika) — Different category. These are conversation apps or OS assistants
  accessed via window or taskbar, not spatial desktop pet overlays.

## Validation against DESIGN.md's list

DESIGN.md Prior art names Shimeji-ee, WindowPet, desktop-homunculus, and
UI-TARS-desktop. Of those, Shimeji-ee appears in the five by documented user
base (500K+ Android). WindowPet (8.2K downloads) is smaller than the five chosen
but credited as ai-buddy's click-through reference. desktop-homunculus and
UI-TARS-desktop are technical references with no published user counts. The list
served design decisions; this research identifies widely used alternatives.

## Sources

All figures cited inline with links and access dates of September 2–3, 2026.
Category sizing: 360iResearch (Aug 16, 2026), IntelMarketResearch, Valuates
Reports, DataInsightsReports, LP Information / Market Research. Downloads:
Steam pages and SteamPulse trackers (Desktop Mate App ID 3301060,
VPet-Simulator App ID 1920960), Google Play store listings (Shimeji Android),
GitHub stars (OpenPets 782 as of Sep 3, 2026), project homepages (Desktop Pet
beta). Pricing: Steam store pages. No fabricated metrics.
