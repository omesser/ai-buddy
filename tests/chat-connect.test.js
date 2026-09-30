import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  canAnswer,
  composerPlaceholder,
  inlineSegments,
  landingCopy,
  loginPresentation,
} from "../src/chat-connect.js";

const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

const http = {
  name: "bmo",
  configured: true,
  enabled: true,
};

function npxOpening(name) {
  return {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: name,
    harness: {
      name,
      session: null,
      alive: false,
      login: null,
      missing: "npx",
    },
  };
}

test("chat.js paints the landing from the helper, and does not celebrate a pick", () => {
  assert.match(js, /import \{ canAnswer, composerPlaceholder, drawInline, landingCopy \} from "\.\/chat-connect\.js"/);
  assert.match(js, /canAnswer\(opening\)/);
  assert.match(js, /landingCopy\(opening\)/);
  assert.doesNotMatch(js, /is the AI brain now/);
});

test("HTTP Completer mode can answer when configured and on", () => {
  assert.equal(canAnswer(http), true);
  assert.equal(composerPlaceholder(http), "Ask bmo…");
});

test("a missing npx launcher cannot answer, for every adapter preset", () => {
  for (const name of ["claude", "codex", "pi"]) {
    const opening = npxOpening(name);
    assert.equal(canAnswer(opening), false, name);
    assert.equal(composerPlaceholder(opening), "Nothing can answer yet", name);
    const copy = landingCopy(opening);
    assert.match(copy.title, /needs `npx`/, name);
    assert.match(copy.lede, /`npx` is not installed/, name);
    assert.match(copy.lede, /does not bundle `npx`/, name);
    assert.equal(copy.command, null, name);
  }
});

test("a missing first-party CLI names that binary, not npx", () => {
  for (const name of ["copilot", "cursor-agent", "goose", "grok", "hermes", "opencode"]) {
    const opening = {
      name: "bmo",
      configured: true,
      enabled: true,
      harness_name: name,
      harness: {
        name,
        session: null,
        alive: false,
        login: null,
        missing: name,
      },
    };
    const copy = landingCopy(opening);
    assert.match(copy.title, new RegExp(`needs \`${name}\``), name);
    assert.match(copy.lede, new RegExp(`\`${name}\` is not installed`), name);
    assert.match(copy.lede, new RegExp(`does not bundle \`${name}\``), name);
    assert.doesNotMatch(copy.lede, /`npx`/, name);
  }
});

// The page comes from the Shell's table, which Settings names it from too.
test("a missing launcher names the install page the opening carries, and only that", () => {
  const opening = npxOpening("codex");
  assert.equal(
    landingCopy({ ...opening, harness: { ...opening.harness, install: "https://nodejs.org/" } }).lede,
    "`npx` is not installed. Fidget does not bundle `npx`. Install from https://nodejs.org/. Then press Codex again, or pick a different Harness below.",
  );
  assert.equal(
    landingCopy(opening).lede,
    "`npx` is not installed. Fidget does not bundle `npx`. Then press Codex again, or pick a different Harness below.",
  );
});

test("a named Harness that has not come up stays on the landing", () => {
  const copy = landingCopy({
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "hermes",
    harness: { name: "hermes", session: null, alive: false, login: null },
  });
  assert.equal(copy.title, "Hermes is not running");
  assert.match(copy.lede, /Static weights/);
});

test("a launcher that died at startup names why on the landing", () => {
  const failed =
    "`npx -y @agentclientprotocol/codex-acp@latest` exited before initialize, signal: 6 (SIGABRT). " +
    "`npx` runs on Node.js: run `node --version` in a terminal to check that it starts.";
  const opening = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "codex",
    harness: { name: "codex", session: null, alive: false, login: null, failed },
  };
  assert.equal(canAnswer(opening), false);
  const copy = landingCopy(opening);
  assert.equal(copy.title, "Codex failed to start");
  assert.equal(copy.lede, `${failed} Then pick Codex again, or pick a different Harness below.`);
});

test("initializing Harness gates chat and shows clear state", () => {
  const opening = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "hermes",
    harness: {
      name: "hermes",
      session: null,
      alive: false,
      login: null,
      initializing: true,
    },
  };
  assert.equal(canAnswer(opening), false, "chat is gated during initialization");
  assert.equal(composerPlaceholder(opening), "Starting Hermes…");
  const copy = landingCopy(opening);
  assert.equal(copy.title, "Initializing Hermes…");
  assert.match(copy.lede, /starting up/i);
  assert.match(copy.lede, /download/i);
  assert.match(copy.lede, /can take a while/i);
  assert.match(copy.lede, /when the Harness answers/i);
  assert.doesNotMatch(copy.lede, /\d+\s*[–-]\s*\d+\s*seconds/i);
  assert.doesNotMatch(copy.lede, /not running/);
});

test("needs-auth still names the login command", () => {
  const copy = landingCopy({
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "codex",
    login: "codex login",
    harness: {
      name: "codex",
      session: null,
      alive: true,
      login: "codex login",
    },
  });
  assert.equal(copy.title, "Codex needs login");
  assert.equal(copy.command, "codex login");
  assert.equal(copy.signInLabel, null);
  assert.equal(copy.hint, "Run this in a terminal:");
});

test("needs-login offers agent sign-in beside the terminal command", () => {
  const opening = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "codex",
    login: "codex login",
    sign_in: [
      { id: "chatgpt", label: "ChatGPT" },
      { id: "apikey", label: "API Key" },
    ],
    harness: {
      name: "codex",
      session: null,
      alive: true,
      login: "codex login",
    },
  };
  assert.deepEqual(loginPresentation(opening), {
    command: "codex login",
    actions: [
      { id: "chatgpt", label: "ChatGPT" },
      { id: "apikey", label: "API Key" },
    ],
  });
  const copy = landingCopy(opening);
  assert.equal(copy.command, "codex login");
  assert.equal(copy.signInLabel, "Login using:");
  assert.equal(copy.hint, "Or run this in a terminal:");
  assert.equal(
    copy.signInWaiting,
    "Finish signing in in your browser. Any code on that page came from Codex, so continue only if you just clicked this button.",
  );
  assert.deepEqual(copy.signIn, [
    { id: "chatgpt", label: "ChatGPT" },
    { id: "apikey", label: "API Key" },
  ]);
  assert.equal(canAnswer(opening), false);

  const bare = { ...opening };
  delete bare.sign_in;
  assert.deepEqual(loginPresentation(bare).actions, []);
  const bareCopy = landingCopy(bare);
  assert.equal(bareCopy.command, "codex login");
  assert.equal(bareCopy.signInLabel, null);
  assert.equal(bareCopy.hint, "Run this in a terminal:");
  assert.equal(bareCopy.signInWaiting, null);
  assert.deepEqual(bareCopy.signIn, []);

  assert.deepEqual(
    loginPresentation({
      configured: true,
      enabled: true,
      login: "codex login",
      kind: "agent",
    }).actions,
    [],
  );
  assert.deepEqual(landingCopy({ configured: false, enabled: false }).signIn, []);
});

test("Antigravity signs in from its buttons and names no terminal command", () => {
  const opening = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "antigravity",
    login: "Log in with Google from Chat",
    sign_in: [
      { id: "oauth-personal", label: "Log in with Google" },
      { id: "oauth-business", label: "Log in with Gemini Enterprise" },
    ],
    harness: {
      name: "antigravity",
      session: null,
      alive: true,
      login: "Log in with Google from Chat",
    },
  };
  const copy = landingCopy(opening);
  assert.equal(copy.title, "Antigravity needs login");
  assert.equal(copy.signInLabel, "Login using:");
  assert.equal(copy.command, null);
  assert.deepEqual(
    copy.signIn.map((action) => action.id),
    ["oauth-personal", "oauth-business"],
  );
});

test("unconfigured and switched-off copy is unchanged", () => {
  assert.equal(
    landingCopy({ configured: false, enabled: false }).title,
    "Connect a Harness to get started",
  );
  assert.equal(
    landingCopy({
      configured: true,
      enabled: false,
      harness_name: "codex",
      harness: { name: "codex", alive: false, missing: "npx" },
    }).title,
    "Chat is switched off",
  );
});

test("a backticked command becomes a code segment without its backticks", () => {
  assert.deepEqual(inlineSegments("Run `node --version` in a terminal."), [
    { kind: "text", text: "Run " },
    { kind: "code", text: "node --version" },
    { kind: "text", text: " in a terminal." },
  ]);
});

test("a URL becomes a link, leaving the full stop after it as text", () => {
  assert.deepEqual(inlineSegments("Install from https://nodejs.org/. Then retry."), [
    { kind: "text", text: "Install from " },
    { kind: "link", text: "https://nodejs.org/" },
    { kind: "text", text: ". Then retry." },
  ]);
  assert.deepEqual(inlineSegments("see (http://x.ai/docs),"), [
    { kind: "text", text: "see (" },
    { kind: "link", text: "http://x.ai/docs" },
    { kind: "text", text: ")," },
  ]);
});

test("code and a link in one sentence each keep their kind", () => {
  const copy = landingCopy({
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "codex",
    harness: { name: "codex", session: null, alive: false, login: null, missing: "npx", install: "https://nodejs.org/" },
  });
  const kinds = inlineSegments(copy.lede).filter((s) => s.kind !== "text");
  assert.deepEqual(kinds, [
    { kind: "code", text: "npx" },
    { kind: "code", text: "npx" },
    { kind: "link", text: "https://nodejs.org/" },
  ]);
});

test("a URL inside backticks stays code, not a link", () => {
  assert.deepEqual(inlineSegments("`curl https://x.ai/`"), [
    { kind: "code", text: "curl https://x.ai/" },
  ]);
});

test("an unmatched backtick reads as itself", () => {
  assert.deepEqual(inlineSegments("run `a` then `b"), [
    { kind: "text", text: "run " },
    { kind: "code", text: "a" },
    { kind: "text", text: " then `b" },
  ]);
});

test("markup and other schemes stay text", () => {
  assert.deepEqual(inlineSegments('<img src=x onerror="alert(1)"> javascript:alert(1)'), [
    { kind: "text", text: '<img src=x onerror="alert(1)"> javascript:alert(1)' },
  ]);
  assert.deepEqual(inlineSegments(null), []);
});
