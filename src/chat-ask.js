// What a permission ask says, as the text the consent row draws.
//
// Its own module because chat.js reaches window.__TAURI__ as it loads and
// cannot be imported outside a webview; this can, so it has a test. It earns
// one: this row is the only place the product asks the user to make a security
// decision, and leading with the tool kind while withholding the question is
// the shape that teaches a reflexive Allow (#678).
//
// Everything but the copy here is untrusted. `title`, `content`, `input` and
// `locations` come from the Harness and an MCP server can steer all four. The
// caller writes the result with `textContent` and this file produces no
// markup: keep both true. It is not Markdown either: a consent row is the last
// surface that should honour someone else's formatting.

// How much of an ask the row may draw, in characters.
//
// The Chat surface is 420 by 560 points (`main.rs`), so `.said` wraps near 55
// characters and this is about eleven wrapped lines: enough for a question and
// its arguments, short enough that the answer buttons stay on screen. `input`
// is arbitrary JSON, so without a bound one ask could push them out of reach.
//
// It bounds the whole of what the row says, `about` included: a kind and three
// 120-character paths otherwise landed after the question had spent the budget.
const DETAIL_LIMIT = 600;

// How many arguments the row names, and how much of each value.
//
// A call with more arguments than this has a shape to summarize rather than a
// payload to print, and the count of what was left says the rest.
const ARGUMENTS = 6;
const VALUE_LIMIT = 120;

// How many paths the row names before it counts them instead. Three fits the
// line; the count is what matters past that, because a tool touching a dozen
// files is a different decision than one touching three.
const PATHS = 3;

// An ask that carried nothing to describe itself. Said as a sentence, so it
// reads as an absence the Harness is responsible for rather than as detail
// this window dropped — which is what `other: (untitled)` read as.
const SILENT = "The Harness asked for permission without saying what for.";

// One untrusted string, flattened to plain text on one line.
//
// The row's own structure is a line per fact, so a newline, a tab or a bidi
// override inside a tool's arguments could forge a fact the Harness never
// sent: a reassuring `edit · /safe/path` under a question that writes
// somewhere else. Collapsing them leaves the text readable and the structure
// ours.
//
// `\p{C}` rather than a list of the characters that do it: it is every
// control and every format character, which is the whole class of things that
// take up no width and change what the rest looks like. `\s` finishes the job
// on the ones that are merely whitespace.
function flat(text) {
  return String(text)
    .replace(/\p{C}+/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function clamp(text, limit) {
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text;
}

// The first few of a list, with a count of the rest rather than the rest.
function first(list, keep, noun) {
  return list.length > keep
    ? [...list.slice(0, keep), `and ${list.length - keep} more ${noun}`]
    : list;
}

// The arguments as `key: value` lines rather than a JSON dump: the argument
// names are what tell a reader what the tool will do with the values. Anything
// that is not an object — a bare string, an array — has no names to show, so
// it goes as the one line of JSON it is.
function argumentLines(input) {
  if (input === null || input === undefined) {
    return [];
  }
  if (typeof input !== "object" || Array.isArray(input)) {
    return [clamp(flat(JSON.stringify(input)), VALUE_LIMIT)];
  }
  const named = Object.entries(input).map(([key, raw]) => {
    const shown = typeof raw === "string" ? raw : (JSON.stringify(raw) ?? String(raw));
    return `${flat(key)}: ${clamp(flat(shown), VALUE_LIMIT)}`;
  });
  return first(named, ARGUMENTS, "arguments");
}

export function askSays(ask) {
  // The title is untrusted too, and a verbose one would spend the row's budget
  // before the question arrived — #678 again, from a merely chatty server
  // rather than a hostile one.
  const said = [clamp(flat(ask?.title ?? ""), VALUE_LIMIT)];
  const content = (ask?.content ?? []).map(flat).filter(Boolean);
  // Content first: it is where a question's own words arrive. The arguments
  // are the fallback, and never both — a tool that sends its question as
  // content usually repeats it in `input`, and the row has no room to say
  // anything twice.
  said.push(...(content.length > 0 ? content : argumentLines(ask?.input)));

  const paths = (ask?.locations ?? [])
    .map((where) => clamp(flat(where), VALUE_LIMIT))
    .filter(Boolean);
  const kind = flat(ask?.kind ?? "");
  // `other` is `ToolKind::Other`, which says only that the Harness declined to
  // classify the call. Every other kind separates reading from writing from
  // running something, which is the distinction the answer turns on — so it
  // rides at the end, after what is actually being asked, and never instead.
  const about = [kind === "other" ? "" : kind, first(paths, PATHS, "paths").join(", ")]
    .filter(Boolean)
    .join(" · ");

  const body = said.filter(Boolean).join("\n");
  // A kind on its own is not an answer to "what am I approving": it names a
  // category, and the whole bug was a row that offered one in place of the
  // question. A path on its own is a fact worth drawing.
  if (body === "" && paths.length === 0) {
    return SILENT;
  }
  return clamp([body, about].filter(Boolean).join("\n"), DETAIL_LIMIT);
}
