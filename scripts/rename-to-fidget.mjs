#!/usr/bin/env node
// Rename lever for the product name. Re-run from the repo root:
//   node scripts/rename-to-fidget.mjs inventory [scripts/rename-inventory.tsv]
//   node scripts/rename-to-fidget.mjs apply
//   node scripts/rename-to-fidget.mjs verify
//
// Three roles, applied in table order. The script is the classification.
// Product display is Fidget. Product slug is fidget. The on-screen companion
// is character. A GitHub repo coordinate stays ai-buddy.

import { readdirSync, readFileSync, writeFileSync, mkdirSync, unlinkSync, rmdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const SKIP_DIRS = new Set([".git", "target", "node_modules", ".verify", "dist"]);
const SKIP_EXT = new Set([
  ".png", ".gif", ".ico", ".icns", ".zip", ".gz", ".woff", ".woff2",
  ".pdf", ".wav", ".mp3", ".ogg", ".webp", ".jar", ".exe", ".dll", ".so", ".dylib",
]);

// Files that must keep the old spellings: this lever, its fixture test, and
// reports that quote the matches they found.
export const ALLOWLIST = new Set([
  "scripts/rename-to-fidget.mjs",
  "tests/rename-to-fidget.test.js",
  "scripts/rename-inventory.tsv",
  ".audit/rename-fidget.tsv",
]);

// Longest forge coordinate first so a URL is one span, not a slash plus a slug.
const FORGE = [
  /https?:\/\/github\.com\/omesser\/ai-buddy\b/g,
  /https?:\\\/\\\/github\\.com\\\/omesser\\\/ai-buddy\b/g,
  /git@github\.com:omesser\/ai-buddy\b/g,
  /github\\.com\\\/omesser\\\/ai-buddy\b/g,
  /github\.com\/omesser\/ai-buddy\b/g,
  /omesser\\.github\\.io\\\/ai-buddy\b/g,
  /omesser\.github\.io\/ai-buddy\b/g,
  /omesser\\\/ai-buddy\b/g,
  /\bomesser\/ai-buddy\b/g,
];

const CHARACTER_NAME = [/Buddy Bot/g, /buddy-bot/g];

// Exact rows run after shields and before the generic slug/display split.
// Each row is [from, to, role].
const EXACT = [
  [
    "_Avoid_: Pet, mascot, avatar, buddy (the app is the buddy, not the character)",
    "_Avoid_: Pet, mascot, avatar, Fidget (Fidget is the product, not a Character)",
    "product-display",
  ],
  // The GitHub project is still titled with the old name. Point at the board
  // without copying that title into prose that verify would have to keep.
  ["**AI Buddy** project board", "project board", "product-display"],
  ["the AI Buddy project board", "the project board", "product-display"],
  ["dev.omesser.ai-buddy", "dev.omesser.fidget", "product-slug"],
  ['"productName": "ai-buddy"', '"productName": "Fidget"', "product-display"],
  ["<title>ai-buddy", "<title>Fidget", "product-display"],
  ['.tooltip("ai-buddy")', '.tooltip("Fidget")', "product-display"],
  // A bundle directory is a filename. The original was lowercase `ai-buddy.app`.
  ["ai-buddy.app", "fidget.app", "product-slug"],
  // Consent fallbacks are the name macOS and the settings pane show.
  ['"ai-buddy".into()', '"Fidget".into()', "product-display"],
  ['.title("ai-buddy")', '.title("Fidget")', "product-display"],
  ['Title: "ai-buddy"', 'Title: "Fidget"', "product-display"],
  ['title: "ai-buddy"', 'title: "Fidget"', "product-display"],
  ['el.title !== "ai-buddy"', 'el.title !== "Fidget"', "product-display"],
  ['is_toolchain("ai-buddy")', 'is_toolchain("Fidget")', "product-display"],
  ["participant Buddy as ai-buddy", "participant Fidget as Fidget", "product-display"],
  // describe_standing echoes the window owner, which the fixture sets to the
  // binary slug. The sentence is "a {owner} window", so the slug stays lowercase.
  ["a ai-buddy window", "a fidget window", "product-slug"],
  // GDK capitalizes the binary name: ai-buddy's WM_CLASS is Ai-buddy.
  ["Ai-buddy", "Fidget", "product-display"],
  ["buddy-vs-no-buddy", "fidget-vs-no-fidget", "product-slug"],
  ["buddy-side", "fidget-side", "product-slug"],
  ["multi-buddy", "multi-character", "character-generic"],
  ["Buddy Cues", "Fidget Cues", "product-display"],
  ["Buddy Bubble", "Fidget Bubble", "product-display"],
  ["Buddy Chat", "Fidget Chat", "product-display"],
  ["Buddy\u2013Harness", "Fidget\u2013Harness", "product-display"],
  ["buddy-harness-two-way", "fidget-harness-two-way", "product-slug"],
  ["AI Buddy", "Fidget", "product-display"],
  ["AiBuddy", "Fidget", "product-display"],
  ["AI_BUDDY", "FIDGET", "product-slug"],
  ["ai_buddy", "fidget", "product-slug"],
];

const PRODUCT_NEEDLES = [
  /ai-buddy/g,
  /Ai-buddy/g,
  /AI Buddy/g,
  /AI_BUDDY/g,
  /AiBuddy/g,
  /ai_buddy/g,
  /Buddy Cues/g,
  /Buddy Bubble/g,
  /Buddy Chat/g,
  /buddy-side/g,
  /buddy-vs-no-buddy/g,
  /multi-buddy/g,
];
const GENERIC_BUDDY = [/\bbuddies\b/g, /\bBuddies\b/g, /\bbuddy's\b/g, /\bBuddy's\b/g, /\bbuddy\b/g, /\bBuddy\b/g];

function shield(text, rules, role, slots) {
  let out = text;
  for (const re of rules) {
    const copy = new RegExp(re.source, re.flags);
    out = out.replace(copy, (match) => {
      const id = `\u0000REN${slots.length}\u0000`;
      slots.push({ id, text: match, role });
      return id;
    });
  }
  return out;
}

function unshield(text, slots) {
  let out = text;
  for (const slot of [...slots].reverse()) out = out.replaceAll(slot.id, slot.text);
  return out;
}

function slugOrDisplay(before, after, next = "") {
  if (before === "-" || after === "-" || before === "_" || after === "_") return "fidget";
  if ("/\\`.".includes(before) || "/\\`".includes(after)) return "fidget";
  // A token wrapped in quotes is a slug. 's after the token is possessive
  // prose (Fidget's), not a closing quote.
  if ((before === '"' || before === "'") && (after === '"' || after === "'")) {
    if (after === "'" && next === "s") return "Fidget";
    return "fidget";
  }
  return "Fidget";
}

// Flags and subcommands whose next word is the package, binary, process, or
// MCP server id. Prose ("lets ai-buddy read") is not in this set.
const SLUG_PREV = new Set(["-p", "--bin", "--package", "-x", "-P", "add", "remove"]);

function previousWord(whole, offset) {
  const head = whole.slice(0, offset).trimEnd();
  const match = head.match(/(\S+)$/);
  return match ? match[1] : "";
}

function linePrefix(whole, offset) {
  const lineStart = whole.lastIndexOf("\n", offset - 1) + 1;
  return whole.slice(lineStart, offset);
}

function productToken(whole, offset) {
  const before = offset > 0 ? whole[offset - 1] : "";
  const after = offset + 8 < whole.length ? whole[offset + 8] : "";
  const next = offset + 9 < whole.length ? whole[offset + 9] : "";
  // pass:ai-buddy, O=ai-buddy. A trailing period or colon in prose is not a slug.
  if (":=".includes(before)) return "fidget";
  // ai-buddy.exe, ai-buddy.window-names. "not by ai-buddy." stays display.
  if (after === "." && /[A-Za-z0-9_]/.test(next)) return "fidget";
  // ai-buddy://windows. "outside ai-buddy:" stays display.
  if (after === ":" && next === "/") return "fidget";
  // A YAML key whose value is the MCP server id: a line that is only `ai-buddy:`.
  if (after === ":" && /^[ \t]*$/.test(linePrefix(whole, offset))) return "fidget";
  if (SLUG_PREV.has(previousWord(whole, offset))) return "fidget";
  // `claude mcp add --transport http ai-buddy` — the server id is not the word after `add`.
  const start = whole.lastIndexOf("\n", offset - 1) + 1;
  const end = whole.indexOf("\n", offset);
  const line = whole.slice(start, end === -1 ? whole.length : end);
  if (/\bmcp (add|remove)\b/.test(line)) return "fidget";
  return slugOrDisplay(before, after, next);
}

function replaceProductSlug(text) {
  return text.replace(/ai-buddy/g, (_match, offset, whole) => productToken(whole, offset));
}

function fixArticles(text) {
  return text
    .replace(/\bAn Fidget\b/g, "A Fidget")
    .replace(/\ban Fidget\b/g, "a Fidget")
    .replace(/\bAn fidget\b/g, "A fidget")
    .replace(/\ban fidget\b/g, "a fidget");
}

function replaceCompanion(text) {
  const word = (from, to) => text.replace(from, (match, offset, whole) => {
    const before = offset > 0 ? whole[offset - 1] : "";
    const after = offset + match.length < whole.length ? whole[offset + match.length] : "";
    // Path segments and hyphenated ids (/Users/buddy/, buddy-1, buddy-bot).
    if ("/-_".includes(before) || "/-_".includes(after)) return match;
    return to;
  });
  text = word(/\bbuddies\b/g, "characters");
  text = word(/\bBuddies\b/g, "Characters");
  text = word(/\bbuddy's\b/g, "character's");
  text = word(/\bBuddy's\b/g, "Character's");
  text = word(/\bbuddy\b/g, "character");
  // Capital Buddy left in place is a Character's own name (Buddy Bot is
  // shielded; Buddy One is a fixture name). Product nicknames are exact rows.
  return text;
}

export function transform(text) {
  const slots = [];
  let out = shield(text, FORGE, "forge-slug", slots);
  out = shield(out, CHARACTER_NAME, "character-name", slots);
  for (const [from, to] of EXACT) out = out.replaceAll(from, to);
  out = replaceProductSlug(out);
  out = fixArticles(out);
  out = replaceCompanion(out);
  return unshield(out, slots);
}

// Title case only in user-facing docs, UX copy, and headings. A lowercase
// `ai-buddy` in technical prose, paths, and identifiers stays a lowercase slug.
const DISPLAY_FILES = new Set([
  "README.md",
  "DESIGN.md",
  "CONTEXT.md",
  "docs/SPEC.md",
  "branding/README.md",
  "src-tauri/tauri.conf.json",
  "src-tauri/src/tray.rs",
  "tests/product-identity.test.js",
]);
const DISPLAY_PREFIXES = [
  "docs/design/",
  "src-tauri/src/settings",
  "tests/fixtures/settings-",
  "spike/settings-webview/",
  "characters/",
];
const UX_FILES = new Set([
  "src-tauri/src/main.rs",
  "src-tauri/src/consent.rs",
  "src-tauri/src/harness.rs",
  "src-tauri/src/chat_surface.rs",
  "tests/ax-settings-win-finder.test.js",
  "tests/verify-anchor-offscreen-win.test.js",
  "tests/settings.test.js",
  "tests/settings-clipboard.test.js",
  "tests/chat-status.test.js",
]);

function fileKind(rel) {
  if (DISPLAY_FILES.has(rel) || DISPLAY_PREFIXES.some((prefix) => rel.startsWith(prefix))) return "display";
  if (rel.startsWith("src/") || UX_FILES.has(rel)) return "ux";
  return "technical";
}

function isHeading(line, rel) {
  const trimmed = line.trim();
  if (rel.endsWith(".md") && /^#{1,6}\s/.test(trimmed)) return true;
  return /<title>/.test(line) || /<h[1-6][\s>]/.test(line);
}

function isComment(line, rel) {
  const trimmed = line.trim();
  if (trimmed.startsWith("//") || trimmed.startsWith("/*") || trimmed.startsWith("<!--")) return true;
  return /\.(sh|py|toml|yaml|yml)$/.test(rel) && trimmed.startsWith("#");
}

function isLog(line) {
  return /console\.(warn|error|log|info|debug)\s*\(/.test(line) || /eprintln!/.test(line) || /println!/.test(line);
}

// Spans that are the product as people see it, or the GDK class of the binary.
const KEEP_CASE = [
  /"Fidget"\.into\(\)/g,
  /\.tooltip\("Fidget"\)/g,
  /\.title\("Fidget"\)/g,
  /"productName": "Fidget"/g,
  /Title: "Fidget"/g,
  /title: "Fidget"/g,
  /title !== "Fidget"/g,
  /--class 'Fidget'/g,
  /WM_CLASS `Fidget`/g,
  /`Fidget` class/g,
  /`Fidget` window/g,
  /is_toolchain\("Fidget"\)/g,
  /Fidget ·/g,
];

function lowercaseProduct(line) {
  const slots = [];
  const shielded = shield(line, KEEP_CASE, "keep", slots);
  return unshield(shielded.replace(/\bFidget\b/g, "fidget"), slots);
}

function lowercaseMcpName(line) {
  const command = /\bmcp (add|remove)\b/.test(line) || /^\s*Fidget\s+"/.test(line);
  if (!command) return line;
  return line.replace(/\bFidget\b/g, "fidget");
}

// Paths and filenames are the slug, including in a heading or a UX sentence.
// `docs/Fidget/notes.md`, `scripts/Fidget.sh`, and `Fidget.app` all become `fidget`.
function lowercasePathNames(line) {
  return line
    .replace(/(?<=[\\/])Fidget(?=[\\/"'\s]|$)/g, "fidget")
    .replace(/\bFidget(?=\.[A-Za-z0-9])/g, "fidget");
}

export function recase(text, rel) {
  const kind = fileKind(rel);
  // The GitHub path stays omesser/ai-buddy. The local directory is the product slug.
  const rewritten = text.replace(
    /git clone https:\/\/github\.com\/omesser\/ai-buddy\.git\ncd (?:Fidget|ai-buddy)\b/g,
    "git clone https://github.com/omesser/ai-buddy.git fidget\ncd fidget",
  );
  let fence = false;
  return rewritten.split("\n").map((line) => {
    if (rel.endsWith(".md") && /^```/.test(line.trim())) fence = !fence;
    let next = line.replace(/\bcd Fidget\b/g, "cd fidget");
    next = next.replace(/\bFidget-/g, "fidget-");
    next = lowercasePathNames(next);
    next = lowercaseMcpName(next);
    // A `#` line inside a fence is a shell comment, not a document heading.
    if (fence) return lowercaseProduct(next);
    if (isHeading(next, rel)) return next;
    if (isComment(next, rel) || isLog(next)) return lowercaseProduct(next);
    if (kind === "technical") return lowercaseProduct(next);
    return next;
  }).join("\n");
}

export function transformPath(rel) {
  const posix = rel.split(path.sep).join("/");
  if (ALLOWLIST.has(posix)) return posix;
  let next = posix.replaceAll("buddy-harness-two-way", "fidget-harness-two-way");
  next = next.replaceAll("ai-buddy", "fidget");
  next = next.replaceAll("ai_buddy", "fidget");
  next = next.replaceAll("AI_BUDDY", "FIDGET");
  next = next.replaceAll("AI Buddy", "fidget");
  next = next.replace(/^Fidget(?=[\\/.]|$)/, "fidget");
  return lowercasePathNames(next);
}

function coveredBy(line, index, end, rules) {
  for (const re of rules) {
    const copy = new RegExp(re.source, re.flags);
    let match;
    while ((match = copy.exec(line))) {
      const start = match.index;
      const stop = start + match[0].length;
      if (index >= start && end <= stop) return true;
      if (match[0].length === 0) break;
    }
  }
  return false;
}

function adjacentKept(line, index, length) {
  const before = index > 0 ? line[index - 1] : "";
  const after = index + length < line.length ? line[index + length] : "";
  return "/-_".includes(before) || "/-_".includes(after);
}

export function classifyLine(line) {
  const hits = [];
  const consider = (re, kind) => {
    const copy = new RegExp(re.source, re.flags);
    let match;
    while ((match = copy.exec(line))) {
      const index = match.index;
      const text = match[0];
      let role = kind;
      if (coveredBy(line, index, index + text.length, FORGE)) role = "forge-slug";
      else if (kind === "companion" && coveredBy(line, index, index + text.length, CHARACTER_NAME)) role = "character-name";
      else if (kind === "companion" && adjacentKept(line, index, text.length)) role = "character-name";
      else if (kind === "companion" && /^Buddy$/.test(text)) role = "character-instance";
      else if (kind === "companion") role = "character-generic";
      else if (kind === "product" && text === "AI Buddy") role = "product-display";
      else if (kind === "product" && (text === "ai-buddy" || text === "ai_buddy" || text === "AI_BUDDY")) {
        role = text === "ai-buddy" && productToken(line, index) === "Fidget" ? "product-display" : "product-slug";
      } else if (kind === "product") role = "product-display";
      hits.push({ role, text, index });
      if (text.length === 0) break;
    }
  };
  for (const re of PRODUCT_NEEDLES) consider(re, "product");
  for (const re of GENERIC_BUDDY) consider(re, "companion");
  return hits;
}

export function failingHits(rel, line, lineNo) {
  if (ALLOWLIST.has(rel)) return [];
  return classifyLine(line)
    .filter((hit) => hit.role === "product-display" || hit.role === "product-slug" || hit.role === "character-generic")
    .map((hit) => ({ path: rel, line: lineNo, role: hit.role, text: hit.text }));
}

function walk(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const ent of entries) {
    if (ent.isSymbolicLink()) continue;
    const abs = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (SKIP_DIRS.has(ent.name)) continue;
      walk(abs, out);
    } else if (ent.isFile()) out.push(abs);
  }
  return out;
}

function relOf(abs, root) {
  return path.relative(root, abs).split(path.sep).join("/");
}

function textFile(abs) {
  const ext = path.extname(abs).toLowerCase();
  if (SKIP_EXT.has(ext)) return false;
  const buf = readFileSync(abs);
  if (buf.includes(0)) return false;
  return buf.toString("utf8");
}

export function inventory(root = ROOT) {
  const rows = [];
  for (const abs of walk(root)) {
    const rel = relOf(abs, root);
    if (ALLOWLIST.has(rel)) continue;
    const text = textFile(abs);
    if (text === false) continue;
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) {
      for (const hit of classifyLine(lines[i])) {
        rows.push({ path: rel, line: i + 1, role: hit.role, text: hit.text });
      }
    }
  }
  return rows;
}

function casingHits(rel, text) {
  if (ALLOWLIST.has(rel)) return [];
  const next = recase(text, rel);
  if (next === text) return [];
  const hits = [];
  const before = text.split("\n");
  const after = next.split("\n");
  for (let i = 0; i < before.length; i++) {
    if (before[i] !== after[i]) hits.push({ path: rel, line: i + 1, role: "product-case", text: before[i].trim() });
  }
  return hits;
}

export function verify(root = ROOT) {
  const hits = [];
  for (const abs of walk(root)) {
    const rel = relOf(abs, root);
    const text = textFile(abs);
    if (text === false) continue;
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) hits.push(...failingHits(rel, lines[i], i + 1));
    hits.push(...casingHits(rel, text));
  }
  return hits;
}

function apply(root = ROOT) {
  const files = walk(root);
  let rewritten = 0;
  let renamed = 0;
  for (const abs of files) {
    const rel = relOf(abs, root);
    const nextRel = transformPath(rel);
    const raw = textFile(abs);
    if (raw === false) {
      if (nextRel !== rel) throw new Error(`refusing to rename binary ${rel}`);
      continue;
    }
    const next = ALLOWLIST.has(rel) ? raw : recase(transform(raw), rel);
    const dest = path.join(root, nextRel);
    if (next !== raw || nextRel !== rel) {
      mkdirSync(path.dirname(dest), { recursive: true });
      const mode = statSync(abs).mode;
      writeFileSync(dest, next, { mode });
      rewritten += next === raw ? 0 : 1;
      if (nextRel !== rel) {
        unlinkSync(abs);
        renamed += 1;
      }
    }
  }
  prune(root, root);
  return { rewritten, renamed };
}

function prune(dir, root) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const ent of entries) {
    if (ent.isSymbolicLink() || !ent.isDirectory()) continue;
    if (SKIP_DIRS.has(ent.name)) continue;
    prune(path.join(dir, ent.name), root);
  }
  if (dir === root) return;
  const rel = path.relative(root, dir).split(path.sep).join("/");
  let left;
  try {
    left = readdirSync(dir);
  } catch {
    return;
  }
  if (left.length === 0 && transformPath(rel) !== rel) rmdirSync(dir);
}

function writeInventory(rows, dest) {
  const header = "path\tline\trole\ttext\n";
  const body = rows.map((row) => `${row.path}\t${row.line}\t${row.role}\t${row.text}`).join("\n");
  writeFileSync(dest, header + body + (body ? "\n" : ""));
}

function counts(rows) {
  const tally = {};
  for (const row of rows) tally[row.role] = (tally[row.role] || 0) + 1;
  return tally;
}

const command = process.argv[2];
if (command === "inventory") {
  const rows = inventory();
  const dest = process.argv[3];
  if (dest) writeInventory(rows, path.resolve(dest));
  console.log(JSON.stringify(counts(rows)));
  console.log(`hits ${rows.length}`);
} else if (command === "apply") {
  const before = verify();
  const result = apply();
  console.log(JSON.stringify(result));
  console.log(`residuals-before ${before.length}`);
} else if (command === "verify") {
  const hits = verify();
  const dest = process.argv[3];
  if (dest) writeInventory(hits, path.resolve(dest));
  if (hits.length) {
    for (const hit of hits.slice(0, 80)) {
      console.error(`${hit.path}:${hit.line}\t${hit.role}\t${hit.text}`);
    }
    console.error(`residual product or companion wording: ${hits.length}`);
    process.exit(1);
  }
  console.log("verify ok");
} else if (command) {
  console.error("use inventory, apply, or verify");
  process.exit(2);
}
