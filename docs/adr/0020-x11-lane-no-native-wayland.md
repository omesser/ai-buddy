# One Linux build takes the X11 lane; there is no native Wayland lane

## Context

Linux desktops split between X11 and Wayland session types. Most harnesses and
compositor features work differently — or not at all — depending on which
protocol the session speaks.

The spatial layer needs four things: other windows' geometry (for Perches),
the global pointer (for Grab and Throw), per-pixel input regions (for
click-through), and placement over the desktop (for the overlay). X11 provides
all four via EWMH atoms, `XShapeCombineMask`, `query_pointer`, and standard
window properties.

Native Wayland provides only one: `wl_surface.set_input_region` with
`wl_region.add` handles click-through. The other three have no solution:
- Other windows' geometry: no compositor reports it; protocols that could carry
  it decline to
- Global pointer: a Wayland client hears about the pointer only while it's over
  that client
- Placement over the desktop: `zwlr_layer_shell_v1` works but GNOME (Mutter)
  doesn't implement it

XWayland bridges the gap: GNOME and KDE sessions proxy EWMH, XShape, and
`query_pointer` for XWayland clients, so the X11 path works under Wayland when
an X server answers.

## Decision

One non-macOS arm ships behind `cfg(all(unix, not(target_os = "macos")))` and
never splits on session type. `platform::x11_answers` decides both capability
gates, so the lane turns on whether an X server answers — a real X11 session or
the XWayland that GNOME/KDE runs.

Where no X server answers, the spatial layer declares the capability absent and
the Character degrades. It does not switch protocols.

XWayland leaves one hole: it does not list native Wayland clients, so those
windows are not Perches. The cost is one missing Perch, same as an unmanaged
window under a real X server.

## When to reopen

Mutter shipping `zwlr_layer_shell_v1` (or equivalent) is the gate condition.
Even then, other windows' geometry and the global pointer would still be
missing, so a native lane built the day layer-shell lands still loses Perches,
fullscreen fade, Grab, and Throw.

## Consequences

Adding a Linux capability means one arm in `platform/x11/` with no second
implementation to mirror. The degraded path stays a supported mode rather than
an error.

The user can override with `GDK_BACKEND` to force the degraded lane on purpose.

Reversing this means writing a spatial layer against protocols that withhold
two of its inputs and accepting that it cannot place itself on GNOME.

## Supersedes

This decision supersedes the first half of
[ADR-0014](./0014-x11-lane-no-native-wayland.md), which combined the X11-only
lane with the GTK3 acceptance into one record.
