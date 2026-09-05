// Run with `node --test tests/`.
//
// The seam ADR-0013 ships: `src/chat-ui.css` is the one definition of the
// visual language, a design is one `.chat-ui-*` block, and adding a second
// design is adding a second block and nothing else. That only holds while the
// rest of the stylesheet reads tokens, so a colour, font family or radius
// written outside a design block is the seam breaking, and it breaks quietly —
// the surface still renders, and the next design simply cannot recolour it.
//
// `rgb()` and `rgba()` are checked beyond the three the ADR names, because most
// of modern minimal's palette is white tints and forbidding hex alone would let
// every one of them back in.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

/// The stylesheet with every design block removed, as `{line, text}` per line.
///
/// Depth-counted rather than matched against a brace in column zero, which is
/// what this check did while it was a shell hook: indenting a design block was
/// enough to make it skip the whole file and pass over anything. Blank lines
/// stand in for what is removed so the line numbers a failure reports are the
/// line numbers in the file.
function outsideDesignBlocks(source) {
  const lines = source.split("\n");
  const kept = [];
  let depth = 0;
  let skipping = false;

  for (const [index, line] of lines.entries()) {
    const opens = (line.match(/{/g) ?? []).length;
    const closes = (line.match(/}/g) ?? []).length;

    if (!skipping && depth === 0 && line.includes(".chat-ui-") && opens > 0) {
      skipping = true;
    }

    kept.push({ line: index + 1, text: skipping ? "" : line });
    depth += opens - closes;

    if (skipping && depth === 0) {
      skipping = false;
    }
  }

  return kept;
}

/// A declaration that hard-codes what a token should carry.
///
/// `var()` and comments come out first, which is what lets the patterns stay
/// this blunt: a rule that reads its token reduces to `border-radius: ;` and
/// matches nothing.
const LITERALS = [
  [/#[0-9A-Fa-f]{3}/, "a hex colour"],
  [/rgba?\(/, "an rgb() colour"],
  [/font-family:[^;]*[A-Za-z]/, "a font family"],
  [/border-radius:[^;]*[0-9]/, "a radius"],
];

test("every colour, font and radius outside a design block reads a token", () => {
  const offenders = [];

  for (const { line, text } of outsideDesignBlocks(css)) {
    const bare = text.replace(/var\([^)]*\)/g, "").replace(/\/\*[^*]*\*\//g, "");
    for (const [pattern, what] of LITERALS) {
      if (pattern.test(bare)) {
        offenders.push(`chat-ui.css:${line} carries ${what}: ${text.trim()}`);
      }
    }
  }

  assert.deepEqual(
    offenders,
    [],
    `these sit outside a .chat-ui-* block, so a second design cannot reach them:\n${offenders.join("\n")}`,
  );
});

// The guard above is only worth having if it still sees a design block that was
// reformatted, which is the case that defeated its shell version.
test("a design block is skipped however it is indented", () => {
  const indented = "  .chat-ui-minimal {\n    color: #fff;\n  }\n";
  const outside = outsideDesignBlocks(indented)
    .map((entry) => entry.text)
    .join("");

  assert.equal(outside, "", "an indented design block was read as ordinary rules");
});

test("a literal outside a design block is caught", () => {
  const leaked = ".bar {\n  color: #fff;\n}\n";
  const found = outsideDesignBlocks(leaked).some(({ text }) =>
    LITERALS.some(([pattern]) => pattern.test(text)),
  );

  assert.equal(found, true, "the check stopped catching a hex colour");
});
