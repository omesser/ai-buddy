# Event-driven pointer input versus the 16 ms poll

Research for #183, prompted by #236 (fixes #182). Question: can the shell stop
polling the mouse button and react to input events instead, within DESIGN.md
decision 9 (no TCC prompt for the spatial layer)?

**Answer.** Partly, and not the way #183's comment assumes. X11 raw events are
event-driven and permission-free; Wayland has no global pointer at all, so
its degradation is webview-only, not a slow poll; on macOS Apple documents the
Input Monitoring gate for keyboard events and says nothing either way about
mouse-only listen taps, so the premise of #183 is unverified until a spike
tests it against a fresh TCC database. Even with events, the "cursor arrived
over the art" transition and the click-through flip cost the same hit-test
per motion event as they do per tick; the only genuine saving is the idle
case (still cursor, still sprite). #236 is correct today and stays correct
under any event design: draining a channel once per tick and setting an edge
bit are the same thing, and the `Witness` is the one-bit form of that queue.
Merge #236. Re-scope #183 into a permission spike before any loop rewrite.

## macOS

**`CGEventTapCreate`.** Apple's discussion gates only keys: "Event taps
receive key up and key down events if one of the following conditions is
true: The current process is running as the root user. Access for assistive
devices is enabled." Per-type masking: "If the event tap is not permitted to
monitor one or more of the events specified in the eventsOfInterest parameter,
then the appropriate bits in the mask are cleared. If that action results in
an empty mask, this function returns NULL." HID-level taps are root-only:
"Only processes running as the root user may locate an event tap at the point
where HID events enter the window server." The callback "is invoked from the
run loop to which the event tap is added as a source" — any thread with a
`CFRunLoop`, not necessarily main.
<https://developer.apple.com/documentation/coregraphics/cgevent/tapcreate(tap:place:options:eventsofinterest:callback:userinfo:)>

**Listen-only versus active.** `listenOnly` "Specifies that a new event tap
is a passive listener"; a passive listener "receives events but cannot modify
or divert them."
<https://developer.apple.com/documentation/coregraphics/cgeventtapoptions>

**Input Monitoring.** WWDC 2019 session 701 is Apple's only statement of the
rule: "macOS Catalina now requires user consent for apps to record the
contents of your screen or the keys that you type on your keyboard." The
keyboard example: "the CGEventTapCreate will fail and return nil. Meanwhile,
a dialog is displayed"; switching `listenOnly` to `defaultTap` makes it a
modifying tap, "where a listen-only event requires authorization" for Input
Monitoring and a modifying tap requires Accessibility. Mouse events are not
mentioned. <https://developer.apple.com/videos/play/wwdc2019/701/>
Apple DTS, asked what triggers the alert: "various APIs that are capable of
'seeing' input events that occur outside of your app" — no event-type
distinction. <https://developer.apple.com/forums/thread/676422>
`CGPreflightListenEventAccess` / `CGRequestListenEventAccess` (macOS 10.15+)
exist to test and request that grant; their pages carry no prose.
<https://developer.apple.com/documentation/coregraphics/cgpreflightlisteneventaccess()>

**Inference from public sources (2026-09):** a `kCGSessionEventTap` +
`kCGEventTapOptionListenOnly` tap whose mask holds only mouse types appears to
require Input Monitoring permission on current macOS. Multiple 2026 public
sources report this behavior: `IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)`
checks the permission (Stack Overflow #79010369), keytap 0.4.0 proactively
checks and fails with `PermissionDenied` when access is not granted,
tauri-plugin-macos-input-monitor documents the requirement and notes that
`CGEventTapCreate` may return non-NULL even without permission but the tap is
disabled. Chromium's remoting host ships exactly that shape
(`kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionListenOnly, 1 <<
kCGEventMouseMoved`) but asks for Accessibility, proving nothing about prompts.
**Stage 1 live Mac spike remains open**: the four observations #183 Stage 1
requests (TCC reset, tap creation, prompt appearance, preflight check) have not
been run on real hardware. If Input Monitoring is required, that violates
DESIGN.md decision 9 (no TCC prompt for spatial layer), so #183 Stage 2b (idle
back-off) is the provisional v1 answer for macOS pending spike or waiver.

**`NSEvent.addGlobalMonitorForEvents`.** "Key-related events may only be
monitored if accessibility is enabled or if your application is trusted for
accessibility access." And: "your handler will not be called for events that
are sent to your own application." The second sentence disqualifies it as a
single witness: a click over the art is delivered to our window (click-through
off), so the global monitor never sees it. A session tap does.
<https://developer.apple.com/documentation/appkit/nsevent/addglobalmonitorforevents(matching:handler:)>

**What the loop polls now.** `CGEventSource.buttonState` "Returns a Boolean
value indicating the current button state of a Quartz event source"; no
permission, latency or reliability caveat is documented.
<https://developer.apple.com/documentation/coregraphics/cgeventsource/buttonstate(_:button:)>
`combinedSessionState` "reflects the combined state of all event sources
posting to the current user login session."
<https://developer.apple.com/documentation/coregraphics/cgeventsourcestateid/combinedsessionstate>
`NSEvent.mouseLocation` "returns the location regardless of the current event
or pending events."
<https://developer.apple.com/documentation/appkit/nsevent/mouselocation>

**Click-through windows.** `ignoresMouseEvents` makes the window "transparent
to mouse events."
<https://developer.apple.com/documentation/appkit/nswindow/ignoresmouseevents>
tao implements `set_ignore_cursor_events` as `setIgnoresMouseEvents` dispatched
asynchronously to the main queue ("`setIgnoresMouseEvents_:` isn't thread-safe,
and fails silently"), so the flip lands after the call returns.
<https://github.com/tauri-apps/tao/blob/dev/src/platform_impl/macos/util/async.rs>
**Unverified:** whether an `NSTrackingArea` fires on a window that ignores
mouse events. Apple documents tracking areas as view-owned and driven by
the pointer being "over that region"; nothing addresses transparent windows.
<https://developer.apple.com/documentation/appkit/nstrackingarea>
Treat "the cursor came back over the art" as not event-driven on macOS unless
a mouse-moved tap is available and permission-free.

## Linux

**X11.** XI2: "RawEvents are sent exclusively to all root windows. ... Clients
supporting XI 2.1 or later receive raw events at all times, even when the
device is grabbed by another client." `XISelectEvents` on the root window has
no privilege check. Event-driven, prompt-free, confirmed.
<https://gitlab.freedesktop.org/xorg/proto/xorgproto/-/blob/master/specs/XI2proto.txt>
x11rb ships it behind `xinput = ["x11rb-protocol/xinput", "xfixes"]`; the
repo enables `randr, xfixes, shape, dpms, screensaver` but not `xinput`
(`src-tauri/Cargo.toml`).
<https://github.com/psychon/x11rb/blob/master/x11rb/Cargo.toml>
As of #562, the `xinput` feature is enabled and the frame loop uses XI2 raw
events to avoid polling when idle on X11.

**Wayland.** `wl_pointer.enter` is "Notification that this seat's pointer is
focused on a certain surface"; `motion` coordinates are "relative to the
focused surface." There is no global pointer, and `XQueryPointer` needs an X
connection, so today Wayland has only the webview latch
(`platform.rs`: "Wayland has only the overlay latch"). The degradation is not
a slow poll; it is no out-of-window witness at all. Wayland remains
webview-only as documented here and is not slowed to a poll rate (no global
pointer to poll). #183, #562.
<https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_pointer>
tao's GTK `CursorIgnoreEvents` sets a 1×1 input shape when ignoring and clears
it otherwise; the repo already carves per-pixel input on X11 (#191).
<https://github.com/tauri-apps/tao/blob/dev/src/platform_impl/linux/event_loop.rs>

## Tauri

`WebviewWindow::set_ignore_cursor_events` is documented only as "Ignores the
window cursor events"; tao adds that the events "are passed through the window
such that any other window behind it receives them."
<https://github.com/tauri-apps/tao/blob/dev/src/window.rs>
No first-party plugin exposes pointer events; `global-shortcut` is keyboard.
<https://github.com/tauri-apps/plugins-workspace/blob/v2/README.md>

## What events cannot give you

The hit-test that decides click-through runs on every cursor position. With a
mouse-moved tap it runs per motion event (60–125 Hz while moving) instead of
per tick; without one it stays a poll and the loop cannot block on `recv()`.
Grab/Throw/fall need `elapsed_ms` anyway. The saving is bounded to the idle
case: cursor still, sprite still — from 60 wakeups/s (cursor read, N hit-tests,
two button reads, Engine tick, frame emit) to zero. Director timers and
activity sensing still wake.

## #236 under either design

`Pointer::update` derives the press edge from `held && !was_held`
(`crates/core/src/input.rs`). Any event producer drained once per tick must
carry a sub-tick down+up across the drain; a two-element queue and an edge
bit are equivalent for that. `Witness::take` is the edge bit. If a session
tap replaces the webview witness, the producer changes and `take` stays.
#236 is the minimal correct fix today and is kept, not replaced, under #183.

## Implementation (as of #562, #183)

**X11** (#562): XI2 raw events + two-clock scheduler. No poll when idle. Event-
driven and permission-free.

**Wayland**: Webview-only. No global pointer exists, so no poll and no events.

**macOS** (#183 Stage 2b): Idle back-off. Mouse-only listen taps appear to
require Input Monitoring (inferred from public sources 2026-09, live spike open
unless waived), which would violate decision 9. The frame loop polls
`CGEventSourceButtonState` and `NSEvent.mouseLocation` but backs off to the next
real deadline (Director wakes, sense interval) when idle. Hidden idle: uncapped
deep sleep (same as X11). Visible idle: capped at 1s for timely gesture/menu.
Active mode (Grab/Throw/fall/animation) keeps 16ms ticks.

**Windows**: Same idle back-off as macOS (no Windows-specific input events in
v1 per SPEC.md).

#236 merged (edge-detection witness for sub-tick clicks). The two-witness OR
stays: full-display overlay + per-window click-through + no Input Monitoring
tap is the v1 design.
