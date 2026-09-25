// What a permission ask says, as parts the consent row draws. Its own
// module because chat.js reaches window.__TAURI__ as it loads and cannot be
// imported outside a webview; this can, so it has a test.

// Everything but the copy here is untrusted: `title`, `content`, `input` and
// `locations` come from the Harness, and an MCP server can steer all four. The
// caller writes each part with `textContent` and this file produces no markup.

// How much of an ask the row may draw, in characters: about eleven wrapped
// lines in a 420-point window, enough for a question and its arguments, short
// enough that the answer buttons stay on screen. `input` is arbitrary JSON.
const DETAIL_LIMIT = 600;

// How many arguments the row names, and how much of each value. A call with
// more arguments than this has a shape to summarize rather than a payload to
// print, and the count of what was left says the rest.
const ARGUMENTS = 6;
const VALUE_LIMIT = 120;

// How many paths the row names before it counts them instead. Three fits the
// line; the count is what matters past that, because a tool touching a dozen
// files is a different decision than one touching three.
const PATHS = 3;

// An ask that carried nothing to describe itself. Said as a sentence, so it
// reads as an absence the Harness is responsible for rather than as detail
// this window dropped.
const SILENT = "The Harness asked for permission without saying what for.";

// One untrusted string, flattened to plain text on one line: a newline, tab or
// bidi override inside a tool's arguments could forge a fact the Harness never
// sent. `\p{C}` is every control and format character; `\s` finishes the whitespace.
export function flat(text) {
  return String(text)
    .replace(/\p{C}+/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
}

export function clamp(text, limit) {
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text;
}

// The first few of a list, with a count of the rest rather than the rest.
function first(list, keep, noun) {
  return list.length > keep
    ? [...list.slice(0, keep), `and ${list.length - keep} more ${noun}`]
    : list;
}

// The arguments as `key: value` lines rather than a JSON dump: the names are
// what tell a reader what the tool will do with the values. Anything that is
// not an object has no names to show, so it goes as the one line of JSON it is.
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

function withinBudget(title, details, metadata) {
  const parts = [
    { kind: "title", value: title },
    ...details.map(({ text, code }) => ({ kind: "detail", value: text, code })),
    { kind: "metadata", value: metadata },
  ].filter(({ value }) => value);
  const shown = [];
  let remaining = DETAIL_LIMIT;
  for (const part of parts) {
    const available = remaining - (shown.length ? 1 : 0);
    if (available <= 0) {
      break;
    }
    const value = part.value.slice(0, available);
    shown.push({ kind: part.kind, value, code: part.code });
    remaining = available - value.length;
    if (value.length < part.value.length) {
      break;
    }
  }
  if (shown.length < parts.length || shown.at(-1)?.value.length < parts.at(-1)?.value.length) {
    const last = shown.at(-1);
    last.value = `${last.value.slice(0, -1)}…`;
  }
  return {
    title: shown.find(({ kind }) => kind === "title")?.value ?? "",
    details: shown.filter(({ kind }) => kind === "detail").map(({ value, code }) => ({ text: value, code })),
    metadata: shown.find(({ kind }) => kind === "metadata")?.value ?? "",
  };
}

export function askSays(ask) {
  // The title is untrusted too, and a verbose one would spend the row's budget
  // before the question arrived, from a merely chatty server.
  const title = clamp(flat(ask?.title ?? ""), VALUE_LIMIT);
  const content = (ask?.content ?? []).map(flat).filter(Boolean);
  // Content takes precedence over repeated input. It can be a prose question;
  // only execute content is a command. Fallback arguments are always code.
  const details = content.length > 0
    ? content.map((text) => ({ text, code: ask?.kind === "execute" }))
    : argumentLines(ask?.input).map((text) => ({ text, code: true }));

  const paths = (ask?.locations ?? [])
    .map((where) => clamp(flat(where), VALUE_LIMIT))
    .filter(Boolean);
  const kind = flat(ask?.kind ?? "");
  // `other` is `ToolKind::Other`, which says only that the Harness declined to
  // classify the call. Every other kind separates reading from writing from
  // running, the distinction the answer turns on, so it rides at the end.
  const about = [kind === "other" ? "" : kind, first(paths, PATHS, "paths").join(", ")]
    .filter(Boolean)
    .join(" · ");

  // A kind on its own is not an answer to "what am I approving": it names a
  // category, and the whole bug was a row that offered one in place of the
  // question. A path on its own is a fact worth drawing.
  if (!title && details.length === 0 && paths.length === 0) {
    return { title: SILENT, details: [], metadata: "" };
  }
  return withinBudget(title, details, about);
}

// An elicitation form's question. Same flattening as a permission ask: the
// Harness wrote `message`, and a newline inside it must not forge a line.
export function elicitSays(form) {
  const message = clamp(flat(form?.message ?? ""), DETAIL_LIMIT);
  return message || "The Harness asked a question without saying what for.";
}
