import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  canAnswer,
  composerPlaceholder,
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
  assert.match(js, /import \{ canAnswer, landingCopy \} from "\.\/chat-connect\.js"/);
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

test("a missing npx names nodejs.org install URL", () => {
  for (const name of ["claude", "codex", "pi"]) {
    const opening = npxOpening(name);
    const copy = landingCopy(opening);
    assert.match(copy.lede, /nodejs\.org/i, `${name} landing mentions nodejs.org`);
  }
});

test("a missing first-party CLI names its install URL", () => {
  const urls = {
    hermes: "hermes-agent.nousresearch.com",
    goose: "goose-docs.ai/docs/getting-started/installation",
    copilot: "docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli",
    "cursor-agent": "cursor.com",
    grok: "x.ai",
    opencode: "opencode.ai",
  };
  for (const [name, urlPart] of Object.entries(urls)) {
    const opening = {
      name: "bmo",
      configured: true,
      enabled: true,
      harness_name: name,
      harness: { name, session: null, alive: false, login: null, missing: name },
    };
    const copy = landingCopy(opening);
    const regex = new RegExp(urlPart.replace(/\./g, "\\."), "i");
    assert.match(copy.lede, regex, `${name} landing mentions ${urlPart}`);
  }
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

test("a sign-in in flight shows the Harness's device code under its button", () => {
  const opening = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "grok",
    login: "grok login",
    sign_in: [
      { id: "cached_token", label: "cached_token" },
      { id: "grok.com", label: "Grok" },
    ],
    sign_in_prompt: {
      method: "grok.com",
      code: "QB3D-P96A",
      url: "https://auth.x.ai/device?user_code=QB3D-P96A",
    },
    harness: { name: "grok", session: null, alive: true, login: "grok login" },
  };
  assert.deepEqual(landingCopy(opening).signInCode, {
    method: "grok.com",
    label: "Your code:",
    code: "QB3D-P96A",
    url: "If no browser opened, go to https://auth.x.ai/device?user_code=QB3D-P96A",
  });

  const noUrl = { ...opening, sign_in_prompt: { method: "grok.com", code: "QB3D-P96A" } };
  assert.equal(landingCopy(noUrl).signInCode.url, null);

  const cleared = { ...opening };
  delete cleared.sign_in_prompt;
  assert.equal(landingCopy(cleared).signInCode, null);

  const stranger = { ...opening, sign_in_prompt: { method: "gone", code: "QB3D-P96A" } };
  assert.equal(landingCopy(stranger).signInCode, null);

  assert.match(js, /paintSignIn\(copy\.signIn, copy\.signInCode\)/);
  assert.match(js, /className = "sign-in-code"/);
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
