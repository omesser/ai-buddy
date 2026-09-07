# Accept the GTK3 / gtk-rs pin until Tauri moves

## Context

Eleven gtk-rs GTK3 crates in `Cargo.lock` are either unmaintained or carry an
unsoundness advisory. They reach the tree through Tauri's Linux window and tray
stack, not through anything this repository directly asks for.

gtk-rs migrated to GTK4, which is a different C library. Tauri v2 builds its
Linux surface on GTK3 via webkit2gtk. Upgrading is a Tauri migration, not a
version bump here. Pinning or vendoring buys nothing: the crates are current,
not stale — absent maintenance is the whole finding.

## Decision

The Linux lane accepts Tauri's GTK3 and gtk-rs pin until Tauri ships a GTK4
backend or a RustSec advisory names a vulnerability rather than absent
maintenance.

The exposure is what the X11 lane already is: GTK3 draws the Linux Settings
window and the tray, both of which read input from this process. An
unmaintained binding matters when a vulnerability is found and nobody ships the
fix, so the cost is a longer wait on that day, not a hole open today.

The repository owner revisits this at each `/security-review` run of the Rust
lane, which is where the cluster surfaces.

## Consequences

The cluster stays in `Cargo.lock` as a known accepted risk. Two things end the
acceptance: a RustSec advisory naming a vulnerability, or Tauri shipping a GTK4
Linux backend.

Advisory inventory is in `docs/research/gtk3-pin.md`.

## Supersedes

This decision supersedes the second half of
[ADR-0014](./0014-x11-lane-no-native-wayland.md), which combined the GTK3
acceptance with the X11-only lane into one record.
