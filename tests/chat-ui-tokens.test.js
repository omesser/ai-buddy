// ADR-0019's seam: a colour, font family or radius written outside a
// `.chat-ui-*` block breaks it quietly, since the next design cannot recolour
// it. `rgb()` and `rgba()` are checked too, or white tints would slip back in.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");
const settings = readFileSync(new URL("../src/settings.css", import.meta.url), "utf8");

// The stylesheet with every design block removed, as `{line, text}` per line.
// Depth-counted, not matched against a brace in column zero, since an indented
// design block defeated the shell hook this replaced. Blank lines keep line numbers.
function outsideDesignBlocks(source) {
  const lines = source.split("\n");
  const kept = [];
  let depth = 0;
  let skipping = false;

  for (const [index, line] of lines.entries()) {
    const opens = (line.match(/{/g) ?? []).length;
    const closes = (line.match(/}/g) ?? []).length;

    // Start skipping when we see .chat-ui- at depth 0, OR when we see
    // @media at depth 0 (media queries for light/dark mode variants)
    if (!skipping && depth === 0 && (line.includes(".chat-ui-") || line.includes("@media")) && opens > 0) {
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

// A declaration that hard-codes what a token should carry. `var()` and
// comments come out first, which is what lets the patterns stay this blunt: a
// rule that reads its token reduces to `border-radius: ;` and matches nothing.
const LITERALS = [
  [/#[0-9A-Fa-f]{3}/, "a hex colour"],
  [/rgba?\(/, "an rgb() colour"],
  [/font-family:[^;]*[A-Za-z]/, "a font family"],
  [/border-radius:[^;]*[0-9]/, "a radius"],
];

// chat-ui.css is where the designs themselves live, so its own `.chat-ui-*`
// blocks are the one exemption. settings.css carries no design: it reads the
// tokens and nothing else, so every line of it is held to the rule.
const SHEETS = [
  ["chat-ui.css", outsideDesignBlocks(css)],
  ["settings.css", settings.split("\n").map((text, index) => ({ line: index + 1, text }))],
];

test("every colour, font and radius outside a design block reads a token", () => {
  const offenders = [];

  for (const [sheet, lines] of SHEETS) {
    for (const { line, text } of lines) {
      const bare = text.replace(/var\([^)]*\)/g, "").replace(/\/\*[^*]*\*\//g, "");
      for (const [pattern, what] of LITERALS) {
        if (pattern.test(bare)) {
          offenders.push(`${sheet}:${line} carries ${what}: ${text.trim()}`);
        }
      }
    }
  }

  assert.deepEqual(
    offenders,
    [],
    `a second design cannot reach these, so they would survive the recolour:\n${offenders.join("\n")}`,
  );
});

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
