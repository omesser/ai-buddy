// A skill that tells an agent to run a script it cannot find is worse than one
// that says nothing: the agent invents a workaround and reports success. The
// failure is silent, so it needs a check rather than a reader.
//
// Vendored skills name their helpers relative to the skill directory —
// `scripts/watch-pr/watch-pr` means
// `.agents/skills/poteto-mode/scripts/watch-pr/watch-pr`. An agent sitting at
// the repository root resolves that to nothing and falls back to `gh`. This
// resolves a reference the way a reader should: beside the file, then under the
// skill, then from the repository root.

import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));
const SKILLS = ".agents/skills";

// A reference worth checking is one an agent would run. Documentation links
// rot differently and are not what breaks a playbook mid-task.
const RUNNABLE = /\.(sh|bash|ps1|py|js|mjs|cjs|ts|rb|pl)$/;
const UNDER_SCRIPTS = /(^|\/)(scripts|helpers)\//;

// Path-shaped tokens, optionally anchored with `./` or `../`.
const TOKEN = /(?:\.{1,2}\/)?[A-Za-z0-9_.][A-Za-z0-9_.\-]*(?:\/[A-Za-z0-9_.\-]+)+/g;

// Prose that looks like a path and is not one. Each entry is a token this scan
// cannot tell from a real reference by shape alone, with the reason it is
// exempt. The same `name|reason` convention as EXCLUDED in
// `scripts/sync-pstack.sh`: the record of why is the point.
const NOT_A_REFERENCE = new Map([
  [
    "scripts/snapshot.sh",
    "An example cell inside the decision-log sample table in show-me-your-work/SKILL.md, not a helper it ships.",
  ],
  [
    "src/triage/handler.ts",
    "An illustrative path in a worked example in triage/AGENT-BRIEF.md; it names the reader's codebase, not this one.",
  ],
  [
    "src/types.ts",
    "Same worked example in triage/AGENT-BRIEF.md.",
  ],
  [
    "src/types/skill.ts",
    "Same worked example in triage/AGENT-BRIEF.md.",
  ],
]);

const walk = (dir, out = []) => {
  for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
    if (entry.name === "node_modules") continue;
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) walk(path, out);
    else if (entry.name.endsWith(".md")) out.push(path);
  }
  return out;
};

// A URL's path segments tokenize exactly like a repository path, and a CDN
// link is the commonest false hit. Blank them before scanning rather than
// growing the exemption list every time upstream cites one.
const withoutUrls = (line) => line.replace(/\b[a-z][a-z0-9+.-]*:\/\/\S+/gi, " ");

const referencesIn = (file) => {
  const skill = file.split("/")[2];
  const found = new Map();
  for (const raw of readFileSync(join(root, file), "utf8").split("\n")) {
    const line = withoutUrls(raw);
    for (const match of line.matchAll(TOKEN)) {
      const token = match[0].replace(/[).,;:'"]+$/, "");
      // A trailing `*` is a family of scripts named in prose, not one path.
      if (line[match.index + match[0].length] === "*") continue;
      if (!(RUNNABLE.test(token) || UNDER_SCRIPTS.test(token))) continue;
      if (NOT_A_REFERENCE.has(token)) continue;
      found.set(token, { token, skill, file });
    }
  }
  return [...found.values()];
};

// Upstream occasionally writes a path from its own plugin root rather than the
// skill, as in `pstack/skills/poteto-mode/scripts/check-plan.mjs`. Vendoring
// flattens that prefix away, so map it back to the skill directory.
const FROM_PLUGIN_ROOT = /^[\w.-]+\/skills\/([\w.-]+)\/(.+)$/;

// Beside the file, then under the skill, then from the repository root, then
// under the skill upstream's own layout names. A vendored skill writes one of
// the middle two; a repo-owned skill writes the third.
const resolveRef = ({ token, skill, file }) => {
  const upstream = FROM_PLUGIN_ROOT.exec(token);
  const candidates = [
    ["beside the file", resolve(root, dirname(file), token)],
    ["under the skill", join(root, SKILLS, skill, token)],
    ["from the repository root", join(root, token)],
  ];
  if (upstream) {
    candidates.push([
      "under the skill, minus upstream's plugin-root prefix",
      join(root, SKILLS, upstream[1], upstream[2]),
    ]);
  }
  return candidates.find(([, candidate]) => existsSync(candidate));
};

const files = walk(SKILLS);

test("the skills tree holds markdown to scan", () => {
  assert.ok(files.length > 0, `${SKILLS} has no markdown; every assertion below would pass over nothing`);
});

test("every runnable path a skill names resolves somewhere", () => {
  const refs = files.flatMap(referencesIn);
  assert.ok(refs.length > 0, "no runnable references found; the scan has stopped matching anything");

  const dangling = refs
    .filter((ref) => !resolveRef(ref))
    .map((ref) => `  ${ref.token}  (named by ${ref.file})`);

  assert.deepEqual(
    dangling,
    [],
    `A skill names a script that is not here, so an agent following it invents a workaround:\n${dangling.join("\n")}\n` +
      `Vendor the file, or add the token to NOT_A_REFERENCE in ${relative(root, fileURLToPath(import.meta.url))} with the reason it is prose.`,
  );
});

// The one this check was written for. Losing it is how the babysit playbook
// silently degrades to `gh` polling.
test("the babysit playbook's PR watcher is vendored and executable", () => {
  const watcher = `${SKILLS}/poteto-mode/scripts/watch-pr/watch-pr`;
  assert.ok(
    existsSync(join(root, watcher)),
    `${watcher} is gone. playbooks/babysit.md step 6 names it as scripts/watch-pr/watch-pr. Run scripts/sync-pstack.sh --fetch.`,
  );
});
