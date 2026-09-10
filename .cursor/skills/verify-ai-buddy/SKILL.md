---
name: verify-ai-buddy
description: "Drive the ai-buddy desktop mascot (Tauri overlay) the way a user does — launch, doctor, poke/perch/summon via existing verify-overlay* scripts, capture evidence. Use when proving overlay, gesture, or chat behavior for this repo."
---

# Verify ai-buddy

Project-local control skill for **ai-buddy**, a Tauri desktop mascot whose primary surface is a full-display overlay sprite (perch, poke, summon/chat). Agents read this cold mid-task: every command below is literal.

## Interview summary (do not re-derive)

| Axis | Finding |
|---|---|
| **Surface** | Desktop overlay mascot (macOS / Linux X11 / Windows). Secondary: native Settings window, Chat surface (Summon), tray menu. |
| **Run** | `cargo run -p ai-buddy` from repo root (or `target/release/ai-buddy` / `target/debug/ai-buddy` after build). Offline by default (Static Director). |
| **Drive** | Prefer existing scripts: `scripts/verify-overlay.sh` (macOS), `scripts/verify-overlay-x11.sh` (Linux X11 under a WM), `scripts/verify-overlay-win.ps1`, `scripts/verify-settings-win.ps1`, `scripts/probe-harness.sh`, `scripts/test_verify_overlay_diagnostics.sh`. Plus `cargo test` / `node --test tests/*.test.js`. |
| **Observe** | Overlay logs (`AI_BUDDY_TRACE_FRAMES=1`, `AI_BUDDY_TRACE_HITTEST=1`), `.verify/` stamp dirs, screenshots when capturable, exit codes, `verbs: …Poke` / `verbs: …Summon` lines. |
| **Isolate** | **Refuse double-drive on one display.** Two instances share the same window list / hit-test path (`AI_BUDDY_INSTANCES` is multi-buddy in *one* process, not two agents). Kill only the PID this run started. |

## Evidence location (survives cleanup)

```sh
export RUN_ID="${RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
export AI_BUDDY_VERIFY_ROOT="/tmp/ai-buddy-verify-$RUN_ID"
export AI_BUDDY_VERIFY_EVIDENCE="$AI_BUDDY_VERIFY_ROOT/evidence"
mkdir -p "$AI_BUDDY_VERIFY_EVIDENCE"
```

Cleanup removes processes and scratch under `$AI_BUDDY_VERIFY_ROOT/scratch` only. **Never delete `$AI_BUDDY_VERIFY_EVIDENCE`.** Name that path in every proof report.

## Launch

From the repo root:

```sh
# One-time build (release preferred for X11 verify)
cargo build -p ai-buddy --release

# Helpers set RUN_ID / evidence / scratch and start a traced instance when a
# platform script does not already own the lifecycle:
.cursor/skills/verify-ai-buddy/helpers/launch.sh
```

Ready signals (any one is enough for doctor):

- Log line matching `^overlay:`
- On X11: an `Ai-buddy` class window ≥200×200 (GDK leaves a 10×10 placeholder — ignore it)
- Process still alive (`kill -0 $APP_PID`)

Env that unattended runs usually want:

```sh
export AI_BUDDY_TRACE_FRAMES=1
export AI_BUDDY_TRACE_HITTEST=1
export AI_BUDDY_CAPTURABLE=1   # force visible in captures (default is already capturable; =0 forces hide)
# Optional: AI_BUDDY_DIRECTOR_API_KEY=… to skip Keychain prompts on macOS
```

`scripts/verify-overlay.sh` and `scripts/verify-overlay-x11.sh` launch (and tear down) the app themselves — use those for overlay/poke proofs rather than a parallel launcher.

## Doctor

Read-only health check. Run before Drive whenever anything looks off:

```sh
.cursor/skills/verify-ai-buddy/helpers/doctor.sh
```

Doctor answers:

1. Repo root looks like ai-buddy (`Cargo.toml` workspace + `src-tauri/`).
2. Binary exists (`target/release/ai-buddy` or `target/debug/ai-buddy`) or `cargo` can build.
3. Platform tools for the active lane are on `PATH` (macOS: `swift`; Linux X11: `xdotool` `xprop` `xwininfo` `xterm` + `DISPLAY` + supporting WM / `openbox`; Windows: PowerShell + dual display for overlay-win).
4. If `$APP_PID` is set, that process is alive and its log (if any) contains `^overlay:`.
5. Unit lanes green when asked: `cargo test -p ai-buddy-core` and `node --test tests/*.test.js` (see Helpers).

Exit `0` = worth driving. Non-zero = fix Launch before Drive.

## Drive

Map lives in [`features/`](features/README.md). Prefer one feature per proof run.

| Lane | Command |
|---|---|
| Linux X11 overlay + perch/ride/drop + poke | `xvfb-run -a -s "-screen 0 1280x720x24" .cursor/skills/verify-ai-buddy/helpers/drive-overlay-x11.sh` (needs `openbox` + `xterm` on bare Xvfb; optional `AI_BUDDY_VERIFY_PREFIX=/path/to/extracted` for deb-extracted libs/themes) |
| macOS overlay + physics + hit-test | `.cursor/skills/verify-ai-buddy/helpers/drive-overlay-macos.sh` |
| Windows overlay | `.cursor/skills/verify-ai-buddy/helpers/drive-overlay-win.ps1` |
| Windows Settings | `scripts/verify-settings-win.ps1` (copy `$Out` into evidence after) |
| Harness ACP (no sprite) | `AI_BUDDY_HARNESS=hermes scripts/probe-harness.sh` |
| macOS Keychain diagnostic unit | `scripts/test_verify_overlay_diagnostics.sh` |
| Core + renderer units | `.cursor/skills/verify-ai-buddy/helpers/doctor.sh --units` |

Stable handles: log patterns (`frame: N Perched`, `verbs:.*Poke`, `verbs:.*Summon`, `EWMH configured`), X11 WM_CLASS `Ai-buddy`, EWMH `_NET_WM_STATE_ABOVE` + `_NET_WM_STATE_SKIP_TASKBAR`. Prefer those over click coordinates when asserting.

## Evidence

For every Drive:

1. Exercise the real user path (click / perch window / double-click), not internal setters.
2. Capture action **and** resulting state (log excerpts before/after, exit code, optional screenshot).
3. Copy platform script artifacts into `$AI_BUDDY_VERIFY_EVIDENCE/<feature-id>/` (helpers do this).
4. Record `RUN_ID`, feature ID, entry point, and commands in `$AI_BUDDY_VERIFY_EVIDENCE/PROOF.md`.

Proof standards:

- Overlay presence: overlay window + EWMH states + `^overlay:` log.
- Poke: `verbs:.*Poke` after a real click on the sprite body.
- Summon: `verbs:.*Summon` after double-click; Chat window appears when a display session can show it.
- Capturable override: env `AI_BUDDY_CAPTURABLE=0|1` observed in settings/platform behavior (macOS sharing type / Windows WDA); Linux has no capture-exclusion API equivalent — document as Gotcha.

## Cleanup

```sh
.cursor/skills/verify-ai-buddy/helpers/cleanup.sh
```

Rules:

- Kill **only** PIDs recorded under `$AI_BUDDY_VERIFY_ROOT/scratch/pids` (and children of those). Never `pkill -f ai-buddy` by name alone when a user's own instance may be running.
- Remove `$AI_BUDDY_VERIFY_ROOT/scratch`.
- **Keep** `$AI_BUDDY_VERIFY_EVIDENCE` intact.
- Platform scripts that trap their own EXIT already tear down the app they started; still run cleanup to clear helper scratch.

After cleanup, confirm:

```sh
test -d "$AI_BUDDY_VERIFY_EVIDENCE" && ls -la "$AI_BUDDY_VERIFY_EVIDENCE"
```

## Helpers

All under `.cursor/skills/verify-ai-buddy/helpers/` (executable):

| Script | Invocation | Role |
|---|---|---|
| `common.sh` | sourced by others | `RUN_ID`, evidence/scratch paths, repo root |
| `doctor.sh` | `…/doctor.sh` [`--units`] | Launch readiness + optional unit suites |
| `launch.sh` | `…/launch.sh` | Build if needed; start traced app into scratch log when no drive script owns lifecycle |
| `cleanup.sh` | `…/cleanup.sh` | Tear down helper-owned PIDs; preserve evidence |
| `drive-overlay-x11.sh` | `…/drive-overlay-x11.sh` | Wraps `scripts/verify-overlay-x11.sh`; copies `.verify/x11-*` → evidence |
| `drive-overlay-macos.sh` | `…/drive-overlay-macos.sh` | Wraps `scripts/verify-overlay.sh`; copies `.verify/<stamp>` → evidence |
| `drive-overlay-win.ps1` | `…/drive-overlay-win.ps1` | Wraps `scripts/verify-overlay-win.ps1`; copies `.verify/win-*` → evidence |
| `prove-units.sh` | `…/prove-units.sh` | `cargo test -p ai-buddy-core` + `node --test` + diagnostics script; writes evidence |
| `xterm-shim.sh` | used automatically by `drive-overlay-x11.sh` | Translates `xterm -geometry/-title/-e` to `xfce4-terminal` when real xterm is absent |

Example end-to-end (Linux box with X11 deps):

```sh
export RUN_ID=$(date +%Y%m%d-%H%M%S)-$$
.cursor/skills/verify-ai-buddy/helpers/doctor.sh --units
xvfb-run -a -s "-screen 0 1280x720x24" \
  .cursor/skills/verify-ai-buddy/helpers/drive-overlay-x11.sh
.cursor/skills/verify-ai-buddy/helpers/cleanup.sh
ls "$AI_BUDDY_VERIFY_EVIDENCE"
```

When GUI/overlay cannot run (headless without `openbox`/`xterm`, Wayland-only with no Perches, no display): prove the runnable subset with `doctor.sh --units` / `prove-units.sh`, write the exact GUI gap into `$AI_BUDDY_VERIFY_EVIDENCE/PROOF.md` and the feature Gotchas — do not leave a manual chore list for the human.

## Maintenance

Keep the feature map honest with `/maintain-verification-skill` as the app changes.
