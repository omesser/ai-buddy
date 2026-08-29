# ai-buddy

A desktop companion in the spirit of Windows 95-era desktop mascots: an animated
sprite that lives on your screen, reacts to the windows around it, and can be
asked to do real work on your machine.

Vocabulary is defined in [CONTEXT.md](./CONTEXT.md), the design in
[DESIGN.md](./DESIGN.md), the v1 scope in [docs/SPEC.md](./docs/SPEC.md), and the
decisions that carry lock-in in [docs/adr/](./docs/adr/).

## State

Early. Work is tracked as [GitHub issues](https://github.com/omesser/ai-buddy/issues).
The overlay is up and the frame loop runs the Engine, so the sprite falls, lands
on the top edge of whatever window is under it, and drops when that window moves
or closes, and it stands on the Dock rather than behind it. It can be clicked,
picked up, dragged and thrown. It is a real Character Package on disk, and its
Animations play at the speeds its Character Manifest declares. Startup stops if
no package loads, because a companion with no Character has nothing to be. There
is no Director and no Functional Layer yet, and right-clicking does nothing —
the menu it opens is the tray's, which arrives with #18.

The Engine drives six of the eight required Animations today. `idle`, `walk`,
`fall`, `sit` and `sleep` each answer a State; `fall` covers being dragged as
well, and `react` answers a Poke. `land` and `talk` are events rather than
States and start playing when Behaviors do.

## Running it

macOS only for now. Windows is stubbed deliberately — see
[docs/SPEC.md](./docs/SPEC.md).

```sh
cd src-tauri
cargo run
```

No bundler: the front end is static files under `src/`, which Tauri embeds at
build time.

## Development

### Toolchains

Three, and each earns its place:

| Toolchain | Needed for | Needed to build? |
|---|---|---|
| **Rust** | everything: the core crate, the Tauri shell | yes |
| **Python** | `pre-commit`, and the placeholder Character's frame generator | no |
| **Node** | the renderer's unit tests, and nothing else | no |

Node is the newest and the least obvious, so: the webview front end has been
JavaScript since the first overlay commit, because that is what a Tauri front
end is. What Node adds is a way to *test* the arithmetic in it. `interpolate`
runs once per display frame between Engine ticks, so it cannot live in Rust, and
docs/SPEC.md holds that "arithmetic is never exempt, wherever it lives".

That dependency is deliberately as thin as it goes: `node --test` and
`node:assert` from the standard library, no test framework, no package manager,
no lockfile, and no `node_modules`. The root `package.json` exists only to
declare the renderer ESM, so no Node release's module detection has to guess.
Any Node that ships `node --test` will do; CI uses the current LTS.

### Hooks

Hooks lint, format and typecheck. They do not run tests — neither `cargo test`
nor the renderer's. Tests run in CI, and by hand as below.

Install them once after cloning:

```sh
pre-commit install
```

They cover whitespace and line endings, YAML/JSON/TOML validity, spelling, shell
formatting and shellcheck, plus `cargo fmt --check` and `cargo clippy -D
warnings`. The toolchain is pinned in `rust-toolchain.toml` so local runs and CI
agree on what rustfmt and clippy consider correct.

CI runs the same hooks on both a Linux and a macOS runner, because the shell
carries a non-macOS code path that only a Linux build exercises.

## Verifying the overlay

Most of what this feature does is invisible. Nothing on screen says whether the
overlay is currently swallowing clicks or passing them on, so verification is
split in three.

**Unit tests** cover the arithmetic — the alpha lookup, the coordinate
conversions, frame selection, and the renderer's interpolation between Engine
ticks. Fast, pure, no windowing system, because the core crate depends on no
platform binding at all:

```sh
cargo test -p ai-buddy-core     # the pure core, builds anywhere
cargo test                      # everything, including the macOS shell
node --test tests/*.test.js     # the renderer's own arithmetic
```

**`scripts/verify-overlay.sh`** covers everything else a machine can reach. It
is deliberately not a `cargo test`: it needs a real desktop, a real window
server and a running app, so it is slow, macOS-only, and cannot run in CI. Run
it when the overlay, the platform layer or the frame loop changes.

```sh
scripts/verify-overlay.sh          # or --keep to leave the app running
```

It checks that exactly one overlay window exists at floating level, that its
bounds match the union of the displays, and that the app is an accessory with no
Dock tile or switcher entry.

Then it checks the frame loop against a real desktop. It opens a plain window of
its own below where the sprite starts, so the sprite has a Perch to aim at, and
steps that window down the screen before closing it. Reading the app's own frame
trace against the bounds the window server reports, it asserts that the sprite
falls under gravity, comes to rest on that window's top edge, follows the edge
when the window moves, drops when the window closes — each within about one poll
interval — and comes to rest again on the display below.

Last it checks the hit-test pipeline: it puts the cursor on the sprite's centre
and then on its transparent top-left corner, and asserts a hit on the first and
a miss on the second. The cursor goes back where you left it. It also saves
screenshots of each display plus a tight crop of the perched sprite under
`.verify/`, so the art and its transparency can be eyeballed.

Keep hands off the mouse while it runs.

For a live view of what the app is deciding, set `AI_BUDDY_TRACE_HITTEST=1` for
the click-through decision and `AI_BUDDY_TRACE_FRAMES=1` for the Engine's
frames — state, position and animation, once per tick. The first line or two of
the hit-test trace are emitted before the window frame settles and report a
stale origin; read the later ones.

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
5. **Motion is continuous, not stepped.** Watch it fall. It slides down the
   screen rather than jumping between positions, and it does not judder when it
   crosses a window's edge.
6. **The art is crisp.** On a Retina display the pixels are hard squares with no
   blur or soft edges, and every pixel of the sprite is the same size as every
   other. A blurred sprite means the integer scale or the nearest-neighbour
   filtering was lost.
7. **It rests on the Dock, not behind it.** Let the sprite settle at the bottom
   of the screen. Its feet stand on the Dock's top edge and the whole sprite is
   visible. Then turn on Dock auto-hiding in System Settings: within a poll the
   sprite falls the rest of the way to the bottom of the screen, because the
   Dock gave the space back. Turn it off and the sprite is lifted again.
8. **Declared cadence is honoured.** Point ai-buddy at a copy of the
   placeholder with a faster `fps idle`, and the idle bob is visibly faster
   than it was at the declared 3. Editing the repository's own
   `characters/` changes nothing on its own: the app reads the copy
   `tauri-build` placed next to the binary, not the source of that copy.

   ```sh
   mkdir -p /tmp/ai-buddy-fast
   cp -R characters/placeholder /tmp/ai-buddy-fast/
   sed -i '' 's/^fps idle = 3$/fps idle = 20/' /tmp/ai-buddy-fast/placeholder/character.manifest
   cd src-tauri && AI_BUDDY_CHARACTERS=/tmp/ai-buddy-fast cargo run
   ```

   `AI_BUDDY_CHARACTERS` replaces the search paths rather than adding to them,
   so nothing installed is touched and there is nothing to put back.

9. **A click makes it react.** Click the sprite once without moving the mouse.
   It plays its `react` animation for about half a second, then goes back to
   what it was doing. Clicking again while it reacts restarts the reaction.
10. **Press and drag picks it up.** Press on the sprite and move. It follows the
    cursor. Drag faster than it can follow, so the cursor leaves the art
    entirely — it stays held. Release over a window and it lands on that
    window's top edge.
11. **A flick throws it.** Drag and release while still moving and it leaves
    your hand on an arc. Hold still for a moment before releasing and it drops
    straight down instead, which is how you put it down rather than throw it.

The sprite starts in the middle of the first display and goes wherever gravity
and your windows take it from there — its position is the Engine's, and until
Grab lands there is no way to place it by hand. To watch it react, move or close
the window it is sitting on.

## Character Packages

A Character Package is a directory or a `.zip` archive holding a
`character.manifest`, a `personality.txt`, and the frames its manifest names.
ai-buddy looks for them in two places, in order:

1. `~/Library/Application Support/ai-buddy/characters/` — anything you add.
2. The Characters shipped with the app, which live in `characters/` in this
   repository and are copied next to the binary at build time.

The first package that loads is the one you get. Set `AI_BUDDY_CHARACTERS` to a
`:`-separated list of directories to look in those instead, which is how to try
a package without installing it.

A package that is rejected says why, one line per mistake. A mistake in a
declaration names the declaration and the line it is on; a mistake the package
makes as a whole, such as declaring no name, has no line to point at. A
directory that holds no `character.manifest` is skipped silently: it was never
a package, which is a different thing from a broken one.

A `.zip` made by Finder's Compress loads as it is. The `__MACOSX/` tree and the
`.DS_Store` files Finder puts in it describe your Mac rather than the Character,
so they are ignored.

The format stays internal and undocumented until v2 — see
[DESIGN.md](./DESIGN.md).

### The placeholder Character

`characters/placeholder/` is a generated stand-in, not art. It exists so the
Engine has a Character to drive, and so click-through has something with
transparent regions to hit-test against. Regenerate its frames with:

```sh
python3 scripts/make-placeholder-character.py
```

Standard library only, so there is nothing to install. Real Characters are drawn
by hand.

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
