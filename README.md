# ai-buddy

A desktop companion in the spirit of Windows 95-era desktop mascots: an animated
sprite that lives on your screen, reacts to the windows around it, and can be
asked to do real work on your machine.

Vocabulary is defined in [CONTEXT.md](./CONTEXT.md), the design in
[DESIGN.md](./DESIGN.md), the v1 scope in [docs/SPEC.md](./docs/SPEC.md), and the
decisions that carry lock-in in [docs/adr/](./docs/adr/).

## State

Early. Work is tracked as [GitHub issues](https://github.com/omesser/ai-buddy/issues);
issue #1 (the transparent overlay) is the only one built. There is no physics, no
Character Package, no Director, and no Functional Layer yet.

## Running it

macOS only for now. Windows is stubbed deliberately — see
[docs/SPEC.md](./docs/SPEC.md).

```sh
cd src-tauri
cargo run
```

No Node toolchain and no bundler: the front end is static files under `src/`,
which Tauri embeds at build time.

## Verifying the overlay

Most of what this feature does is invisible. Nothing on screen says whether the
overlay is currently swallowing clicks or passing them on, so verification is
split in three.

**Unit tests** cover the arithmetic — the alpha lookup and the coordinate
conversions. Fast, pure, no windowing system:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
```

**`scripts/verify-overlay.sh`** covers everything else a machine can reach. It
is deliberately not a `cargo test`: it needs a real desktop, a real window
server and a running app, so it is slow, macOS-only, and cannot run in CI. Run
it when the overlay or the platform layer changes.

```sh
scripts/verify-overlay.sh          # or --keep to leave the app running
```

It checks that exactly one overlay window exists at floating level, that its
bounds match the union of the displays, that the app is an accessory with no
Dock tile or switcher entry, and that the hit-test pipeline actually fires: it
places the sprite over wherever your cursor already is and asserts a hit on
drawn pixels and a miss on transparent ones at the same cursor position. It also
saves screenshots of each display plus a tight crop of the sprite under
`.verify/`, so the art and its transparency can be eyeballed.

Keep the mouse still while it runs; it tells you if the cursor moved.

For a live view of the decision, set `AI_BUDDY_TRACE_HITTEST=1` and move the
cursor across the sprite. The first line or two are emitted before the window
frame settles and report a stale origin — read the later ones.

**A human** is still needed for the last step, because only the window server
can answer it. Run the app, then confirm:

1. **Clicks pass through empty space.** Click the desktop or a window anywhere
   the sprite is not. The click lands underneath.
2. **Clicks on the sprite do not pass through.** Click the sprite's body. The
   window underneath does not receive the click.
3. **Typing is never interrupted.** Put the cursor in another application and
   type. Click the sprite mid-sentence and keep typing. Every keystroke reaches
   the other application and focus never moves.
4. **Follows you across Spaces.** Switch Spaces. The sprite is present on the
   new one, in the same screen position.

To place the sprite somewhere specific — the second display, say — relaunch with
`AI_BUDDY_SPRITE_POS=x,y` in logical points from the top-left of the display
union. That variable exists only until Grab lands and the sprite can be dragged.

## The placeholder Character

`src/assets/placeholder-idle.png` is a generated 32x32 stand-in, not art. It
exists to give click-through something with transparent regions to hit-test
against. Real Characters arrive with the Character Package format.

## Prior art and attribution

[WindowPet](https://github.com/SeakMengs/WindowPet) (MIT) is the reference for a
Tauri desktop pet: transparent overlay, click-through hit-testing, tray, and
updater. ai-buddy is a greenfield build rather than a fork, for the reasons in
[ADR-0001](./docs/adr/0001-greenfield-tauri-not-fork-windowpet.md).

The overlay here is an independent implementation — no WindowPet source is
copied into this repository. Should any be lifted later, it is MIT and the
attribution belongs in this section.

[desktop-homunculus](https://github.com/not-elm/desktop-homunculus) informed the
MCP-server-as-companion shape considered and rejected in the same ADR.

## License

MIT.
