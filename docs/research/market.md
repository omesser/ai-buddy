# Desktop pet category — what buyers want and which five have traction

Research for feature comparison. Question: what is the desktop pet / mascot
category, what jobs do buyers hire them for, and which five software alternatives
have documented user bases?

**Answer.** Desktop pets are animated characters that live on your screen as
overlays — they sit on windows, react to mouse, obey physics or wander freely.
Buyers hire them for different jobs (see segments below). Five software
alternatives selected by documented user counts: Desktop Mate (2M vendor claim,
Steam F2P + paid DLC, Mixed 61% English), VPet-Simulator (51,678 Steam reviews,
Overwhelmingly Positive 98%, care-sim + Workshop), Shimeji-ee ecosystem (500K+
Android, decades of fan packs), Desktop Pet (free beta, minimal public voice,
BYO OpenAI key), OpenPets (1,130 GitHub stars, MIT, Electron, plugin SDK + MCP).
Selection based on documented download counts, Steam reviews, and GitHub stars,
not by the prior-art list in DESIGN.md.

## How buyers segment

Buyers hire a character to occupy the desktop. Common jobs (cited from Steam
reviews, GitHub READMEs, not analyst vibes):

1. **My character / pack / VRM / OC on the desktop.** Shimeji-ee pack culture
   (1000s of fan-made packs shared on DeviantArt, Tumblr); VPet Workshop
   (51,678 reviews, 98% positive); Mate-Engine emergence (3,400 GitHub stars,
   "free Desktop Mate alternative with VRM + Workshop"). Desktop Mate removed
   mod support in Feb 2025, which triggered community backlash and switching
   language: "I've just uninstalled Desktop Mate because of this" (Steam
   discussion thread); "obvious cash grab," "removed mod support to make you
   purchase the overpriced DLC" (GameBrain review cluster); "trying to kill the
   modding scene won't turn out well" (Steam technical support forum, 2026). The
   job is *my* character, not a publisher's SKU roster.

2. **Idle companion during work/study.** VPet positive cluster: "boost work
   motivation," "desktop companion enhances experience" (VaporLens sentiment
   analysis, 31% and 28% respectively). Desktop Mate sentiment split: "comforting
   presence" vs "just there" + DLC annoyance. Desktop Pet homepage: Pomodoro,
   break reminders.

3. **Care-sim / Tamagotchi.** VPet official Steam description: "feed, bathe,
   play mini-games"; Workshop tags include "pet simulator." OpenPets Virtual Pet
   plugin (Tamagotchi-style hunger, affection, energy).

4. **Licensed 3D DLC roster.** Desktop Mate store: Hatsune Miku, Hello Kitty,
   Sanrio, VTuber personas, Touhou Project at $7.49–$14.99 each (40+ SKUs). This
   is a minority of the category; Steam English reviews are Mixed (61% of 5,278),
   with recurring DLC cost complaints.

5. **Coding-agent sidekick.** OpenPets README: MCP for Claude Code, OpenCode,
   Cursor, Pi; tools `openpets_status`, `openpets_react`, `openpets_say`. This
   is the thinnest segment by public volume (1,130 stars vs 51,678 VPet reviews).

**Vendor/analyst disagreement.** Published "desktop AI robot pet" reports mix
hardware robots (Living.AI Aibo, Misty Robotics) with software overlays, which
is why 2026 numbers span roughly $14M–$480M+. AI chat features exist in
OpenPets plugins, Desktop Pet roadmap, and Desktop Mate feature requests, but
Steam review language for Desktop Mate and VPet is dominated by "cute,"
"companion," "desktop," "character," not "AI." Do not describe this category as
racing to add AI.

## TAM caveat

Analyst reports estimate "Desktop AI Robot Pets" at $14–590M USD in 2026 with
5.2–10.9% projected CAGR to 2032–2034, varying by scope and methodology. The
high estimates ($480M+) include physical desktop robots (Living.AI, Sony Aibo,
Misty Robotics); the low estimates ($14–17M) track software-only desktop
overlays. ai-buddy is software overlay, so the relevant band is the lower one:
**$14–24M annually, mid-single-digit growth**. Do not treat the $480M reports as
the size of this category. Examples of the mix:

- **$142M** (2026) → $246M (2032), 9.4% CAGR
  ([360iResearch](https://www.360iresearch.com/library/intelligence/desktop-ai-robot-pets),
  August 16, 2026)
- **$480M** (2025) → $1,247M (2034), 10.9% CAGR
  ([DataInsightsReports](https://www.datainsightsreports.com/reports/desktop-ai-robot-pet-400016))

## The five alternatives

Listed in descending order by documented user counts:

1. **VPet-Simulator** — 51,678 Steam reviews (98% positive, Overwhelmingly
   Positive) as of September 2026. Free and open source (GitHub:
   LorisYounger/VPet). Launched Aug 13, 2023. Windows only (official Steam
   platforms: `mac: false`, `linux: false`; Proton users report non-transparent /
   black background, ValveSoftware/Proton#8471). Steam Charts all-time peak
   85,283 players. No purchase price; two paid DLC (ModMaker, Pancake Cat Skin
   package). Extensive Steam Workshop support for community animations,
   interactions, skins. Built to promote VUP Simulator; the desktop pet is a
   standalone extraction. Animation assets require PNG sequence-frame structure
   (`{status}/{type}/{name}_{action}_{time}.png`).
   ([Steam](https://store.steampowered.com/app/1920960/VPetSimulator/);
   [GitHub](https://github.com/LorisYounger/VPet);
   [SteamPulse](https://steampulse.org/game/1920960/metadata))

2. **Desktop Mate** — 2M+ cumulative downloads claim by vendor Infinite Loop
   (June 2026 PR, not audited Steam count). Free-to-play on Steam (App ID
   3301060, launched Jan 7, 2025). Base app is free; 40+ DLC character packs at
   $7.49–$14.99 each. Steam review sentiment: Mixed, 61% positive of 5,278
   English reviews (6,202 positive / 9,262 total all-language reviews as of
   2026-09-03). Mod support removed Feb 2025; anti-modding measures triggered
   community backlash (Steam discussion threads cite uninstalls, switching to
   Mate-Engine). Mac version (Apple Silicon, open beta) launched June 24, 2026.
   Official platforms: Windows + macOS only (Steam `platforms.linux=false`);
   Proton/community ports exist but "doesn't work on linux / black desktop"
   review cluster. Daily concurrent players ~2.9K (Steam Charts). Characters are
   3D models that sit on windows, react to mouse, include voice lines and alarm
   integrations. Multi-Character Mode (up to two characters simultaneously).
   ([Steam](https://store.steampowered.com/app/3301060/Desktop_Mate/);
   [Infinite Loop PR](https://www.infiniteloop.co.jp/pr-blog/2026/06/desktop-mate-2-million-downloads/))

3. **Shimeji-ee ecosystem** — Windows-first Java desktop mascot, forked and
   maintained across multiple repositories (Kilkakon's v1.0.13 is the reference
   build). No centralized download tracker. Android port ("Shimeji - desktop
   pet") reached 500K+ downloads on Google Play with ~790K total installations
   (mid-2025). Desktop version distributed via
   [kilkakon.com](https://kilkakon.com/shimeji/),
   [SourceForge](https://sourceforge.net/app/shimeji-ee/), and fan sites.
   Character packs are community-created and shared free on DeviantArt, Tumblr,
   dedicated archives. Shimeji Shop ([shimejishop.com](https://shimejishop.com/))
   sells individual packs at ~$8.90 each. The mobile app ("Shimeji: Screen
   Buddies" by Digital Cosmos) has 10M+ downloads on Android. DalekCraft2's fork
   adds "Boss mode" (double-click tray icon to hide all mascots) and
   multi-monitor toggle. Required animation set includes Fall/Thrown; gravity
   integrator in the Java lineage (sprite kinematics + throw/climb).
   ([Kilkakon](https://kilkakon.com/shimeji/);
   [DalekCraft2/Shimeji-Desktop](https://github.com/DalekCraft2/Shimeji-Desktop))

4. **OpenPets** — 1,130 GitHub stars (as of September 3, 2026). MIT licensed,
   Electron-based desktop companion platform. Launched May 2026. Animated pets
   idle/wander/react out of the box; no agent required. Official plugins: focus
   timer, reminders, mood check-in, mini games, launcher, hydration, virtual pet
   stats (Tamagotchi-style). Plugin SDK v3: sandboxed JS/TS runtime, permissions
   model, schedules, storage, audio, notifications, ctx.ai with user-configured
   Anthropic/OpenAI/Ollama keys. MCP server for Claude Code, OpenCode, Cursor,
   Pi: tools `openpets_status`, `openpets_react`, `openpets_say`. Speech
   sanitized (no paths/secrets). Catalog at openpets.dev is optional. Releases:
   macOS arm64/x64 dmg, Windows signed exe, Linux AppImage (Wayland overlay
   issues reported: focus steal #32, invisible pet #108/#141). Local-first, no
   accounts. Gravity overlay + Walkabout roam (motion-engine in desktop.md), not
   window-edge Perch riding. Closest software alternative to ai-buddy on
   overlay + MCP architecture.
   ([GitHub](https://github.com/alvinunreal/openpets);
   [openpets.dev](https://openpets.dev/))

5. **Desktop Pet** — Free beta desktop companion by independent developer
   (desktoppet.app). Windows 10/11 (~150MB zip, v1.1.1, Oct 19, 2025) and macOS
   10.15+ (~191MB dmg). Homepage (as of 2026-09-03): "0 Downloads / Free (Beta) /
   1 Pet Type +2 Coming Soon." Cats/dogs/bunnies with unique animations and
   personality (1 type shipped, +2 coming soon). Software overlay that roams the
   screen; click/drag, right-click menu, double-click interact, Tab to place.
   Pomodoro focus timer, break and hydration reminders, sound effects. AI
   Assistant Mode (voice or text chat) with user's own OpenAI API key; wake word
   default "Hey Pet"; key stored locally. Privacy claims: data on device, no
   collection; conversations not saved permanently. AI chat requires internet +
   OpenAI. Unsigned builds; DirectX 11 / Metal. No public customer voice found
   (no independent reviews, no Steam page, no GitHub community). Vendor-only
   evidence.
   ([desktoppet.app](https://desktoppet.app/))

## Substitutes and non-consumption

- **Doing nothing.** Most people do not have a desktop pet.
- **Wallpaper Engine class.** "Don't disrupt me." Buyers who want aesthetic
  only, not interaction or focus-steal. Desktop pets that steal focus or cover
  clicks get uninstalled: VPet Auto Hide workshop mods; OpenPets #32 focus steal;
  Desktop Goose "make it stop" sentiment.
- **Mate-Engine** (shinyflvre/Mate-Engine on GitHub). Emerging free alternative
  to Desktop Mate with custom VRM support, Workshop integration, and no DLC.
  3,400 GitHub stars as of 2026-09-03. Named in Desktop Mate negative reviews as
  exit: "Recommendation for MateEngine" (22% of VaporLens feedback cluster).
  ([GitHub](https://github.com/shinyflvre/Mate-Engine))
- **Desktop Goose.** 2.1M downloads on Uptodown, 250K+ by February 2020 when it
  went viral. Free download (itch.io and mirrors). A mischievous goose that
  steals your mouse, drags windows, and leaves memes on your desktop. Last
  release v0.3 (Feb 2020); no updates since. Windows-only. Viral novelty;
  switching language is "how do I close this" and "make it stop." Not a sustained
  product.
  ([Uptodown](https://desktop-goose.en.uptodown.com/windows))
- **Mobile Shimeji / Shijima.** Android 10M+ downloads; adjacent category, not
  desktop software overlay.
- **Hardware robots.** Living.AI, Sony Aibo, Misty Robotics. Adjacent category;
  TAM pollution in analyst reports.

## Honorable mentions (not in the five)

- **WindowPet** (MIT, Tauri, 631 GitHub stars, ~8.2K downloads v0.0.9) —
  Smaller scale than the five chosen, but cited in DESIGN.md as the reference
  for ai-buddy's click-through implementation.
- **desktop-homunculus** (MIT/Apache, Bevy, MCP server, early alpha) and
  **UI-TARS-desktop** (Apache-2.0, Electron) — Technical references listed in
  DESIGN.md with no published user counts. Informed design, not alternatives to
  compare against.
- **petdex** (17K CLI downloads, 4K GitHub stars, May 2026) — Import source for
  ai-buddy (`scripts/import-pet.py`). Coding-agent specific (reacts to agent
  events via hooks). Different job than general desktop pets.

## Validation against DESIGN.md's list

DESIGN.md Prior art names Shimeji-ee, WindowPet, desktop-homunculus, and
UI-TARS-desktop. Of those, Shimeji-ee appears in the five by documented user
base (500K+ Android). WindowPet (8.2K downloads) is smaller than the five chosen
but credited as ai-buddy's click-through reference. desktop-homunculus and
UI-TARS-desktop are technical references with no published user counts. The list
served design decisions; this research identifies widely used software
alternatives.

## Sources

All figures cited inline with links and access dates of September 3, 2026.
Category sizing: 360iResearch (Aug 16, 2026), IntelMarketResearch, Valuates
Reports, DataInsightsReports, LP Information / Market Research. User counts:
Steam pages and review trackers (Desktop Mate App ID 3301060, VPet-Simulator App
ID 1920960 via SteamPulse/Steambase; VPet 51,678 reviews August 2026), Google
Play store listings (Shimeji Android), GitHub stars (OpenPets 1,130 as of Sep 3
2026, Mate-Engine 3,400), vendor claims (Desktop Mate 2M Infinite Loop PR).
Buyer language: Steam reviews via GameBrain/VaporLens sentiment analysis, Steam
discussion forums, GitHub READMEs. No fabricated metrics.
