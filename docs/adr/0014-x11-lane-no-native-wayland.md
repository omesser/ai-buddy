# One Linux build takes the X11 lane wherever an X server answers; there is no native Wayland lane

One non-macOS arm ships, behind `cfg(all(unix, not(target_os = "macos")))`, and
it never splits on session type. `platform::x11_answers` decides both capability
gates and `platform::prefer_x11_backend` points GDK at its X11 backend, so the
lane turns on whether an X server answers this process — a real X11 session, or
the XWayland that a GNOME or KDE session runs. Where none answers, the spatial
layer declares the capability absent and the Character degrades. It does not
switch protocols. Decision 3 in [DESIGN.md](../../DESIGN.md) holds the
per-limit detail; this records the choice behind it.

The X11 lane is not a shim over one call. `platform/x11/` reads window geometry
from EWMH atoms, carves the per-pixel input region with `XShapeCombineMask`,
tracks the global pointer with `query_pointer`, and reads idle from the
Screensaver extension and display sleep from DPMS. A native Wayland lane owes
the same four answers, and three of them have none:

- **Other windows' geometry.** No compositor reports it, and the protocols that
  could carry it decline to. `ext-foreign-toplevel-list-v1` names a toplevel and
  gives no rectangle. `wlr-foreign-toplevel-management`'s `set_rectangle` is a
  hint the client sends for a minimize animation, not a query. GNOME's private
  `org.gnome.Shell.Introspect.GetWindows` returns width and height with no x or
  y, and allowlists the two portal backends as senders.
- **The pointer outside our own surface.** A Wayland client hears about the
  pointer while it is over that client. Nothing carries it seat-wide, so Grab
  and Throw see only the clicks the webview witnesses.
- **Placement over the desktop.** `zwlr_layer_shell_v1` anchors a surface above
  the desktop, which is the whole of what the overlay wants, and KWin, Sway,
  Hyprland, niri, river, COSMIC and Mir implement it. Mutter does not, so the
  largest Linux desktop cannot be served that way.

Per-pixel click-through is the exception, and it argues against a second lane
rather than for one. `wl_surface.set_input_region` with a `wl_region` is core
Wayland, `wl_region.add` takes the same rectangles `XShapeCombineMask` takes,
and tao hands us the `wl_surface` through `raw_window_handle`. The gap is the
`_ =>` arm in `platform/x11/overlay.rs`, which drops that handle. #267 opened on
the opposite claim and its own body named an upstream limit; a reviewer reading
tao's locked source found the surface already exposed and corrected the record.
The one capability a native lane would buy back is reachable by wiring a handle
we already receive.

## The XWayland lane

Both gates used to read `WAYLAND_DISPLAY`, which every Wayland session sets for
its XWayland clients too. #266 found that this degraded GNOME and KDE users
without ever asking whether the X11 path would have worked; under XWayland it
does, because Mutter and KWin proxy the EWMH states, the XShape input region and
`query_pointer` the app asks for. #269 replaced the environment test with the
capability question. `prefer_x11_backend` stays conditional on the same answer,
because GTK aborts on a backend it cannot open and an unconditional
`GDK_BACKEND=x11` would trade a degraded Character for one that never starts. A
`GDK_BACKEND` the user names wins, which is how someone asks for the degraded
lane on purpose.

XWayland leaves one hole: it does not list native Wayland clients, so those
windows are not Perches. `window_geometry` still declares `true`, because the
rectangles handed over are real geometry and a window missing from the list
costs one Perch — the same shortfall an unmanaged window already causes under a
real X server.

## Considered Options

- **Gate the lane on the session type.** What shipped before #269, and it read
  the environment instead of the capability. Every GNOME and KDE user lost
  Perches, Poke, Grab and Throw to a variable that says nothing about whether
  the X11 path works.
- **A native Wayland lane beside the X11 one.** Two spatial implementations to
  keep in step, for a lane that can offer neither window geometry nor the global
  pointer on any compositor, and cannot place the overlay on GNOME. It would buy
  back click-through, which the X11 module can wire without it.
- **Wayland only, and drop X11.** Cleaner on paper and gives up the only lane
  that answers all four questions today, on the platform where XWayland means
  almost every session can take it.

## When to reopen

Mutter shipping `zwlr_layer_shell_v1`, or an equivalent placement protocol
reaching GNOME, is the condition. Until then a native lane cannot put the
overlay where it belongs on the desktop most Linux users run.

That condition alone is not sufficient. Other windows' geometry and the pointer
outside our own surface would still be missing, so a native lane built the day
layer-shell lands still loses Perches, fullscreen fade, Grab and Throw. It would
be a placement fix, not a spatial layer.

## Consequences

Adding a Linux capability means one arm, in `platform/x11/`, with no second
implementation to mirror. The degraded path stays a supported mode rather than
an error, which is what lets the lane gate answer honestly instead of guessing.

The Linux behavior nobody has run is what this rests on. #269 asked for a manual
pass on a GNOME Wayland desktop and on KDE, to confirm XWayland serves
`query_pointer` and `screensaver::query_info` seat-wide rather than per-client.
#266's closing comment records a live check on a Linux agent box: pure X11 took
the X11 lane, and with a synthetic `WAYLAND_DISPLAY` the new code still climbed
by window geometry where the pre-#269 parent degraded. A native Wayland session
without an X server was not available there, so the seat-wide question is
answered for a faked session and open for a real one.

Reversing this means writing a spatial layer against protocols that withhold two
of its inputs, and accepting that it cannot place itself on GNOME.
