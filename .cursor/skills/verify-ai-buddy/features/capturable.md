# Capturable / hide from captures

By default the buddy appears in screenshots and screen shares. Settings → Presence → "Appear in screenshots and screen shares" (and `AI_BUDDY_CAPTURABLE`) force capturable or hidden so verify scripts can screenshot the sprite or test the hide path.

## Sub-features

- `capturable-default` default is capturable (`true`).
- `capturable-force-on` `AI_BUDDY_CAPTURABLE=1` forces visible in captures.
- `capturable-force-off` `AI_BUDDY_CAPTURABLE=0` forces exclusion where the platform supports it.
- `capturable-settings` the Settings checkbox matches the in-force value when no env override is set.

## How to get to it (user POV)

- Open Settings (tray / platform menu) → Presence → toggle "Appear in screenshots and screen shares".
- Or export `AI_BUDDY_CAPTURABLE=0` or `=1` before launch for a one-process override (CI / verify scripts).

## Driving it with verify-overlay helpers

Preconditions:

- macOS or Windows for real capture-exclusion APIs. Linux has no equivalent exclusion API in this codebase — see Gotchas.
- Doctor green; binary built.

- **Force visible for screenshot proofs.** `export AI_BUDDY_CAPTURABLE=1` then run the platform `drive-overlay-*` helper. Screenshots in the stamp dir should include the sprite when the capture tool honors capturable windows.
- **Force hidden (macOS).** `AI_BUDDY_CAPTURABLE=0 scripts/verify-overlay.sh` path: inspector / sharing type should show exclusion (`sharing=0` style checks in the macOS script's default expectations may assume exclusion historically — read the current script comments; default product is capturable, env overrides for tests).
- **Windows.** `scripts/verify-overlay-win.ps1` asserts `WDA_EXCLUDEFROMCAPTURE` behavior per its checks; copy `$Out` into evidence.
- **Settings smoke (Windows).** `scripts/verify-settings-win.ps1` for the Settings window chrome (not capturable-specific alone).
- **Proof.** Record env value, platform, and either a screenshot with/without sprite or the platform property the script already asserts. Put artifacts under `$AI_BUDDY_VERIFY_EVIDENCE/capturable/`.

## Gotchas

- `docs/DEVELOPMENT.md`: `AI_BUDDY_CAPTURABLE=1` forces visible; `=0` forces hide. An older comment in `scripts/verify-overlay.sh` that says `=1` excludes is stale — trust DEVELOPMENT.md and `dev_flags::CAPTURABLE`.
- Linux overlay does not implement macOS `NSWindowSharingNone` / Windows `WDA_EXCLUDEFROMCAPTURE`. Do not fail a Linux run for missing capture exclusion; document the platform gap.
- Env overrides Settings for that process; a Settings toggle will not win while the env is set.
- Screenshot tools that capture the compositor differently may still omit or include the sprite — prefer the platform property checks the verify scripts already encode.
