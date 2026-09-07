# GTK3 / gtk-rs Advisory Inventory

Supporting detail for ADR-0021 (accept GTK3 pin until Tauri moves).

## RUSTSEC Advisories

Eleven gtk-rs GTK3 crates marked unmaintained or unsound as of 2026-09-07:

- RUSTSEC-2024-0411: `gdkwayland-sys` (unmaintained)
- RUSTSEC-2024-0412: `gdk` (unmaintained)
- RUSTSEC-2024-0413: `atk` (unmaintained)
- RUSTSEC-2024-0414: `gdkx11-sys` (unmaintained)
- RUSTSEC-2024-0415: `gtk` (unmaintained)
- RUSTSEC-2024-0416: `atk-sys` (unmaintained)
- RUSTSEC-2024-0417: `gdkx11` (unmaintained)
- RUSTSEC-2024-0418: `gdk-sys` (unmaintained)
- RUSTSEC-2024-0419: `gtk3-macros` (unmaintained)
- RUSTSEC-2024-0420: `gtk-sys` (unmaintained)
- RUSTSEC-2024-0429: `glib` 0.18.5 (unsound `VariantStrIter`)

## Version Constraints

`gtk` is frozen at 0.18.2, which requires `glib ^0.18`. The patched `glib` 0.20
line is unreachable while the GTK3 lane holds. Issue #383 opened on the bump
and closed `NOT_PLANNED` for this reason.

## Historical Notes

Issue #397 raised all eleven advisories. This record accepts them together
because they are one pin: the crates reach `Cargo.lock` through Tauri's Linux
window and tray stack, and upgrading is a Tauri migration to GTK4, not a
version edit here.
