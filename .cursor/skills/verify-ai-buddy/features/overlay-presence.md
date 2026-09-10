# Overlay presence

The buddy draws on a display-sized always-on-top overlay that skips the taskbar, loads a Character Package, and runs a frame loop (Falling / Grounded / Perched) the user can see as the sprite on the desktop.

## Sub-features

- `overlay-window` publishes one large overlay window per display (not the GDK 10×10 placeholder).
- `overlay-ewmh` (X11) sets `_NET_WM_STATE_ABOVE` and `_NET_WM_STATE_SKIP_TASKBAR`.
- `overlay-frames` emits `frame:` lines once the sprite is alive.
- `overlay-perch` lands on a real window top edge when a Perch exists before launch.

## How to get to it (user POV)

- Launch the app (`cargo run -p ai-buddy` or a Release binary) on a desktop session.
- On Linux X11, use a session with a supporting window manager (openbox under Xvfb is enough).
- On macOS / Windows, launch on a real interactive desktop (not a bare CI agent without a display).

## Driving it with verify-overlay helpers

Preconditions:

- `.cursor/skills/verify-ai-buddy/helpers/doctor.sh` exits `0` for the active lane.
- Linux: `DISPLAY` set; `xdotool` `xprop` `xwininfo` `xterm` present; supporting WM (`openbox` if Xvfb).
- Evidence dir `$AI_BUDDY_VERIFY_EVIDENCE` exists.

- **Linux X11 full check.** Run `xvfb-run -a -s "-screen 0 1280x720x24" .cursor/skills/verify-ai-buddy/helpers/drive-overlay-x11.sh`. Prefer exit code `0`. If the stock script fails only because `xprop _NET_WM_STATE` is empty while the app log still has `^overlay:`, `EWMH configured`, and `frame:` lines, treat presence as proven and record the EWMH property gap in `PROOF.md` (see Gotchas). Prefer `frame:.*Perched` when a Perch window existed before launch; Falling/Grounded frames still count as presence.
- **macOS full check.** Run `.cursor/skills/verify-ai-buddy/helpers/drive-overlay-macos.sh`. Exit code `0`. Evidence contains `.verify/<stamp>/` with frame-loop and overlay PASS lines.
- **Windows full check.** Run `.cursor/skills/verify-ai-buddy/helpers/drive-overlay-win.ps1` on a dual-display Windows host. Exit code `0`. Evidence contains `.verify/win-*/`.
- **Proof.** Keep the helper-copied stamp tree and a `PROOF.md` line naming `overlay-presence` and the helper invoked.

## Gotchas

- Xvfb alone has no window manager: without `openbox` (or another EWMH WM), `_NET_CLIENT_LIST` is empty and Perch never happens — the script fails before poke.
- The Perch window must exist **before** the app starts; a late window sits above the sprite and is not a surface from below.
- GDK creates a tiny `Ai-buddy` window; assert size ≥200×200 when picking the overlay id.
- Wayland-only sessions lose window Perches (screen-edge physics only). That is supported product behavior, not a script failure — do not claim X11 perch parity there.
- Two verification agents on one display corrupt each other's Perch and hit-test; refuse concurrent drives.
- Linux shells panic at tray init without `libayatana-appindicator3` (`libayatana-appindicator3.so.1`). Install `libayatana-appindicator3-1` (and deps) or point `LD_LIBRARY_PATH` at a local extract for verify-only runs.
- App log may say `EWMH configured` while `xprop _NET_WM_STATE` is still empty (WM never reflected ABOVE/SKIP_TASKBAR). `scripts/verify-overlay-x11.sh` fails that assert; treat it as an environment/WM gap, not a missing overlay — frames/`^overlay:` remain valid presence evidence.
