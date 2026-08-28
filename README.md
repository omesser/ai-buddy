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
or closes. There is no Character Package, no Director, and no Functional Layer
yet, and the sprite cannot be grabbed.

## Running it

macOS only for now. Windows is stubbed deliberately — see
[docs/SPEC.md](./docs/SPEC.md).

```sh
cd src-tauri
cargo run
```

No Node toolchain and no bundler: the front end is static files under `src/`,
which Tauri embeds at build time.

## Development

Install the hooks once after cloning:

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

**Unit tests** cover the arithmetic — the alpha lookup and the coordinate
conversions. Fast, pure, no windowing system, because the core crate depends on
no platform binding at all:

```sh
cargo test -p ai-buddy-core     # the pure core, builds anywhere
cargo test                      # everything, including the macOS shell
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

The sprite starts in the middle of the first display and goes wherever gravity
and your windows take it from there — its position is the Engine's, and until
Grab lands there is no way to place it by hand. To watch it react, move or close
the window it is sitting on.

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
