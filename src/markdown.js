// Markdown in a reply, drawn as formatting instead of as its punctuation
// (#677). Parsing belongs here rather than in the Shell for the reason
// `src/main.js` and `CONTEXT.md` give about every other drawing decision: the
// Shell names what was said, the webview draws it. Formatting is derived from
// the text and carries no authority, so it is drawing, and the next view
// decision — highlighting a fence, collapsing one, a copy button — stays in
// this file instead of becoming a wire change.
//
// The parser is `marked` (`src/vendor/`), and only its token API is used.
// Its HTML-string renderer is never called: a reply is untrusted text — a
// model's output, steerable by an MCP server's content — so every piece of it
// reaches the DOM through `createElement` and `textContent` on a node built
// here. That makes injection structurally impossible rather than sanitized,
// which is the property #371 is about and the one `src/chat.js` had for free
// while replies were a single text node.

import { Lexer } from "./vendor/marked.esm.js";

// GFM, because that is what harnesses emit: Claude Code's output style asks
// for tables and task lists, Codex emits fenced code with an info string, and
// Zed — the reference ACP client — renders agent messages with
// `pulldown-cmark`'s table, strikethrough, task-list and footnote extensions
// on. CommonMark alone would draw a harness's table as a run of pipes.
const FLAVOUR = { gfm: true, breaks: false, pedantic: false };

// Schemes a link target may carry. Nothing here navigates — the app has no
// opener plugin, and a live `href` would take the chat webview itself to the
// page — but the target is written into the DOM for the opener that will read
// it, and an allowlist means `javascript:` or `data:` never gets that far.
// Checked as a scheme rather than searched for as a string: `JaVaScRiPt:`,
// `&#106;avascript:` and `java<tab>script:` all fail to be one of these three,
// so each is refused without needing its own rule.
const SCHEMES = new Set(["http:", "https:", "mailto:"]);

function target(href) {
  const trimmed = (href ?? "").trim();
  const scheme = /^[a-z][a-z0-9+.-]*:/i.exec(trimmed);
  return scheme && SCHEMES.has(scheme[0].toLowerCase()) ? trimmed : null;
}

// Whatever `marked` grew a token for and this file does not draw reads as the
// source the model wrote — an image, a stretch of raw HTML, a footnote. That
// is what the surface did for every construct before #677, so an unsupported
// one is a line that looks unformatted rather than a line that vanished.
function asSource(token, parent, doc) {
  parent.append(doc.createTextNode(token.raw ?? token.text ?? ""));
}

function inline(tokens, parent, doc) {
  for (const token of tokens) {
    switch (token.type) {
      case "text":
      case "escape":
        parent.append(doc.createTextNode(token.text));
        break;
      case "strong":
      case "em":
      case "del":
        wrap(token.type, token, parent, doc);
        break;
      case "codespan": {
        const code = doc.createElement("code");
        code.textContent = token.text;
        parent.append(code);
        break;
      }
      case "br":
        parent.append(doc.createElement("br"));
        break;
      case "link": {
        const where = target(token.href);
        // A refused target leaves the link text behind rather than the row:
        // the words a model wrote are still what it said.
        const node = doc.createElement("span");
        if (where) {
          node.className = "md-link";
          node.dataset.href = where;
          node.title = where;
        }
        inline(token.tokens ?? [], node, doc);
        parent.append(node);
        break;
      }
      default:
        asSource(token, parent, doc);
    }
  }
}

function wrap(tag, token, parent, doc) {
  const node = doc.createElement(tag);
  inline(token.tokens ?? [], node, doc);
  parent.append(node);
}

function row(cells, tag, doc) {
  const node = doc.createElement("tr");
  for (const cell of cells) {
    const td = doc.createElement(tag);
    // `align` is one of three words `marked` derives from the delimiter row,
    // never text the model chose, and it names a class so the alignment stays
    // in the stylesheet with the rest of the table.
    if (cell.align) {
      td.className = `md-${cell.align}`;
    }
    inline(cell.tokens ?? [], td, doc);
    node.append(td);
  }
  return node;
}

function blocks(tokens, parent, doc) {
  for (const token of tokens) {
    switch (token.type) {
      // Blank lines, and a link definition that already produced its link.
      case "space":
      case "def":
        break;
      case "paragraph":
        wrap("p", token, parent, doc);
        break;
      case "text":
        // A tight list item's line, which `marked` hands over as a block-level
        // `text` token holding the inline ones. It belongs in the item with no
        // paragraph of its own, so it flattens into the parent.
        inline(token.tokens ?? [token], parent, doc);
        break;
      case "heading":
        wrap(`h${token.depth}`, token, parent, doc);
        break;
      case "blockquote": {
        const quote = doc.createElement("blockquote");
        blocks(token.tokens ?? [], quote, doc);
        parent.append(quote);
        break;
      }
      case "hr":
        parent.append(doc.createElement("hr"));
        break;
      case "list": {
        const list = doc.createElement(token.ordered ? "ol" : "ul");
        if (token.ordered && token.start > 1) {
          list.start = token.start;
        }
        for (const item of token.items) {
          const li = doc.createElement("li");
          blocks(item.tokens ?? [], li, doc);
          list.append(li);
        }
        parent.append(list);
        break;
      }
      case "checkbox": {
        // Drawn and disabled. A live box would claim this surface can change
        // what the reply says, and it cannot: the reply is a log line.
        const box = doc.createElement("input");
        box.type = "checkbox";
        box.checked = Boolean(token.checked);
        box.disabled = true;
        parent.append(box);
        break;
      }
      case "code": {
        // A fence and a table are the two blocks that do not reflow, and the
        // Chat window opens 420 points wide (`src-tauri/src/main.rs`). The
        // scroll goes on a wrapper so the overflow stays inside the row
        // instead of widening it.
        const fence = doc.createElement("div");
        fence.className = "md-wide";
        const pre = doc.createElement("pre");
        const code = doc.createElement("code");
        // The info string is the model's, so it is filtered to a word before
        // it names a class — the conventional hook a highlighter would read.
        const lang = /^[\w+#.-]+/.exec(token.lang ?? "");
        if (lang) {
          code.className = `language-${lang[0]}`;
        }
        code.textContent = token.text;
        pre.append(code);
        fence.append(pre);
        parent.append(fence);
        break;
      }
      case "table": {
        const scroll = doc.createElement("div");
        scroll.className = "md-wide";
        const table = doc.createElement("table");
        const head = doc.createElement("thead");
        head.append(row(token.header ?? [], "th", doc));
        const body = doc.createElement("tbody");
        for (const cells of token.rows ?? []) {
          body.append(row(cells, "td", doc));
        }
        table.append(head, body);
        scroll.append(table);
        parent.append(scroll);
        break;
      }
      default:
        asSource(token, parent, doc);
    }
  }
}

// The reply drawn in each row so far, so a chunk can be added to it. Keyed on
// the element because that is what the surface holds; a removed row takes its
// entry with it.
const drawn = new WeakMap();

// Draw the whole of `text` into `body`, replacing whatever is there, and put
// the caret back where it was found.
//
// The caret is found and put back with `querySelector` and `append`,
// deliberately never `insertBefore`. A previous attempt at this threw
// `DOMException: The child can not be found in the parent` on the second
// chunk, because it nested the caret inside a block while inserting against a
// reference node it assumed was a direct child of `body`. With no reference
// node there is nothing to be wrong about, the lookup is a descendant search
// so the caret's depth does not matter, and `append` moves a node that still
// has a parent rather than needing it taken out first.
export function drawReply(body, text, doc = globalThis.document) {
  const caret = body.querySelector(".caret");
  drawn.set(body, text ?? "");
  body.replaceChildren();
  blocks(Lexer.lex(text ?? "", FLAVOUR), body, doc);
  if (caret) {
    // Inside the last paragraph, so it blinks at the end of the line rather
    // than on one of its own. Anywhere else — a fence, a table, an empty
    // reply — it goes back to the row, where it is at least valid.
    const last = body.lastElementChild;
    (last && last.tagName === "P" ? last : body).append(caret);
  }
}

// One chunk of a streaming reply.
//
// The row is redrawn from the whole reply rather than having the chunk
// appended to it, because a chunk lands mid-marker: a renderer fed `**bo` and
// then `ld**` sees two things that are neither of them Markdown. Re-parsing
// costs nothing today — the Shell accumulates the answer and `ChatReply`
// carries it in one emit — and it is still right when chunks arrive.
export function appendReply(body, chunk, doc = globalThis.document) {
  drawReply(body, (drawn.get(body) ?? "") + chunk, doc);
}
