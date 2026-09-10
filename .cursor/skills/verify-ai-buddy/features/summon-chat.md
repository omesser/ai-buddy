# Summon / chat

Double-clicking the buddy opens its Chat surface: the same conversation that drives desktop Behavior, so answers show as bubble speech and as a Behavior, not only as text.

## Sub-features

- `summon-verb` double-click records `verbs:.*Summon` in the frame/trace log.
- `summon-chat-open` a Chat window for that buddy appears (when the session can show windows).
- `summon-status` the Chat status bar names Behavior / State when a Director path is active.

## How to get to it (user POV)

- Double-click the sprite body.
- Or choose the speech bubble's **Open chat** control when a bubble is showing.
- Tray / menu Chat entries stay disabled; Summon is the intended entry.

## Driving it with verify-overlay helpers

Preconditions:

- Overlay is running with `AI_BUDDY_TRACE_FRAMES=1` (and ideally `AI_BUDDY_TRACE_ENGINE=1`).
- A real interactive display for Chat window proof; Xvfb can still prove the Summon verb.
- Doctor green for the lane.

- **Summon verb (X11).** After overlay is up (or after a successful `drive-overlay-x11.sh` with `--keep`-style hold if you extend the helper), locate sprite `pos()` from the last `frame:` line, `xdotool mousemove --sync $X $Y`, then `xdotool click --repeat 2 --delay 50 1`. Assert `grep -E 'verbs:.*Summon' "$TRACE_LOG"`. Copy the matching lines into `$AI_BUDDY_VERIFY_EVIDENCE/summon-chat/`.
- **Chat window (interactive desktop).** After the double-click, observe a Chat window belonging to the buddy. Capture a screenshot with `AI_BUDDY_CAPTURABLE=1` into evidence when the platform allows.
- **Harness without sprite.** Chat Completer wiring without the overlay: `AI_BUDDY_HARNESS=<name> scripts/probe-harness.sh` (exit `0` = end_turn). This does **not** prove Summon UI; record it as harness-only if used.
- **Proof.** Require the Summon verb line for the gesture path. Treat Chat window visibility as a second observer when a GUI session exists.

## Gotchas

- Existing `verify-overlay*.` scripts prove Poke, not Summon — do not mark Summon verified solely because overlay-x11 passed.
- Double-click timing follows the OS interval; a slow second click becomes two Pokes.
- Chat needs the Shell's windowing path; a crashed WebKit / missing display shows the verb without a usable Chat surface — report both observations.
- On this project's Linux CI-style boxes, Summon UI may be unprovable while the verb remains provable under Xvfb+WM; say which half you proved.
