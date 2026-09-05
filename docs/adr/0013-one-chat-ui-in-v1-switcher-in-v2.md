# One chat UI ships in v1 and the switcher waits for v2

A **Chat UI** is a named, swappable visual design for the chat surface:
palette, type scale, and shape. v1 ships one — modern minimal, from #339 — and
the seam that makes it swappable, but no way for a user to swap it. v2 adds the
switcher over #339's three, and stays open to more.

The single definition of the visual language is `src/chat-ui.css`: a `:root`
block naming the tokens, one `.chat-ui-*` block per design setting them. No
part of the chat surface names a colour, font family, or radius literal outside
a `.chat-ui-*` block. There is no second artifact.

The scope is the chat surface and nothing else — not "the webview", which today
means the overlays: one per display, all built by `build_overlay` from
`WebviewUrl::default()`, all loading `src/index.html`, which draws sprites and
speech bubbles and no chat. The chat surface is a second webview window
(ADR-0010), with its own document and stylesheet, that #17 has yet to build.

## The seam is one class and one token file

v1 owes the token names in `src/chat-ui.css` and `class="chat-ui-minimal"` on
the root element of the chat window's own document, which #17 creates. No
registry, no `chat_ui` key in `settings.rs`, no Character Manifest key, no
stylesheet loaded at runtime. Those are the switcher's.

Because the surface does not exist yet, the tokens ship with it rather than
being extracted from anything. Nothing is lifted out of `src/main.css` or
`src/bubble.js`: those are the overlay's, and a seam born clean in a new window
beats one retrofitted into the stylesheet that draws Characters.

A seam with one implementation and no test is speculative generality. This one
has three, and not hypothetically: `docs/design/chat-mockups.html` on #339's
branch runs modern minimal, terminal log and pane of glass off one window and
one rendering engine, at 53, 62 and 52 CSS rules each, switched by

```js
desk.classList.remove("v-min", "v-term", "v-glass");
desk.classList.add(b.id);
```

That is the whole mechanism, and the page is the evidence that one skeleton
holds for three designs that look nothing alike. v1 lifts the first and keeps
the seam it was drawn against.

The check is a grep, not a second design in the product: no hex colour,
`font-family` or `border-radius` literal in the chat surface's own stylesheet
outside a `.chat-ui-*` block. A design that cannot recolour the surface is a
surface that stopped reading tokens, and that is the only way this seam breaks.

## Surfaces

The chat surface (#17) is in. It is unbuilt, so it is born in the chat UI.

- **Everything the overlay draws** — out. The sprite is a Character's art,
  integer-scaled (ADR-0006), with no chrome to style. The speech bubble and
  thinking ellipsis (`src/bubble.js`, #119) are drawn in that same overlay
  window, above the sprite and beside the art: they are the Character's
  expression rather than the chat surface's chrome, and they keep the palette
  #119 shipped. Cues (`src/cue.js`) are tuned to read over arbitrary wallpaper
  and arbitrary Character art, not against window chrome (#277).
- **Tray icon and menus** — out. The system draws them; `tray.rs` and
  `tauri::menu` are portable by #197's constraint and expose nothing to style.
- **Pages under `docs/design/`** — out. A Dated page is frozen at its approval
  date (ADR-0011); a proposal that silently followed `chat-ui.css` would stop
  being the thing that was approved.
- **The Settings window** — deliberately not addressed here and still
  undecided: it is hand-written AppKit and GTK that `chat-ui.css` cannot reach,
  and nothing in this ADR says whether the visual language extends to it.

## The unshipped two survive as a Dated page

Terminal log and pane of glass exist only on `design/chat-surface-mockups`. A
branch is not a review surface, so v2's switcher would have one design and two
memories. #339 merges as a Dated page carrying its issue number and approval
date — the class ADR-0011 defined for exactly this — and that page is what
carries the other two designs to v2.

## The switcher is an application setting

One chat UI for the whole app: one row in the settings form, one key in
`settings.rs`.

Per-Character is closed already — #119 ruled out per-Character bubble styling,
and ADR-0002 keeps the Manifest declaring Behaviors. Per-Instance is the
tempting one now that #338 has made per-Instance state live, and it is still
wrong: an Instance's identity is its Character's art, already per-Instance and
already the thing the user looks at. Per-Instance chrome would make one product
look like two on one screen to distinguish buddies that are already distinct.

## Considered Options

- **Ship the switcher in v1.** Three designs to keep working through every
  chat-surface change, before the chat surface has shipped once.
- **A chat UI per Character, declared in the Character Manifest.** It would
  make every third-party Character Package a UI author.
- **Tokens with no root class**, recolouring `:root` in place. It holds for one
  design and collapses at two: the three mockups differ in radius, density and
  type as much as in palette, and a scope-free stylesheet has nowhere to put a
  rule that applies to one design only.
- **A root class with no tokens**, one full stylesheet per design. Three
  independent copies of every rule, which is how three designs become three
  products.

## Consequences

`src/chat-ui.css` does not exist, and neither does the surface it dresses.
Both arrive with #17's chat window; the grep above is what keeps the literals
out afterwards. The overlay is untouched: `src/main.css` and `src/bubble.js`
keep the colours and fonts they shipped with.

CONTEXT.md gains **Chat UI** beside **Chat surface**, so the design has a word
of its own that is not read as another name for the window.

v2's switcher depends on #339 merging. If it does not, this decision loses the
two designs it promises to offer.

Reversing this means shipping the chat surface with its colours written inline,
and revisiting modularity once a second design is wanted.
