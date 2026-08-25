# 0014 — Memory store

## Why

Memory is one shared Markdown file the user owns. Plaintext-inspectable was a hard
requirement: the user must be able to read exactly what the buddies know, edit it in any
editor, and wipe it.

## Scope

One Markdown file, append-structured under stable headings. Headings are **advisory and
never parsed for correctness** — malformed content is still valid Markdown, so a bad
hand-edit degrades rather than breaks.

- Shared by every Character Instance.
- Watched for external modification and reloaded.
- A single timestamped backup written before a wipe.
- Treated as **untrusted input**: it reaches Harness prompts and the user can type
  anything into it.

Per-Instance memory may become configurable later. Not built now.

## Acceptance criteria

- A remembered fact round-trips through the file.
- An external edit is picked up without restarting ai-buddy.
- Malformed content still loads and preserves what it can.
- Wipe writes a backup before clearing.
- A hand-written file ai-buddy has never touched loads correctly.
- Writes are visible to the user rather than silent.

## Tests

Store tests against a temporary file covering every criterion above.
