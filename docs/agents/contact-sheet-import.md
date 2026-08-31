# Contact-sheet import

How to turn a pack of loose PNGs into a Character Package with
`scripts/import-pet.py --format frames`. The mode exists for you: nothing in
the pack names a pose, so somebody with eyes has to look at the art and say
which frames form which Animation. That somebody is the agent running this.

## When to reach for it

Check the source ecosystem first. The importer has two adapters, and both do
more than this mode does.

- The pack came from [petscodex.com](https://petscodex.com/), or sits in
  `~/.codex/pets/<id>/` — use `--format petscodex`.
- The pack is a Shimeji-ee mascot: `shime*.png` files, with or without a
  `conf/actions.xml` — use `--format shimeji`.
- Anything else — use `--format frames`.

Reach for `--format frames` only when no adapter reads the pack. An adapter
knows what its poses mean and mirrors art that heads the wrong way; this mode
knows neither.

## The two passes

The mode runs twice over two different directories, and it tells the passes
apart by one thing: **a source directory with a `character.manifest` in it is
pass 2; a source directory without one is pass 1.** There is no flag.

Set up Python once. [uv](https://docs.astral.sh/uv/) owns it, and the
finishing pass reads TOML with `tomllib`, so the interpreter must be 3.11 or
newer — a bare `uv venv` may pick an older one:

```sh
uv venv --python 3.12
uv pip install pillow
```

### Pass 1 — sheet the pack

```sh
.venv/bin/python scripts/import-pet.py <pack> --format frames -o /tmp/<id>-worksheet
```

You get a **worksheet**: `contact-sheet.png`, a `frames/` copy of every PNG in
the pack, and a `character.manifest` whose every frame list is empty. A
worksheet is not a Character Package — `character::load` rejects an Animation
with no frames, and the tool says so.

An unknown ecosystem declares no license, so pass 1 always warns about it.
That warning is correct and does not stop the import.

### Read the sheet

Open `contact-sheet.png`. Each cell carries the frame's number and its file
name, and the same numbering is repeated as a `# frame N:` index in the
worksheet manifest's comment header.

Thumbnails are 96px and tell poses apart, nothing more. **Do not judge facing
or detail from the sheet.** When a frame's pose or direction decides a
mapping, open that frame from `frames/` at full size, or render the
candidates enlarged.

### Fill the manifest

Write each Animation's frame list into the worksheet `character.manifest`,
naming the files the index names:

```toml
[animations.walk]
frames = ["frames/pose1.png", "frames/pose2.png", "frames/pose1.png", "frames/pose3.png"]
fps = 8
```

Rules the tool enforces:

- Every Animation declared in the worksheet must get at least one frame, and
  the skeleton declares all nine of the Required Animation Set: `idle`,
  `walk`, `fall`, `land`, `sit`, `sleep`, `react`, `talk`, `hold`. Leave one
  empty and pass 2 stops and names it.
- Repeating a frame is free. `read_frames` writes each distinct file once and
  the manifest replays the order, so a four-beat walk out of three drawings
  costs three frames.
- A frame the list names must exist under the worksheet directory. Pass 2
  stops on a name it cannot open.

Rules nothing enforces, and you own:

- No pack supplies all nine poses. Substitute rather than invent — the shimeji
  adapter plays its stand for `talk`, and doing the same is honest.
- `idle` decides the whole Character's scale: pass 2 measures how tall the
  idle art stands and resamples everything by that one factor.
- Leave the comment header above the `# frame N:` index alone. It is the
  provenance the finished package carries. Pass 2 keeps everything above the
  index and drops the index itself.

### Pass 2 — finish the package

```sh
.venv/bin/python scripts/import-pet.py /tmp/<id>-worksheet --format frames -o characters/<id>
```

Pass 2 defringes, resamples, registers each Animation's frames to its first
frame, plants each Animation's baseline on the canvas bottom, writes the
Character Manifest, and then runs `character::load` over the result. Add
`--force` to replace an output directory, and `--stand <px>` when the default
100–130 band is the wrong on-screen height.

Validate again at any time:

```sh
cargo run -q -p ai-buddy-core --example validate -- characters/<id>
```

### Review before you ship

`character::load` accepting the output means the package parses, not that the
art reads. Look at the finished frames and hold them to this bar:

- Walk heads right.
- Sleep reads as rest.
- Every Animation reads as its name, to somebody who has never seen the pack.

A miss means you re-open the worksheet, change that Animation's list, and
re-run pass 2 with `--force`. That loop is the normal case, not a failure.

### Write the personality

Pass 2 writes no `personality.txt`, ever, and a Character Package without one
is not finished. Author it to fit the art. Read
`characters/bmo/personality.txt` for the length and register: a few sentences
in the third person, saying who the Character is and how it speaks.

## What this mode will not do for you

- **It mirrors nothing.** Art that heads left stays heading left, so a
  side-facing pack drawn heading left cannot pass the walk bar. Mirror the
  PNGs yourself before pass 1.
- **A worksheet inside a zip's top-level folder is not pass 2.** The zip
  unpacks to a temporary directory, and the manifest lands one level down
  where the pass-1/pass-2 test does not look. The tool does not stop — it
  sheets the worksheet again, contact sheet included, and hands you a
  plausible-looking second worksheet. Pass 2 a directory, not a zip.
- **`-o` must not point back at the worksheet.** The tool clears the output
  directory before writing, which deletes the worksheet — `contact-sheet.png`
  and the frame index are gone, and you cannot revise the mapping without
  re-running pass 1. It reports success either way. Write pass 2 somewhere
  else.
- **`[director]` does not survive.** The worksheet manifest carries a
  `[director]` block, but `read_frames` ignores it and pass 2 re-emits the
  default `model_base = 2`, `model_power = 1`. Tune the Director in the
  finished package, after pass 2, not in the worksheet.
