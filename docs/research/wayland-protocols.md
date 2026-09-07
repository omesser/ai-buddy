# Wayland Protocol Gaps for Desktop Mascot Spatial Layer

Research extracted from ADR-0014 supersession (2026-09-07).

## Context

This document records why a native Wayland lane cannot provide the four
capabilities the spatial layer needs: other windows' geometry, the global
pointer, per-pixel input regions, and placement over the desktop.

## Protocol Gaps

### Other windows' geometry

No compositor reports it, and the protocols that could carry it decline to:

- `ext-foreign-toplevel-list-v1` names a toplevel and gives no rectangle
- `wlr-foreign-toplevel-management`'s `set_rectangle` is a hint the client
  sends for a minimize animation, not a query
- GNOME's private `org.gnome.Shell.Introspect.GetWindows` returns width and
  height with no x or y, and allowlists the two portal backends as senders

### The pointer outside our own surface

A Wayland client hears about the pointer while it is over that client. Nothing
carries it seat-wide, so Grab and Throw would see only the clicks the webview
witnesses.

### Placement over the desktop

`zwlr_layer_shell_v1` anchors a surface above the desktop, which is the whole
of what the overlay wants. KWin, Sway, Hyprland, niri, river, COSMIC, and Mir
implement it.

**Mutter does not**, so the largest Linux desktop cannot be served that way.

### Per-pixel click-through

This one works. `wl_surface.set_input_region` with a `wl_region` is core
Wayland. `wl_region.add` takes the same rectangles `XShapeCombineMask` takes,
and tao hands us the `wl_surface` through `raw_window_handle`.

The gap is the `_ =>` arm in `platform/x11/overlay.rs`, which drops that
handle. Issue #267 opened on the opposite claim; a reviewer reading tao's
source found the surface already exposed and corrected the record. The one
capability a native lane would buy back is reachable by wiring a handle we
already receive.

## XWayland Bridge

XWayland answers the question differently: GNOME and KDE proxy EWMH atoms,
XShape, and `query_pointer` for XWayland clients, so the X11 path works under
Wayland when an X server answers.

One hole remains: XWayland does not list native Wayland clients, so those
windows are not Perches.

## GDK_BACKEND Forensics

`prefer_x11_backend` is conditional on `x11_answers` because GTK aborts on a
backend it cannot open. An unconditional `GDK_BACKEND=x11` would trade a
degraded Character for one that never starts.

A `GDK_BACKEND` the user names wins, which is how someone asks for the degraded
lane on purpose.

## Verification Notes

Issue #269 asked for a manual pass on GNOME Wayland and KDE to confirm
XWayland serves `query_pointer` and `screensaver::query_info` seat-wide rather
than per-client.

Issue #266's closing comment records a live check on a Linux agent box: pure
X11 took the X11 lane, and with a synthetic `WAYLAND_DISPLAY` the new code
still climbed by window geometry where the pre-#269 parent degraded.

A native Wayland session without an X server was not available for testing, so
the seat-wide question is answered for a faked session and open for a real one.

## When Native Wayland Becomes Viable

Mutter shipping `zwlr_layer_shell_v1` (or equivalent) is the first gate. Even
then, other windows' geometry and the global pointer would still be missing.

A native lane built the day layer-shell lands would place correctly but still
lose Perches, fullscreen fade, Grab, and Throw.
