# Comments

Comments should say why, not what. We strive to keep them short and informative.
A comment earns every line it takes.

## What earns the words

A reason, a constraint, a rejected alternative, or a bound that looks arbitrary
and is not. From `crates/core/src/character.rs`:

```rust
/// How long a Personality Prompt may be, in characters.
///
/// A bound rather than a preference: the prompt is untrusted text that goes
/// into every Character Prompt the Director sends, so an unbounded one is a way
/// to spend a user's tokens and to bury the sensing context under prose.
/// Generous enough for a paragraph of personality.
pub const PERSONALITY_LIMIT: usize = 2000;
```

Six lines for a constant, and every one is load-bearing. Without them, 2000 is
a number the next reader is free to change.

## What does not

A comment that restates the code. From a one-line getter this repository once
shipped (`src-tauri/src/cast.rs`, dissolved in #94 — the example outlives the
file):

```rust
/// Every Animation's frames as `data:` URLs, for the webview to draw from.
///
/// Handed over once. The webview picks a frame out of this by the name and
/// index each `Frame` carries, so it still holds no authoritative state —
/// only the art, which is not state.
pub fn art(&self) -> &BTreeMap<String, Vec<String>> {
```

The module doc above it already said why the frames are `data:` URLs, and
`src/main.js` already says the webview owns no state. The first line is the
whole comment:

```rust
/// Every Animation's frames as `data:` URLs, for the webview to draw from.
```

Delete the restatement. If the code needed it, simplify the code instead.

## History goes in the commit message

Not how the bug was found, not what the code looked like before. `git log`
keeps that, and keeps it better.

The exception is a comment that stops the next reader putting the bug back.
This one, from `crates/core/src/snapshot.rs`, reads as history and is not:

```rust
/// #39: the sprite used to come to rest at the bottom of the display, which
/// is behind the Dock — the Dock draws above the overlay, so three quarters
/// of the art disappeared under it. [...] The fix is upstream of the Engine: the
/// rectangles it is handed are the usable part of each display, so the floor
/// it already derives is the Dock's top edge.
```

That is a constraint on where the fix belongs, and it is why the test exists.

An issue number is worth the four characters. `crates/core/src/engine.rs` ends
a paragraph with `#39.` and points at the whole argument without retelling it.
The retelling is what to leave out.

## `ponytail:` comments

The sanctioned exception. A deliberate shortcut with a known ceiling names the
ceiling and the upgrade path, and that is worth the words. From
`crates/core/src/memory.rs`:

```rust
/// ponytail: seconds since the epoch rather than a civil timestamp. It sorts
/// correctly and costs no date library; swap it for an ISO stamp if one ever
/// arrives for another reason. Two wipes in the same second share a name, and
/// the later one wins.
```

The ceiling, what it costs, and the condition under which it changes. Without
all three it is a shortcut nobody can price.
