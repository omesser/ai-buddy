# Accept the GTK3 / gtk-rs pin until Tauri moves

## Context

RUSTSEC-2024-0411 through RUSTSEC-2024-0420 mark ten gtk-rs GTK3 crates
unmaintained. RUSTSEC-2024-0429 is the eleventh: `glib` 0.18.5 has an unsound
`VariantStrIter`. None of the first ten name a vulnerability; each says the
crate has no maintainer.

These crates reach `Cargo.lock` through Tauri's Linux window and tray stack,
not through anything this repository directly asks for. `gtk` is frozen at
0.18.2, which requires `glib ^0.18`, so the patched `glib` 0.20 line is
unreachable while the GTK3 lane holds.

gtk-rs migrated to GTK4, which is a different C library. Tauri v2 builds its
Linux surface on GTK3 via webkit2gtk. Upgrading is a Tauri migration, not a
version bump here. Pinning or vendoring buys nothing: these crates are current,
not stale — unowned is the whole finding.

## Decision

The Linux lane accepts all eleven crates at their current versions until Tauri
ships a GTK4 backend or a RustSec advisory names a vulnerability rather than
absent maintenance.

The exposure is what the X11 lane already is: GTK3 draws the Linux Settings
window and the tray, both of which read input from this process. An
unmaintained binding matters when a vulnerability is found and nobody ships the
fix, so the cost is a longer wait on that day, not a hole open today.

The repository owner revisits this at each `/security-review` run of the Rust
lane, which is where the cluster surfaces.

## Consequences

The cluster stays in `Cargo.lock` as a known accepted risk. Two things end the
acceptance:
1. A RustSec advisory naming a vulnerability moves this to a fix, on whatever
   path exists at the time
2. Tauri shipping a GTK4 Linux backend removes the cluster and unpins `glib`

## Supersedes

This decision supersedes the second half of
[ADR-0014](./0014-x11-lane-no-native-wayland.md), which combined the GTK3
acceptance with the X11-only lane into one record.
