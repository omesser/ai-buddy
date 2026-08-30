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
on the top edge of whatever window is under it, rides that edge when the window
is dragged slowly, and drops when the window is yanked or closed, and it stands
on the Dock rather than behind it. It can be clicked,
picked up, dragged and thrown. It knows when to get out of the way: it fades out
while a fullscreen application has the screen, goes away at once on
Control-Option-Command-B and comes back the same way, and never appears in a
screen share or a screen recording at all. It is a real Character Package on
disk, and its Animations play at the speeds its Character Manifest declares.
Startup stops if no package loads, because a companion with no Character has
nothing to be. A Director proposes Behaviors: Static weights with nothing
configured, or an HTTP stand-in if you set a key (see [Running it](#running-it)).
A Harness will replace that stand-in ([ADR-0008](./docs/adr/0008-one-harness-session.md)).
There is no chat surface and no menu yet: double-clicking is a Summon the
Engine accepts and nothing answers (#17), and right-clicking does nothing at
all — the menu it opens is the tray's (#18).

The Engine drives all nine required Animations. `idle`, `fall`, `sit`, `sleep`
and `walk` each answer a State, `fall` covering being dragged as well; `land`
plays when a fall ends, `hold` when a Perch is ridden, and `react` answers a
Poke. Eight of the nine are also Primitives a Character can compose into a
Behavior — all but `fall`, which is what losing your footing looks like rather
than something a Behavior can ask for. A Behavior plays its Primitives in order and the Behaviors it chains into,
and is refused or abandoned when the State the sprite is in does not permit it.
`talk` plays when a proposal names a Behavior that includes it.

## Running it

macOS only for now. Windows is stubbed deliberately — see
[docs/SPEC.md](./docs/SPEC.md).

```sh
cd src-tauri
cargo run
```

No bundler: the front end is static files under `src/`, which Tauri embeds at
build time.

With no Director key the sprite still has a life: Static weights pick among
the Character's declared Behaviors. A key turns on the HTTP stand-in
([ADR-0008](./docs/adr/0008-one-harness-session.md) — disposable until a
Harness attaches).

OpenAI, Anthropic's compatibility layer, and Ollama use `/v1/chat/completions`.
[xAI](https://docs.x.ai/developers/model-capabilities/text/comparison) uses
`/v1/responses`; `AI_BUDDY_DIRECTOR_BASE_URL=https://api.x.ai` selects that
path. An explicit full URL (ending in `/chat/completions` or `/responses`)
is used as-is.

```sh
# OpenAI
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$OPENAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.openai.com \
AI_BUDDY_DIRECTOR_MODEL=gpt-4o-mini \
cargo run

# Anthropic (OpenAI-compatible /v1/chat/completions)
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$ANTHROPIC_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.anthropic.com \
AI_BUDDY_DIRECTOR_MODEL=claude-haiku-4-5 \
cargo run

# xAI — get a key at https://console.x.ai
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY="$XAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.x.ai \
AI_BUDDY_DIRECTOR_MODEL=grok-4.6 \
cargo run

# Ollama — the key is required by the stand-in and ignored by Ollama
cd src-tauri
AI_BUDDY_DIRECTOR_API_KEY=ollama \
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=llama3.2 \
cargo run
```

| Variable | What it does |
|---|---|
| `AI_BUDDY_DIRECTOR_API_KEY` | Required for the HTTP stand-in. Empty or unset is Static only. |
| `AI_BUDDY_DIRECTOR_BASE_URL` | Provider origin. Default `https://api.openai.com`. |
| `AI_BUDDY_DIRECTOR_MODEL` | Model name. Default `gpt-4o-mini`. |
| `AI_BUDDY_DIRECTOR` | `off`, `0`, or `false` keeps Static even when a key is set. |
| `AI_BUDDY_DIRECTOR_WAKE_SECS` | First ambient wait, in seconds (default 900). Doubles after each unused ambient wake, caps at two hours. Not a heartbeat. Poke and Summon wake immediately. |

Session calls stay quiet while the main display is asleep. #18 will bind these
in settings.

A 403 from xAI is the server refusing the key, not a bad JSON body (that is
a 400). Keys are granted per-endpoint in [console.x.ai](https://console.x.ai);
`/v1/responses` and `/v1/chat/completions` are separate ACLs. A team that
requires mTLS wants `https://mtls.api.x.ai`. The stand-in retries
chat-completions if Responses returns 403 or 404.

`scripts/probe-model.sh` hits the same Completer without starting the
overlay — GET `/v1/models` (and `/v1/api-key` on xAI), then both POST
paths. Same env as `cargo run`. It prints status and body, never the key.
Later this is also how to check a Harness is reachable.

```sh
AI_BUDDY_DIRECTOR_API_KEY="$XAI_API_KEY" \
AI_BUDDY_DIRECTOR_BASE_URL=https://api.x.ai \
AI_BUDDY_DIRECTOR_MODEL=grok-4.6 \
scripts/probe-model.sh
```

## Development

### Toolchains

Three, and each earns its place:

| Toolchain | Needed for | Needed to build? |
|---|---|---|
| **Rust** | everything: the core crate, the Tauri shell | yes |
| **Python** | `pre-commit`, and the Characters' frame generators | no |
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

It checks that there is one overlay window per display, that every one of them
is on screen at floating level, that each display is covered whole by one of
them and none of them covers anything else, that the window server has been told
never to hand any of them to a screen capture, and that the app is an accessory
with no Dock tile or switcher entry.

Then it checks the frame loop against a real desktop. It opens a plain window of
its own below where the sprite starts, so the sprite has a Perch to aim at, and
steps that window down the screen before closing it. Reading the app's own frame
trace against the bounds the window server reports, it asserts that the sprite
falls under gravity, comes to rest on that window's top edge, rides the edge
when the window steps down, drops when the window closes — each within about one
poll interval — and comes to rest again on the display below.

Last it checks the hit-test pipeline: it puts the cursor on the sprite's centre
and then on its transparent top-left corner, and asserts a hit on the first and
a miss on the second. The cursor goes back where you left it. It also saves a
screenshot of each display under `.verify/`. The sprite is not in them: the
overlay refuses every screen capture, and `screencapture` is one. To photograph
your own Character — to look at its art, or to show somebody — start the app
with `AI_BUDDY_CAPTURABLE=1`, which gives that up for one run.

Keep hands off the mouse while it runs.

Every change of the hide rules prints a `presence:` line without being asked
for, which says whether the sprite was shown or hidden and over how long. It is
a handful of lines in a session, and it is how to tell a rule that did not fire
from a fade that did not play.

For a live view of what the app is deciding, set `AI_BUDDY_TRACE_HITTEST=1` for
the click-through decision and `AI_BUDDY_TRACE_FRAMES=1` for the Engine's
frames — state, position and animation, once per tick. Both trace in the point
space every display shares, so the positions they report are comparable with
what the window server says about any display.

`AI_BUDDY_TRACE_DIRECTOR=1` prints each session wake: `--- prompt ---` is
what we sent, `--- model ---` is the reply, then whether that played as a
Behavior (or fell back to Static). Off unless asked. Poke, throw, pick up,
or place on a Perch to force a wake.

There is one overlay per display and every one of them is told where the
sprite is, so the trace is one line per tick rather than one per overlay. Which
overlay the cursor is on is on the hit-test line.

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
   Blip whose idle declares a faster `fps`, and the idle bob is visibly faster
   than it was at the declared 3. Editing the repository's own
   `characters/` changes nothing on its own: the app reads the copy
   `tauri-build` placed next to the binary, not the source of that copy.

   ```sh
   mkdir -p /tmp/ai-buddy-fast
   cp -R characters/blip /tmp/ai-buddy-fast/
   sed -i '' 's/^fps = 3$/fps = 20/' /tmp/ai-buddy-fast/blip/character.manifest
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
12. **It can be put down over the Dock, and does not stay there.** Drag it down
    over the Dock — it follows the cursor the whole way, because a held sprite
    goes where your hand goes. Let go and it settles back onto the Dock's top
    edge, fully visible.
13. **The two shipped Characters are two companions.** Run each in turn (see
    [The shipped Characters](#the-shipped-characters)) and watch it idle. BMO
    stands in three-quarter view and blinks about once a second, in the ten
    flat colours of its sheet; Nim eases through six, blinks, and carries a
    translucent shadow. Poke each: BMO's startle is two frames and over, Nim's
    plays through five. The Behaviors each declares show when a Director
    proposes one (Static, or the HTTP stand-in with a key). Sitting and
    sleeping can also be the Engine's own idling and look the same for either.
    Then judge the drawing itself, which no test can: the frames are generated
    by a script and nobody has claimed they are good.

14. **A fullscreen application takes the screen and the Character leaves it.**
    Put any application into fullscreen — the green button, or
    Control-Command-F. Within about a tenth of a second the sprite fades out
    over half a second: it dissolves rather than blinking off. Leave
    fullscreen and it fades back in, carrying on from wherever the Engine got
    to, still falling or still walking rather than restarting. A *zoomed* window
    is not a fullscreen one: Option-click the green button, or double-click a
    title bar, and the sprite stays and can still sit on that window's top edge.
    On two displays, a fullscreen application on either one takes the sprite
    away from both: what decides is the window you are working in, not the
    screen the sprite is standing on. Then quit ai-buddy, go fullscreen, and
    start it again: the sprite never appears at all, because what it must be
    rides on every frame rather than being announced once, before the window
    that draws it was listening.
15. **Ordinary window switching changes nothing.** Command-Tab between
    applications, open and close windows, drag them around, switch Spaces. The
    sprite never blinks, never changes what it is in front of, and never
    disappears. Only a fullscreen application takes it away.
16. **The hotkey puts it away and brings it back at once.** Press
    Control-Option-Command-B. The sprite is gone on the keystroke, with no
    fade. Press it again and it is back, instantly, wherever it had got to.
    While it is away, click where it was: the click reaches the window
    underneath, and the sprite does not react to it when it returns.
17. **The hotkey outranks the rules.** Press the hotkey to put the sprite away,
    then enter a fullscreen application and leave it again. The sprite stays
    away — a fullscreen application quitting must not hand back a Character you
    put away yourself. Press the hotkey again to get it back.
18. **It is absent from a real screen share.** Start a real share — Zoom, Meet,
    Teams — sharing your whole screen, and look at what the other end sees,
    either on a second machine or in the meeting's own preview of your share.
    The sprite is on your screen and not in theirs. Then check the system's own
    capture the same way: Command-Shift-5 to record, and Command-Shift-3 to take
    a screenshot. Neither one contains the sprite. This is the window server
    refusing to hand the overlay to any capture, rather than ai-buddy detecting
    a share, so it holds for every sharing tool including ones nobody has heard
    of. `scripts/verify-overlay.sh` asserts the setting; only this step shows
    the effect. It is also why you cannot screenshot your Character: start the
    app with `AI_BUDDY_CAPTURABLE=1` for a run where you can.

The last three need a second display, and only a window server can answer them:

19. **A Character on a seam is whole.** Drag the sprite slowly across the
    boundary between two displays and hold it there, half on each. Both halves
    are drawn, and they meet — no gap between them, nothing drawn twice, and
    the art is not doubled or offset at the seam. Watch them while your hand is
    still moving, not only once it stops: each overlay interpolates between the
    Engine's ticks on its own clock, so a moving sprite is the only thing that
    can catch two overlays disagreeing about where it is. Two displays with
    different scale factors or refresh rates is the case worth doing this on.
20. **Either half can be clicked.** With the sprite straddling, click the half
    on each display in turn. Both pick it up, and clicking beside it on either
    display still reaches whatever is underneath.
21. **A display can come and go.** With the app running, unplug a display, or
    turn one off in System Settings > Displays. The sprite carries on, and the
    remaining display still passes clicks through where the sprite is not.
    Plug it back in: the sprite can be dragged onto it again within a second
    or so, without restarting the app.

The sprite starts in the middle of the first display and goes wherever gravity
and your windows take it from there, or wherever you put it. To watch it fall
without touching it, move or close the window it is sitting on.

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

### The shipped Characters

Two ship, in deliberately different styles, so that the format is proven against
real variance rather than against itself:

- **`characters/bmo/` — BMO**, game-sprite art. Every pose is cut from the
  *Flambo's Inferno* sprite sheet (see
  [Prior art and attribution](#prior-art-and-attribution)), quantised to its
  ten flat colours, side-on and heading right — the Engine's facing mirrors it
  to walk left. The idle, talk and sleep faces are drawn by hand on the
  sheet's own screen, in its grid and palette. BMO never settles: every
  Behavior it declares ends on its feet.
- **`characters/nim/` — Nim**, modern pixel art. A palette larger than
  sixteen colours shaded on a ramp lit from the upper left, a translucent
  contact shadow wherever there is ground to cast it on, and twice the frames
  everywhere so the motion eases rather than steps. Nim comes to rest: every
  Behavior it declares but the walk ends sitting or asleep.

The difference is the Behaviors as much as the drawing, which is what makes
switching feel like a different companion rather than a reskin — from the day a
Director proposes one. Sitting and sleeping can also be the Engine's own
idling when nothing has proposed. Both are drawn by a
script rather than by hand, which is what the repository can honestly claim;
regenerate both with:

```sh
python3 scripts/make-shipped-characters.py
```

#### Running one of them

Name a Character to start that one:

```sh
cd src-tauri && AI_BUDDY_CHARACTER=bmo cargo run
cd src-tauri && AI_BUDDY_CHARACTER=nim cargo run
cd src-tauri && AI_BUDDY_CHARACTER=blip cargo run
```

The name is the package's directory, without the `.zip` if it is an archive.
Naming a Character that is not installed starts nothing and says so, rather than
quietly starting a different one.

With nothing named you get **BMO**, which is `DEFAULT_CHARACTER` in
`src-tauri/src/package.rs` rather than whichever package happens to sort first —
otherwise adding one could silently replace the Character everybody meets. It is
a preference and not a requirement: if BMO will not load, the search carries on
behind it. Remembering a Character you chose is settings, which is #18.

Either way the app prints what it loaded, which is the quickest way to be sure
you are looking at the Character you meant:

```
character: BMO from ../characters/bmo
```

This is a developer's switch, and the app has no menu to change Character while
it runs — that is #18. To try a package without installing it, point the search
somewhere else instead:

```sh
cd src-tauri && AI_BUDDY_CHARACTERS=/path/to/my-packages cargo run
```

### Blip, the stand-in

`characters/blip/` is a generated stand-in rather than art. It shipped before
BMO and Nim did, so that the Engine had a Character to drive and click-through
had something with transparent regions to hit-test against, and it stays for
both. Regenerate its frames with:

```sh
python3 scripts/make-blip-character.py
```

Standard library only, so there is nothing to install, which is the same reason
Nim is generated the same way.

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

BMO's frames are cut from a community rip of the *Adventure Time: Flambo's
Inferno* browser-game sprite sheet, quantised to a fixed palette and
hand-adjusted (faces, sleep screen). BMO and the underlying sprites are
Cartoon Network IP; the character is a development asset, and none of this
repository's license claims cover that art.

## License

MIT.
