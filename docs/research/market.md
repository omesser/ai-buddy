# Desktop companion category — context and similar projects

Research for feature comparison. Question: what category does ai-buddy sit in,
how many people use similar projects, and which five projects are the most
widely known?

**Answer.** Two overlapping categories: traditional desktop pets (Shimeji-ee,
Desktop Mate, VPet) and AI desktop assistants (ChatGPT desktop, Microsoft
Copilot). Desktop pet software ranges from hobbyist projects to commercial
products; the category is estimated at $14–590M globally in 2026 depending on
scope, growing 5–11% CAGR. AI assistant space is larger (420M–1B MAU) but
bundled into operating systems rather than standalone. Five widely used
alternatives selected by documented user counts: Desktop Mate (2M+ downloads,
Steam), VPet-Simulator (51K reviews, 22K peak concurrent), Shimeji-ee ecosystem
(500K+ Android, decades of fan content), ChatGPT desktop (1B MAU total, July
2026 unified desktop launch), Microsoft Copilot (420M MAU, 160M enterprise
seats). Selection based on documented download counts, Steam review volume,
concurrent player peaks, and MAU disclosures, not by the prior-art list in
DESIGN.md.

## The category and its variants

ai-buddy sits at the intersection of two categories that have not historically
overlapped: **desktop pets / mascots** (presence-first, model-optional, Windows
95 nostalgia) and **AI desktop assistants** (capability-first, no spatial
layer, chat or command bar). The Spatial Layer is similar to traditional desktop
pets; the Functional Layer is similar to AI assistants that can operate a
computer. No other project combines both in one product as of September 2026.

### Desktop pets and mascots

Category sizing reports estimate the "Desktop AI Robot Pets" space at $14–590M
USD in 2026, with 5.2–10.9% projected CAGR to 2032–2034, varying by scope and
methodology:

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

### AI desktop assistants

Counted in hundreds of millions of MAU rather than tens of millions of dollars.
Microsoft Copilot reached **420 million monthly active users** across all
surfaces (Windows, Edge, Microsoft 365, Bing, mobile) in Q1 2026, with 160
million enterprise-licensed users
([Stackmatix](https://www.stackmatix.com/blog/microsoft-copilot-adoption-statistics-2026),
sourced from Microsoft earnings). Enterprise users average 11.3 Copilot
interactions per workday; 67% report daily usage
([Stackmatix](https://www.stackmatix.com/blog/microsoft-copilot-adoption-statistics-2026)).
ChatGPT crossed **1 billion monthly active users** in June 2026, making it the
fastest app in history to that scale
([QRCodePress](https://www.qrcodepress.com/one-billion-people-use-chatgpt-weekly/8545876/),
citing Reuters and Sensor Tower, June 2026). Weekly actives hit 1 billion in
July 2026
([QRCodePress](https://www.qrcodepress.com/one-billion-people-use-chatgpt-weekly/8545876/)).
The unified ChatGPT desktop app for macOS and Windows launched July 9, 2026
([Archynewsy](https://www.archynewsy.com/openai-merges-codex-into-unified-chatgpt-desktop-app/);
[CryptoBriefing](https://cryptobriefing.com/openai-chatgpt-desktop-sync-update/),
July 16–17 updates). OpenAI does not break out desktop-only download counts
from the 1B total.

Both are **bundled distribution**, not standalone downloads. Copilot ships in
Windows 11 and Microsoft 365; ChatGPT is free-to-paid SaaS. Neither is acquired
as "a desktop companion."

## How the five were chosen

Selected by **documented user base evidence**, not by similarity to
ai-buddy or appearance on DESIGN.md's prior-art list. Criteria:

1. **Verifiable scale.** Download counts from official sources (Steam, GitHub
   releases, SourceForge), app store totals (Google Play), or platform-disclosed
   MAU (company earnings, press releases). No "popular on Reddit" without
   numbers.
2. **Actually used, not technical prior art.** desktop-homunculus (MIT/Apache,
   Bevy, MCP server, MOD system) and UI-TARS-desktop (Apache-2.0, Electron,
   vision-only executor) are listed in DESIGN.md but have no published user
   base; they informed design. WindowPet (MIT, Tauri, 631
   GitHub stars, ~8.2K downloads on v0.0.9 across all platforms
   [April 2025](https://github.com/SeakMengs/WindowPet/releases/tag/v0.0.9))
   is credited as the reference for ai-buddy's click-through implementation but
   is two orders of magnitude smaller than the five chosen.
3. **Active in 2026.** Desktop Goose reached 250K downloads by February 2020
   ([Hypertext](https://htxt.co.za/2020/02/desktop-goose-is-spreading-untitled-chaos-around-the-world/)),
   grew to 2.1M on Uptodown
   ([Uptodown](https://desktop-goose.en.uptodown.com/windows), "2.1 M
   downloads"), but is a one-time viral novelty with no updates since v0.3.
   Excluded for being dormant.

### The five

Listed in descending order by documented user counts:

1. **Microsoft Copilot** — 420M MAU (Q1 2026), 160M enterprise licensed users.
   Bundled into Windows 11 desktop, rendered via WebView2, consumes 200MB–1GB
   RAM while active
   ([MakeUseOf](https://www.makeuseof.com/i-disabled-copilot-in-windows-11/)).
   Free-to-paid ($30/mo M365 Copilot, enterprise volume pricing). Distribution
   is OS bundling; users do not "download Copilot."
   ([Stackmatix](https://www.stackmatix.com/blog/microsoft-copilot-adoption-statistics-2026))

2. **ChatGPT desktop** — 1B MAU total (June 2026, all platforms combined).
   Unified desktop app for macOS and Windows launched July 9, 2026, merging
   chat, Work mode, and Codex. Free tier plus $20/mo Plus, $200/mo Pro.
   Desktop-specific download counts not disclosed. Global keyboard shortcut
   (Option+Space on macOS). No spatial layer; entirely chat-surface and agent
   task execution.
   ([QRCodePress](https://www.qrcodepress.com/one-billion-people-use-chatgpt-weekly/8545876/);
   [Archynewsy](https://www.archynewsy.com/openai-merges-codex-into-unified-chatgpt-desktop-app/))

3. **Desktop Mate** — 2M+ cumulative downloads as of June 2026. Free-to-play on
   Steam (App ID 3301060, launched Jan 7, 2025). Base app is free; 40+ DLC
   character packs at $14.99 or ¥2,200 each (some at $7.49 for "Yukkuri"
   variants), 20% off during sales. Mac version (open beta) launched June 24,
   2026. Daily concurrent players fluctuate between 2,900 and 4,900 (August
   2026). Characters are 3D models that sit on windows, react to mouse, include
   voice lines and alarm integrations. Developed by Infinite Loop (Sapporo).
   ([HolidayTravel](https://www.haveagood-holiday.com/en/articles/desktop-mate-2-million-downloads-mac-beta-steam-sale);
   [Steam](https://store.steampowered.com/app/3301060/Desktop_Mate/);
   [SteamPulse](https://steampulse.org/game/3301060))

4. **VPet-Simulator** — 50,795 Steam reviews (98% positive) as of mid-2026,
   6,900 current players, 22,071 tracked peak. Free and open source (GitHub:
   LorisYounger/VPet). Launched Aug 13, 2023. Windows, Mac, Linux, Steam Deck.
   No purchase price; two paid DLC (ModMaker, Pancake Cat Skin package).
   Supports Steam Workshop for community content (animations, interactions,
   custom characters). Built to promote VUP Simulator; the desktop pet is a
   standalone extraction of that program's mascot system.
   ([Steam](https://store.steampowered.com/app/1920960/VPetSimulator/);
   [SteamPulse](https://steampulse.org/game/1920960);
   [GG.deals](https://gg.deals/game/vpet-simulator/))

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

**Not chosen:**

- **petdex** (4,675 open-source pets in gallery, 17,493 CLI downloads, 3,975
  GitHub stars as of September 2, 2026) — launched May 2, 2026, so four months
  old. Growing quickly but nascent. Desktop app is a native floater for macOS,
  Windows, Linux that reacts to coding agent activity (Codex, Claude Code,
  etc.). MIT licensed.
  ([GitHub](https://github.com/crafter-station/petdex);
  [npm](https://www.npmjs.com/package/petdex))
- **Character.AI and Replika** — 20M+ and 42M+ users respectively, but no
  desktop overlay or spatial layer. Web-first companions accessed via browser or
  Progressive Web App. Character.AI's c.ai+ is $9.99/mo, Replika Pro is
  $19.99/mo. Desktop "apps" are browser tabs, not native.
  ([Webeeky](https://webeeky.com/character-ai-for-pc/);
  [AICompanionGuides](https://aicompanionguides.com/blog/desktop-ai-companion-best-options-beyond-mobile/))
- **Desktop Goose** — 2.1M downloads on Uptodown, 250K+ by February 2020.
  Free download, last release v0.3 (Feb 2020). Viral novelty, not a sustained
  product.
  ([Uptodown](https://desktop-goose.en.uptodown.com/windows);
  [Hypertext](https://htxt.co.za/2020/02/desktop-goose-is-spreading-untitled-chaos-around-the-world/))

## Validation against DESIGN.md's list

DESIGN.md Prior art names Shimeji-ee, WindowPet, desktop-homunculus, and
UI-TARS-desktop. Of those, only Shimeji-ee appears in the top five by user
base. WindowPet (8.2K downloads) is smaller than VPet (50K reviews) or Desktop
Mate (2M downloads) by two orders of magnitude. desktop-homunculus and
UI-TARS-desktop are technical references with no published user counts, so they
cannot be ranked by market evidence. The list served design decisions.

## Sources

All figures cited inline with links and access dates of September 2–3, 2026
except where noted. Market sizing: 360iResearch (Aug 16, 2026),
IntelMarketResearch, Valuates Reports, DataInsightsReports, LP Information /
Market Research. MAU and downloads: company disclosures (Microsoft Q1 2026
earnings via Stackmatix; OpenAI via Reuters/Sensor Tower per QRCodePress),
Steam pages and SteamPulse trackers (Desktop Mate App ID 3301060,
VPet-Simulator App ID 1920960), Google Play store listings, GitHub release
download counts. Pricing: Steam store pages, subscription pages captured in
third-party breakdowns (Replika via eesel.ai, Character.AI via RoboRhythms,
tech-insider.org). No fabricated metrics.
