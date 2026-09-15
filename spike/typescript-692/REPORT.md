# Spike 692: should the webview move from JavaScript to TypeScript?

Run in a detached worktree at `68e3fe41`. No production code changed. This pull request
commits the report and the prototypes under `spike/`, and nothing else.

## Answer

Stay on JavaScript files. Add a type check, not a build.

- Run `tsc --noEmit` over `src/` with JSDoc types (the issue's option 1). It is about
  30 small fixes, one config file, and one CI step. It already found a crash.
- Generate the Rust payload types with `ts-rs` from a `cargo test` that fails when the
  committed copy is stale (the issue's option 2, but generated, not hand-written). It
  costs six dev-only crates and eighteen attribute lines. A hand-written `.d.ts`
  would only buy autocomplete.
- Do not take option 3. Nothing measured here needs a build step, and it would put a
  `tsc` run in front of every `cargo build` and `cargo test`.
- Start with `src/settings.js` from #734. It is new, and it reads the largest payload
  this app has. Retrofit `chat.js` and `main.js` with the 30 fixes; do not rewrite them
  mid-migration.

## What the type check found on the first run

`tsc --noEmit` with `checkJs` over the twelve modules, `strict` off, implicit `any` and
null checks off:

| Class | Count | What it is |
|---|---|---|
| `window.__TAURI__` untyped | 10 | one global declaration removes all ten |
| DOM narrowing | 16 | `getElementById` gives `HTMLElement`; `.value`, `.disabled`, `.closest` need a JSDoc cast each |
| `webkitAudioContext` | 1 | a `/** @type {any} */` on `cue.js:38` |
| tuple inference | 1 | `chat.js:392`, a `[string, boolean]` array read as `(string \| boolean)[]` |
| vendor stub artefact | 1 | `markdown.js` imports `Lexer`; the stub below needs that name |
| **real bug** | **1** | see below |
| **Total** | **30** | |

With `strict` on the count is 337, nearly all "implicitly has an `any` type". That is
the cost of typing every parameter, and it is not the cheap first step.

**The real bug.** `src/chat.js:561` assigns `loginSaid = null` and nothing declares
`loginSaid`. #563 (`b597885a`) removed the declaration and left this line. In an ES
module an assignment to an undeclared name throws `ReferenceError`, so the
`chat-session` handler dies before it writes its "New session — …" note. Every earlier
line in that handler (clearing the log, the strip, the asks) still runs. Not verified
live; verified by reading the module semantics and the git history.

That became #737 and shipped in #738, with `tests/no-undeclared-assignment.test.js` as a
guard. Re-run against `main` at `32a8d28d`, the loose config now reports 29: that one
line is gone and the other 29 are unchanged. The logs in this directory are the original
run, before the fix.

**Vendored code.** `tsc` follows `markdown.js`'s import into `src/vendor/marked.esm.js`
and reports 273 errors there. `exclude` does not stop it. A nine-line
`src/vendor/marked.esm.d.ts` beside the vendored file does; `tsc` prefers a `.d.ts` to a
sibling `.js`. It would be served raw like everything in `src/` and never loaded by a
browser. The stub is at `marked.esm.d.ts.stub`; copy it into `src/vendor/` to reproduce
the numbers above.

## Has boundary drift caused bugs, and would types have caught them?

**History.** 31 commits changed one of the seven structs the webview listens to
(`ChatOpening`, `ChatHarness`, `ChatReply`, `ChatStatus`, `PermissionAsk`,
`SpritePlacement`, `Settled`). 30 of them also changed a `src/*.js` file in the same
commit. The one that did not (#322) changed internals, not the wire. The shape "Rust
changed, JavaScript forgot" does not appear in this repo's history.

**The incidents the coordinator named** are real but a different shape:

- #654 changed `select_harness` from `Result<(), String>` to `Result<String, String>`.
  A reviewer walked every call site by hand. A type checker would not have flagged
  anything: JavaScript that ignores a return is fine whether the return is `void` or
  `string`.
- #659 and #678: `ChatHarness` (`main.rs:1106`) carries `name`, `login`, `alive`,
  `session` and no `missing`, so Chat says "not running" where Settings says "not
  installed". Still on `main`, documented in a comment at `src/chat-status.js:170`. A
  type checker cannot flag a field that nobody reads because it does not exist. Types
  say what is there, not what should be.

So the boundary argument is weaker than the issue framed it. What the boundary types do
catch is the mechanical class: a misread name, wrong case, a renamed or removed field,
wrong nullability, an unknown event or command name, a wrong argument key. The probe
in `probe/misread.js` makes six such mistakes on purpose and `tsc` reports all six
(`tsconfig.probe.json`). Nothing in the history shows that class has bitten yet.

What did pay on the first run was the plain type check over 2,529 lines of JavaScript
that no test reaches: the `loginSaid` crash has nothing to do with the boundary.

## Can the payload types be generated?

Yes, cheaply. `ts-rs-proto/` is a standalone crate that copies the real payload
shapes and derives `ts_rs::TS` only under `#[cfg(test)]`, so the shipped binary carries
none of it.

- A test writes `generated/payloads.d.ts` when `REFRESH_BINDINGS=1` is set and
  otherwise compares the committed file to a fresh render. This is the same shape as
  #729's `both_ai_sources_serialize_to_the_committed_fixtures` in `form.rs:1606`.
- Renaming `alive` to `missing` in Rust fails the test with the refresh command in the
  message. Measured.
- Cold build 6.9 s, warm 0.2 s. Six dev-only crates join the tree: `ts-rs`,
  `ts-rs-macros`, `thiserror`, `thiserror-impl`, `termcolor`, `syn 2` (`syn 3` is already
  there). Nothing at runtime.
- Lifetimes (`SpritePlacement<'a>`), `Option`, `Vec`, `BTreeMap`, `&'static str` and
  `#[serde(tag = "type")]` all render correctly. `#[serde(tag = "type")]` matters
  because `FormRow` in `form.rs` uses it and is what `settings.js` will read.
- One gotcha: `u64` renders as `bigint`, but the JSON wire carries a number. Without a
  fix `chat.js:661` (`payload.wake_ms + …`) is a false error.
  `#[cfg_attr(test, ts(type = "number | null"))]` on the two `Option<u64>` fields fixes
  it. Measured.
- `serde_json::Value` renders as `JsonValue` and needs that alias declared once.
- A second gotcha, found while committing this: the generator must end the file with
  exactly one newline. ts-rs's per-type render leaves a trailing blank line,
  `end-of-file-fixer` trims it on commit, and the next `cargo test` then fails against a
  fixture the lint itself rewrote. One `trim_end` in the generator settles it. Any
  generated file this repository commits has the same constraint.
- The committed `Cargo.lock` needed a `.pre-commit-config.yaml` fix, included here: all
  three `Cargo.lock` excludes were anchored at the repo root, so `pretty-format-toml`
  reformatted this nested lock and cargo rewrote it back on the next build.

Commands are not generated. `tauri-boundary.d.ts` hand-writes the 8 event names and
15 command names with their payloads, and types `window.__TAURI__` through a global
declaration, so `src/` needs no edit for `listen` and `invoke` to be checked.
`tauri-specta` would generate the command half too, but it is a runtime dependency and
a registration step, and 15 lines did not earn it. Not prototyped.

With the generated payloads plus that declaration on top of the loose config, the
count over the real `src/` drops from 30 to 20: the ten `__TAURI__` errors go, and no
new boundary error appears. Today's JavaScript reads the payloads correctly.

## What each option costs

| | Option 1: JSDoc + `tsc --noEmit` | Option 2: generated payload types | Option 3: full TypeScript |
|---|---|---|---|
| What ships | unchanged | unchanged | `dist/`, `frontendDist` moves |
| `package.json` | unchanged (`npx -y -p typescript@5`) | unchanged | probably unchanged, but a lockfile question |
| Files added | `tsconfig.json`, `src/vendor/marked.esm.d.ts` | `types/payloads.d.ts` (generated), `types/tauri.d.ts` | `dist/` in `.gitignore`, watch config |
| Files edited | ~30 one-line JSDoc fixes across `chat.js`, `main.js`, `cue.js`, `markdown.js` | `src-tauri/Cargo.toml` (one dev-dep), ~18 `#[cfg_attr(test, derive(TS))]` lines, one test | rename 12 modules, every import in `tests/`, `tauri.conf.json`, `release.yml` |
| CI | one step, ~2 s cold `npx` + 1.4 s `tsc` | none new; `cargo test` already runs | `tsc` before every `cargo build`/`cargo test`, because Tauri embeds `frontendDist` at compile time |
| Local loop | none | `REFRESH_BINDINGS=1 cargo test` after editing a payload struct | `tsc -w` beside `cargo tauri dev` |
| `tests/` | unchanged | unchanged | imports change to `.ts`. Node 22.18+ strips types with no loader; CI's `lts/*` and Homebrew Node 26 pass, the local nvm Node 20 fails with `ERR_UNKNOWN_FILE_EXTENSION` (measured) |
| Pre-commit | one `npx` hook, or none (CI is enough for a 3 s check) | none | build hook — which `docs/agents/writing.md` says pre-commit is not for |

Option 3's one real cost is the ordering: `cargo test` in `src-tauri` would need a
built `dist/`, so every Rust-only pull request pays for the JavaScript build. Not
verified in this run that Tauri fails hard on a missing `frontendDist` directory; that
is from its documented behaviour.

## #706 changes the timing, in one direction

Settings is the largest payload this app will have: `SettingsSnapshot` is
`FormDescription` (tabs, sections, a nine-variant tagged `FormRow`) plus `SettingsView`
(28 fields). #734's `settings.js` and its tests already read `.form.tabs`. That file is
new and has no 30 errors to fix, so it is the right first user of a generated
`SettingsSnapshot` type. Chat and the overlay should get the config and the 30 fixes,
nothing more, until #706 is done.

## Adjacent

#723 owns the dependency half (when the webview earns a bundler). Nothing here needs
one; if #723 adds one anyway, option 3's build cost is already paid and the answer here
can be revisited.

## Not verified

- The `loginSaid` crash in the running app.
- `ts-rs` on the real `src-tauri` crate; the prototype copies the shapes into a
  standalone crate to avoid a cold Tauri build.
- Tauri's behaviour with a missing `frontendDist` directory.
- `tauri-specta`.
- `npx` availability on the Windows runner for a pre-commit hook.

## Files in this directory

- `tsconfig.option1.json`, `tsconfig.option1-loose.json` — option 1, strict and loose.
- `tsc.option1.strict.txt` (337), `tsc.option1.loose.txt` (30) — the runs. Named
  `.txt` because the root `.gitignore` skips `*.log`.
- `marked.esm.d.ts.stub` — copy to `src/vendor/marked.esm.d.ts` to reproduce.
- `tauri-boundary.d.ts`, `tsconfig.option2.json`, `tsc.option2.txt` (20) — option 2.
- `probe/misread.js`, `tsconfig.probe.json` — six deliberate misreads, six errors.
- `ts-rs-proto/` — the generator: `src/lib.rs`, `generated/payloads.d.ts`,
  `REFRESH_BINDINGS=1 cargo test` to regenerate.
