# One Linux build takes the X11 lane; there is no native Wayland lane

## Context

Linux desktops split between X11 and Wayland session types. The spatial layer
needs four things: other windows' geometry (for Perches), the global pointer
(for Grab and Throw), per-pixel input regions (for click-through), and
placement over the desktop (for the overlay).

X11 provides all four. Native Wayland provides only click-through; the other
three have no solution in the protocol. Protocol gap detail is in
`docs/research/wayland-protocols.md`.

XWayland bridges the gap: GNOME and KDE sessions proxy the X11 capabilities for
XWayland clients, so the X11 path works under Wayland when an X server answers.

## Decision

One Linux (non-macOS) build ships and takes the X11 lane when an X server
answers — a real X11 session or the XWayland that GNOME/KDE runs.

Where no X server answers, the spatial layer declares the capability absent and
the Character degrades. It does not switch protocols.

XWayland leaves one hole: it does not list native Wayland clients, so those
windows are not Perches. The cost is one missing Perch, same as an unmanaged
window under a real X server.

## When to reopen

Mutter shipping layer-shell (or equivalent) is the gate condition. Even then,
other windows' geometry and the global pointer would still be missing, so a
native lane built the day layer-shell lands still loses Perches, fullscreen
fade, Grab, and Throw.

## Consequences

Adding a Linux capability means one implementation with no second lane to
mirror. The degraded path stays a supported mode rather than an error.

Reversing this means writing a spatial layer against protocols that withhold
two of its inputs and accepting that it cannot place itself on GNOME.

## Supersedes

This decision supersedes the first half of
[ADR-0014](./0014-x11-lane-no-native-wayland.md), which combined the X11-only
lane with the GTK3 acceptance into one record.
