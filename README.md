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

Cursor's `CURSOR_API_KEY` is for the Cloud Agents API and SDKs, not a
Completer. `https://api.cursor.com` has no `/v1/chat/completions`; a POST
there is a 404 and Static takes over.

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

# Ollama — a local server, so no key at all
cd src-tauri
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
cargo run
```

| Variable | What it does |
|---|---|
| `AI_BUDDY_DIRECTOR_API_KEY` | Required for a remote provider. Empty or unset is Static only — unless the base URL is [local](#a-local-model-server), which needs no key. |
| `AI_BUDDY_DIRECTOR_BASE_URL` | Provider origin. Default `https://api.openai.com`. |
| `AI_BUDDY_DIRECTOR_MODEL` | Model name. Default `gpt-4o-mini`. |
| `AI_BUDDY_DIRECTOR` | `off`, `0`, or `false` keeps Static even when a key is set. |
| `AI_BUDDY_DIRECTOR_TIMEOUT_SECS` | Completer timeout. Default 20 remote, 120 local — a cold local model loads weights on the first call. |
| `AI_BUDDY_DIRECTOR_MAX_TOKENS` | Reply cap. Default 80 remote, 512 local. |
| `AI_BUDDY_DIRECTOR_WAKE_SECS` | First proactive model-call wait, in seconds (default 120). After each proactive model call the wait grows by the Character's `[director]` `model_base` and `model_power` (`wait * model_base ^ model_power`, default doubling), and caps at two hours. Not a heartbeat. Poke and Summon wake immediately. |

A Character that should grow faster or slower than doubling says so:

```toml
[director]
model_base = 3
model_power = 1
```

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

### A local model server

The buddy wakes on a pace all day and every Poke is a wake on top of that, so
a hosted API puts a meter on idling — and each wake sends the frontmost
application name and the clock off the machine. A model served on your own
desk removes both problems, and it needs no key: set a local
`AI_BUDDY_DIRECTOR_BASE_URL` and leave `AI_BUDDY_DIRECTOR_API_KEY` unset.
Local means loopback, an RFC1918 address, or a `.local` name; anything else
still requires a key, so a missing cloud key never turns into an
unauthenticated request.

All five servers below speak `/v1/chat/completions`, which is the path the
Completer already builds:

| Server | Base URL | Model name | Tested |
|---|---|---|---|
| [Ollama](https://ollama.com) | `http://localhost:11434` | a tag: `gemma4`, `llama3.2:3b` | yes — `gemma4:latest`, 9.6 GB, on an Apple-silicon Mac |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) `llama-server` | `http://localhost:8080` | the gguf path, or `--alias` | no |
| [LM Studio](https://lmstudio.ai) | `http://localhost:1234` | the id shown in its server tab | no |
| [vLLM](https://docs.vllm.ai) | `http://localhost:8000` | the served model id | no |
| [MLX](https://github.com/ml-explore/mlx-examples) `mlx_lm.server` | `http://localhost:8080` | a Hugging Face repo id | no |

Only Ollama has been run against this repo; the other four are listed because
they speak the same path, not because anyone here has proved them. MLX's own
docs say its server is not meant for production.

Check a server before you trust it — this needs no key either, and reports
whether the model you configured is actually loaded:

```sh
AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
AI_BUDDY_DIRECTOR_MODEL=gemma4 \
scripts/probe-model.sh
```

At startup the app asks the same question once, in the background, and says
so when the answer is no:

```
director: http://localhost:11439 unreachable: Connection refused; staying on StaticDirector until it answers
director: http://localhost:11434 model "llama3.2" is not served; it has gemma4:latest
```

Neither line stops anything: a wake that fails already falls back to Static
per turn. The line exists so a buddy that went quiet is not a mystery.

**Size and the reply contract.** The Director asks for a Behavior name on one
line and an optional spoken line on the next. Small models break that shape
more often than hosted ones do, and every break is silent — an unparsable
reply becomes speech, a failed one becomes Static. A local reasoning model
(Qwen3, gpt-oss) has a second failure: it thinks inside the same token budget
on chat-completions, so a tight cap can be spent before it writes anything.
That is why the local cap defaults to 512 rather than 80. Constrained
decoding is the real fix, and four of the five servers support it through
`response_format`; #144 decides that shape.

## Development

### Toolchains

Three, and each earns its place:

| Toolchain | Needed for | Needed to build? |
|---|---|---|
| **Rust** | everything: the core crate, the Tauri shell | yes |
| **Python** | `pre-commit`, the Characters' frame generators, and the pet importer (the one script that needs Pillow) | no |
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

Then it falls again, onto a window that covers the same ground in one jump
rather than three steps. That one is faster than the sprite can hold on to, so
the assertions invert: it must not be carried, and it has to reach the new edge
by falling onto it.

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

   The Dock does not stretch to the sides of the screen, and the sprite knows
   it: a walk past the Dock's real end falls to the true bottom of the screen,
   and standing on the Dock is standing on the island the screen actually
   shows. The exact rectangle comes from a chain the startup log names —
   a private SPI that needs no consent, then the Accessibility API where that
   trust was already granted (never prompted for), then the full-width
   work-area strip as the fallback where neither answers.
8. **Declared cadence is honoured.** Point ai-buddy at a copy of
   Black Mage whose idle declares a faster `fps`, and the idle is visibly faster
   than it was at the declared 1. Editing the repository's own
   `characters/` changes nothing on its own: the app reads the copy
   `tauri-build` placed next to the binary, not the source of that copy.

   ```sh
   mkdir -p /tmp/ai-buddy-fast
   cp -R characters/black-mage /tmp/ai-buddy-fast/
   sed -i '' 's/^fps = 1$/fps = 20/' /tmp/ai-buddy-fast/black-mage/character.manifest
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
13. **A window you drag slowly carries it.** Let the sprite settle on a
    window's top edge, then drag that window by its title bar — down, up and
    sideways, slowly. The sprite rides the edge and keeps its place along it,
    playing its hold animation while the window moves rather than standing
    idle. It is carried, not launched: stop the drag and it stops with the
    window rather than sliding on.
14. **A window you fling leaves it behind.** With the sprite perched, throw the
    same window: grab the title bar and move fast, or zoom the window by
    double-clicking it. The sprite stays where it stood, in the air, and falls
    — onto the same edge again when the window is still under it, past it to
    whatever is below when it is not. Fling the window upwards and the sprite
    is passed through rather than lifted. Where the line between this and the
    step above falls is a tuned number rather than a computed one: a drag that
    looks slow should ride and one that looks like a yank should not, and a
    disagreement about a drag in between is that number wanting a tune.
15. **The two shipped Characters are two companions.** Run each in turn (see
    [The shipped Characters](#the-shipped-characters)) and watch it idle. BMO
    hums to itself through a four-frame singing loop, in soft drawn lines at
    its authored size; Nim eases through six, blinks, and carries a
    translucent shadow. Poke each: BMO's startle is two frames and over, Nim's
    plays through five. The Behaviors each declares show when a Director
    proposes one (Static, or the HTTP stand-in with a key). Sitting and
    sleeping can also be the Engine's own idling and look the same for either.
    Then judge the drawing itself, which no test can: the frames are generated
    by a script and nobody has claimed they are good.

16. **A fullscreen application takes the screen and the Character leaves it.**
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
17. **Ordinary window switching changes nothing.** Command-Tab between
    applications, open and close windows, drag them around, switch Spaces. The
    sprite never blinks, never changes what it is in front of, and never
    disappears. Only a fullscreen application takes it away.
18. **The hotkey puts it away and brings it back at once.** Press
    Control-Option-Command-B. The sprite is gone on the keystroke, with no
    fade. Press it again and it is back, instantly, wherever it had got to.
    While it is away, click where it was: the click reaches the window
    underneath, and the sprite does not react to it when it returns.
19. **The hotkey outranks the rules.** Press the hotkey to put the sprite away,
    then enter a fullscreen application and leave it again. The sprite stays
    away — a fullscreen application quitting must not hand back a Character you
    put away yourself. Press the hotkey again to get it back.
20. **It is absent from a real screen share.** Start a real share — Zoom, Meet,
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

21. **A Character on a seam is whole.** Drag the sprite slowly across the
    boundary between two displays and hold it there, half on each. Both halves
    are drawn, and they meet — no gap between them, nothing drawn twice, and
    the art is not doubled or offset at the seam. Watch them while your hand is
    still moving, not only once it stops: each overlay interpolates between the
    Engine's ticks on its own clock, so a moving sprite is the only thing that
    can catch two overlays disagreeing about where it is. Two displays with
    different scale factors or refresh rates is the case worth doing this on.
22. **Either half can be clicked.** With the sprite straddling, click the half
    on each display in turn. Both pick it up, and clicking beside it on either
    display still reaches whatever is underneath.
23. **A display can come and go.** With the app running, unplug a display, or
    turn one off in System Settings > Displays. The sprite carries on, and the
    remaining display still passes clicks through where the sprite is not.
    Plug it back in: the sprite can be dragged onto it again within a second
    or so, without restarting the app.

24. **Several buddies run at once.** Start with
    `AI_BUDDY_INSTANCES="bmo:One,bmo:Two,nim:Nim"` (see [Running several
    buddies](#running-several-buddies)). Three sprites come into the world side
    by side rather than in a stack, and each falls and lands on its own. The
    startup line names all three. Nothing stutters: the frame rate with three
    is the frame rate with one, because they share one reading of the desktop
    and one window-list poll.

25. **Each buddy is its own.** Watch the three idle for a minute. They do not
    move in lockstep — the two running the same Character pick their own
    Behaviors at their own moments, because each has its own Director and its
    own seed. Drag one somewhere and let it go: only that one moves, the others
    carry on. Poke one: only that one reacts.

26. **A press finds the buddy under the cursor, even when they overlap.** Drag
    two buddies until their art overlaps, then press where both are. The one
    drawn in front is the one that lifts, and only one lifts. Drag it faster
    than it can follow so the cursor leaves the art and crosses the other
    buddy: the drag keeps hold of the one you picked up and does not hand over.
    Release, and clicking where no sprite is drawn still reaches the
    application underneath.

27. **A second buddy already knows what the first was told.** Memory is one
    file for every Instance —
    `$TMPDIR/ai-buddy-mcp/memory.md`. Write a fact into it, or have a Harness
    write one through the MCP server, and it is the same Memory every buddy
    reads. Dismissing a buddy leaves the file untouched. Nothing in the app
    reads Memory into the Character Prompt yet, so this is a check on the file
    being shared rather than on a buddy reciting it.

The sprite starts in the middle of the first display and goes wherever gravity
and your windows take it from there, or wherever you put it. To watch it fall
without touching it, move or close the window it is sitting on.

### Running several buddies

`AI_BUDDY_INSTANCES` names the buddies to run, as `character:name` separated by
commas:

```sh
cd src-tauri && AI_BUDDY_INSTANCES="bmo:One,bmo:Two,nim:Nim" cargo run
```

The Character is a package name, the same one `AI_BUDDY_CHARACTER` takes. The
name is yours and is what the buddy is called; give the Character alone —
`AI_BUDDY_INSTANCES="bmo,nim"` — and each is named after its package. Naming the
same Character twice runs two of it, which is the point: they share the art and
the Memory, and differ in position and in what they are doing.

Setting nothing runs the one buddy ai-buddy has always run, and
`AI_BUDDY_CHARACTER` still picks its Character.

Spawning and dismissing a buddy while the app runs needs somewhere to type a
name, which arrives with the menu in
[#18](https://github.com/omesser/ai-buddy/issues/18). Until then the set is
settled at launch.

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

### Writing a personality

`personality.txt` is plain prose the loader never interprets, up to 2000
characters. A register alone is not enough: a model given only temperament
converges on the same three assistant-flavored lines. A good one contains
three things, unlabeled (#156):

1. Who the character is and how it carries itself, fused — the paragraph or
   two every shipped file opens with. Skip what the sprite already shows: a
   description of the art buys nothing, and the words are better spent on how
   the character speaks and what it notices.
2. A fixations paragraph: three to five strong, specific opinions that
   generate material — things it loves, resents, takes personally, or takes
   credit for.
3. Sample lines, verbatim, introduced in prose ("It has been heard to say:
   …"). Well-chosen lines also carry the character's recurring bits, which is
   why bits get no section of their own. Be generous: each line is another
   calibration point, and `characters/black-mage/` shows how far that goes. A
   catchphrase belongs here: the prompt asks for variety but leaves repetition
   the character owns to the personality.

#### Universal rules

Leave these out of a personality file. `character_prompt` in
`crates/core/src/director.rs` injects them once for every Character, so the
files cannot drift apart on them:

- Stay in character, and never mention being a model or an assistant.
- Fit the bubble — five short sentences at the most.
- Vary, preferring an unused line, while a signature phrase may recur.
- Lean away from the Behaviors that just played.
- Dialogue is demeanour, never capability: no promising actions on the
  machine, no claiming abilities.

The format stays internal and undocumented until v2 — see
[DESIGN.md](./DESIGN.md).

### The shipped Characters

Two ship, in deliberately different styles, so that the format is proven against
real variance rather than against itself:

- **`characters/bmo/` — BMO**, drawn shimeji art (see
  [Prior art and attribution](#prior-art-and-attribution)): soft anti-aliased
  lines rather than a pixel grid, so its manifest declares
  `render_mode = "smooth"` and `scale = 1` — the render mode ADR-0006
  reserved, and the first Character to use it. Every pose is cut from the
  pack's 46 and heads right; the Engine's facing mirrors it to walk left.
  Idling it sings, then rides its skateboard; walking it sometimes dribbles a
  football; sitting it plays four games on its own screen; scaling a display
  edge it climbs hand over hand — `climb` being the one optional Animation
  the engine asks for by name, with walk art the silent fallback. The idle
  and walk extras are `variant_of` declarations: more art for the same life,
  cycled by the renderer a few seconds apiece. BMO never settles: every
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
cd src-tauri && AI_BUDDY_CHARACTER=jotaro-kujo cargo run
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

### Importing a pet

`scripts/import-pet.py` translates a pet from another desktop-pet ecosystem
into a Character Package, once. An authoring tool rather than a build step: the
output lands in a directory you review, hand-tune, and own — not a runtime
dependency and not a live bridge (#112). It is the one script allowed to need
Pillow, because petscodex ships webp sprite sheets and decoding webp rules out
the standard library.

You can import a pack from these galleries into a Character Package: [Pets Codex](https://petscodex.com/) and [petdex](https://petdex.dev/), which use the Pets Codex / petdex engine (same atlas, one adapter), and [Shimeji Shop](https://shimejishop.com/), which uses Shimeji-ee. Those are the only adapters. No other format is supported. If you want another format, open a GitHub issue. In that issue, show that the request does not break license restrictions (the importer warns on undeclared or non-permissive licenses; whoever imports is who ships; see the license paragraph later in this section).

`npx petscodex install <id>` lands a pack at `~/.codex/pets/<id>/`; the importer slices it by petdex's row semantics and builds the whole Required Animation Set, with `waiting` as an idle `variant_of` ring member. Shimeji-ee packs are per-pose PNGs plus the pack's `actions.xml` naming pose sequences, every frame mirrored to head right. A pack of bare `shime*.png` files with no conf (shimejishop distributes these) rides Shimeji-ee's standard conf instead.

An ecosystem without an adapter needs one written — the importer reads a machine-readable convention or it reads nothing. There is deliberately no generic fallback for a bare pile of frames ([ADR-0009](./docs/adr/0009-no-generic-import-on-ramp.md)).

Pillow lives in a [uv](https://docs.astral.sh/uv/)-managed virtual
environment, never in a system Python:

```sh
uv venv
uv pip install pillow

npx petscodex list
npx petscodex install labubu
.venv/bin/python scripts/import-pet.py ~/.codex/pets/labubu --format petscodex \
    -o characters/labubu
cd src-tauri && AI_BUDDY_CHARACTER=labubu cargo run
```

The first run after an import rebuilds, which refreshes the shipped-character
copy beside the binary; `AI_BUDDY_CHARACTERS=/path/to/characters` skips the
rebuild by searching the directory itself.

The importer notices a missing Pillow and says exactly this rather than
tracing back.

A `.zip` works as a source wherever a directory does, `--force` replaces an
existing output directory, and `--stand` names the on-screen height in
logical pixels when the default band (100–130, where a shimeji stands) is
not the right size for the pet. The importer prints the pet's license, and
warns when it is undeclared or not one of the permissive ones it recognizes —
MIT, CC0, CC-BY-4.0, Apache-2.0, BSD-3-Clause. It warns rather than refuses:
whoever imports a pet is who ships it, and a tool cannot read a license on
their behalf. The manifest header keeps that record after the terminal is
gone. An import is a development asset unless its license says otherwise, and
one that ships gets a line in
[Prior art and attribution](#prior-art-and-attribution). Success is
declared only after `character::load` accepts the output, through a validator
that is also useful on its own:

```sh
cargo run -p ai-buddy-core --example validate -- characters/labubu
```

Then look at the frames — the tool says so at the end of every run, because
two things are the reviewer's to judge, not code's:

- **Walk must head right**; the Engine's facing mirrors it for leftward
  travel. Every petscodex pet sampled so far draws petdex's "running-right"
  row heading left, so the importer cuts walk from the other row by default.
  `--walk-row 1` picks the first row anyway, and `--mirror-walk` flips
  whichever row is chosen, for the pet whose only clean walk heads left.
- **Sleep must read as sleep.** petscodex has no sleep row, so the importer
  synthesizes one: idle's stillest frame, twice, the second lifted a pixel as
  the breath. Swap in a better pose when the sheet has one.
- **Every animation must read as its name.** The row semantics are labels
  each generated pet interprets loosely — one pet's "jumping" row is a sword
  lunge, and its sleeping art lives in the "waiting" row. `--map` recuts an
  animation from the row the pet actually drew: `--map sleep=6:2,3` takes
  frames 2 and 3 of row 6, `--map react=7` takes all of row 7. Remaps are
  recorded in the manifest header.
- **A personality is authored, never derived.** The importer writes no
  `personality.txt` — the pet's own description is provenance, not a voice.
  Write one that fits the art, the way the shipped characters' read.

`characters/cat/` is the first shipped import — petscodex's `cat`, with the
defaults. The importer's self-test runs with no pet installed:

```sh
.venv/bin/python scripts/import-pet.py --self-test
```

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

BMO's frames are cut from the free
[BMO shimeji pack](https://shimejishop.com/free/bmo-shimeji/) on shimejishop,
flipped to head right, with a one-pixel breathing shift added for sleep. BMO
is Cartoon Network IP and the pack is fan art; the character is a development
asset, and none of this repository's license claims cover that art.

Cat's frames are cut from the [petscodex](https://petscodex.com/pets/cat) pet
`cat` by `scripts/import-pet.py` (#112). The installed package declares no
license, which the importer warns about; the character is a development asset,
and none of this repository's license claims cover that art.

Trump's frames are cut from the [petscodex](https://petscodex.com/pets/trump)
pet `trump` by the same importer.

Jotaro Kujo's frames are cut from the
[petscodex](https://petscodex.com/pets/jotaro-kujo) pet `jotaro-kujo` by
`scripts/import-pet.py`. The installed package declares no license, so the
import was accepted with `--accept-license`. Jotaro Kujo is Shueisha /
Hirohiko Araki IP and the pack is fan art; the character is a development
asset, and none of this repository's license claims cover that art.

## License

MIT.
