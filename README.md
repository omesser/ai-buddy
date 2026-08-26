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

## Verifying the overlay by hand

Click-through, panel level, and Spaces membership are thin, platform-specific,
and expensive to fake convincingly, so they are checked by hand rather than in
tests. The hit-test arithmetic underneath them is unit tested.

Run the app, then confirm each of these:

1. **Clicks pass through empty space.** Click the desktop or a window anywhere
   the sprite is not. The click lands underneath, and the sprite is unaffected.
2. **Clicks on the sprite do not pass through.** Click the sprite's body. The
   window underneath does not receive the click. Try the transparent corners of
   the sprite's bounding box too — those must pass through, which is the whole
   point of hit-testing alpha rather than the rectangle.
3. **Typing is never interrupted.** Put the cursor in another application and
   type. Click the sprite mid-sentence and keep typing. Every keystroke reaches
   the other application and focus never moves.
4. **Absent from the application switcher.** Hold Cmd-Tab. ai-buddy is not
   listed, and there is no Dock tile.
5. **Follows you across Spaces.** Switch Spaces. The sprite is present on the new
   one, in the same screen position.
6. **Works on more than one display.** With two displays attached, repeat checks
   1–3 against overlapping windows on each. To place the sprite on the second
   display, relaunch with `AI_BUDDY_SPRITE_POS=x,y` in logical points from the
   top-left of the display union — the app logs the union's size at startup.
   That variable exists only until Grab lands and the sprite can be dragged.

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
