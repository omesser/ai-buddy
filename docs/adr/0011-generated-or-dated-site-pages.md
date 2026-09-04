# Every page on the published site is Generated or Dated

Every page on `omesser.github.io/ai-buddy` is one of two classes. Nothing else
may exist.

A **Generated page** is built at deploy time by a script under `scripts/` from
repository data. The page's source is the thing it describes, and malformed
input fails the build.

A **Dated page** is a hand-written proposal, frozen, carrying its issue number
and approval date. A Dated page never claims to be current. The reader knows
they are looking at September.

The forbidden third class is the **Described page**: hand-written prose claiming
to describe shipped behavior. That is the page that is worse than no page. A
Described page and the code it describes share no link a build can check, so
they agree until one of them changes, and nothing then tells the reader which
one they are holding. Forbidding the class answers the maintenance question by
construction instead of by promise.

## The site is a review surface

The site is where one design question at a time is argued, for @omesser and
whoever is arguing it. The README stays the project's front door.

A user-facing landing page — what the app is, screenshots, install links — is
rejected. Two front doors drift, and the README wins the drift because that is
where a contributor already is. If someone wants that page, it is a separate
issue rather than growth of this one.

A page earns its place when the thing it documents is timed, audible, or
operated, and github.com can only give you a still or a paragraph.
`docs/design/cues.html` plays a 660 Hz tone falling to 600 over a 140 ms decay
(`src/cue.js:111`); you cannot hear a table. #289's gallery plays the nine
Animations of the Required Animation Set at the fps each Character Manifest
declares; GitHub renders a PNG, not eight frames at 12 fps.

## `cues.html` is Described today

`docs/design/index.html` calls it "The spec the Shell implements".
`src/cue.js:4` calls itself "Ported value for value from the reviewed proposal
in docs/design/cues.html". Nothing checks that the two agree.

There are two ways out, and this records both rather than picking one, because
neither has an owner yet:

- Relabel the page as a Dated proposal — the #277 proposal as approved, not the
  spec.
- Have `.github/workflows/pages.yml` copy `src/cue.js` and `src/main.css` beside
  the page, and have the page load those instead of its own inlined copies.

The second is the recommendation. It is honest by construction and adds no build
step. One open question decides it: whether that `main.css` can load on that
page without fighting its layout. Its cue rules run from line 188 to the end of
a stylesheet that also styles the overlay and the speech bubble.

## Three pages, flat

The index, `cues.html`, and #289's Character gallery. Flat still holds at five,
because grouping three or five entries adds a level of navigation and removes
none.

A fourth page is not commissioned. One exists when a design question arrives
that has to be heard or operated before it can be answered.

The gallery shows every installed Character, `bmo`, `jotaro-kujo` and `trump`
included, and it is Generated. It is also the one page on the site meant to be
found in search, so it does not carry the `noindex` the proposal pages do. That
is a deliberate choice to publish those Characters on a page meant to be found.

Every proposal page gets `<meta name="robots" content="noindex">`. No page on
`main` carries it yet; this decides that they will. The site is public with no
access control, and noindex stops a superseded proposal becoming the canonical
search result for the thing it proposed.

## No versioning tree

One file per design question, revised in place. Git holds the history and the
deployment is built from the commit being deployed, so a superseded proposal
does not stay reachable on the site. The issue number and approval date on the
page are what a reader needs, not a `/v1/` path.

One exception, default off: if a merged issue links a proposal that is later
replaced, freeze a copy under its own name at that point.

## Navigation

Each index entry gains one line naming the page's class — `Generated from
characters/ at deploy`, or `Proposal, approved in #277`. That puts the whole
versioning answer at the front door.

Each page repeats that line in its own header with a link to its issue, because
a page reached from search arrives without the index.

## The allowlist changes its unit

`.github/workflows/pages.yml` names every file it copies, and that list is the
security model. #289's gallery needs a few hundred PNGs, too many to name as
reviewed lines in a workflow, so the unit changes. The workflow names every file
*and* every generated tree, one line per generator. The generator decides what
enters its own tree and is reviewed as code rather than as a diff line. A guard
after assembly fails the build if `_site` holds anything but `.html`, `.css`,
`.js`, `.png` or `.svg`.

This is a weakening and it is taken on purpose: the guarantee moves from a
reviewed line in a diff to a reviewed script. The guard restores most of it for
one line. The failure the allowlist exists to prevent is `docs/research/` and
`docs/agents/` getting crawlable URLs, and every one of those files is `.md`. A
generator is constrained in review to enumerate and copy named files, never to
copy a source directory wholesale.

## Considered Options

- **A real landing page** — what the app is, screenshots, install links. Two
  front doors that drift, and the README wins the drift.
- **Mirroring `docs/SPEC.md`, the ADRs and `CONTEXT.md`.** github.com renders
  them and their cross-links work. Jekyll does not run, so a mirror needs a
  build step in order to be worse.
- **A downloads or install page.** GitHub Releases is that page and it updates
  itself.
- **A Perch and physics sandbox.** A second implementation of `crates/core`
  physics in JavaScript. It cannot be generated from the Rust, so it is
  guaranteed to drift. The best-looking idea here and the clearest reject.
- **A Behavior and Primitive vocabulary explorer.** It is a table, and tables
  are Markdown's home turf.
- **An in-browser wasm demo of the app.** A wasm build of the Engine is its own
  project, and the payoff is a toy that misrepresents a thing whose whole point
  is sitting on top of your other windows.
- **A version archive of superseded proposals.** Machinery for a problem two
  pages do not have.

## Consequences

The gallery needs a script under `scripts/` and the allowlist change above.
`cues.html` needs an owner for one of its two fixes. Neither is this ADR's work;
this decision builds nothing.

Two things are wrong today, recorded as recommendations rather than changes.

`docs/design/index.html` overstates `cues.html` as the spec the Shell
implements. Either of the two fixes above settles it.

#289 says `characters/` holds seven Character Packages. When this was written it
held eight — `timber-wolf` landed in `fed87105` and `buddy-bot` in #305, both
after that sentence was last edited. The count in that issue has been wrong
twice now, which is the Generated rule argued in one line: a hand-written
gallery would already be wrong about the thing it exists to show.

Reversing this means allowing a Described page, and accepting that each one
drifts from the code it describes with nothing to catch it.
