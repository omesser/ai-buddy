# 0009 — Two shipped Characters

## Why

Two Characters in genuinely different styles validate the Character Package abstraction
against real variance before the format is published. One style would only prove the
format fits itself.

## Scope

- **Faithful Win95** — 16-color, hard pixels, dithering.
- **Modern pixel art** — larger palette, smoother animation.

Each ships the eight required animations, a Personality Prompt, and declared Behaviors.
The two must move and react differently enough that switching feels like a different
companion rather than a reskin.

## Acceptance criteria

- Both packages load through the same loader with no special-casing.
- Both are playable with the static Director alone.
- Switching between them visibly changes idle life, not just artwork.
- Neither package requires a Primitive that the other cannot use.

## Tests

Both packages load and validate in Engine tests. Visual quality is judged by hand.
