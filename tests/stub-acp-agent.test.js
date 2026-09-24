// The stub is the only way to drive an auth refusal mid-conversation on
// demand (#991), so its contract is pinned: the first prompt is refused with
// the error the env names, and the next one is answered.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const STUB = fileURLToPath(new URL("../scripts/stub-acp-agent.cjs", import.meta.url));

async function speak(env, requests) {
  const child = spawn(process.execPath, [STUB], { env: { ...process.env, ...env } });
  const lines = createInterface({ input: child.stdout });
  const replies = [];
  const next = () => new Promise((resolve) => lines.once("line", (line) => resolve(JSON.parse(line))));
  for (const request of requests) {
    const got = next();
    child.stdin.write(`${JSON.stringify(request)}\n`);
    let reply = await got;
    while (reply.id === undefined) {
      replies.push(reply);
      reply = await next();
    }
    replies.push(reply);
  }
  child.stdin.end();
  await once(child, "exit");
  return replies;
}

const prompt = (id) => ({
  jsonrpc: "2.0",
  id,
  method: "session/prompt",
  params: { sessionId: "stub-session", prompt: [{ type: "text", text: "hi" }] },
});

test("the first prompt is refused the way claude-code-acp refuses it, the second is answered", async () => {
  const replies = await speak({}, [
    { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: 1 } },
    { jsonrpc: "2.0", id: 2, method: "session/new", params: { cwd: "/", mcpServers: [] } },
    prompt(3),
    prompt(4),
  ]);

  assert.deepEqual(replies[0].result.authMethods, [
    { id: "stub", name: "Stub login", description: "stub --login" },
  ]);
  assert.deepEqual(replies[1], { jsonrpc: "2.0", id: 2, result: { sessionId: "stub-session" } });
  assert.deepEqual(replies[2], {
    jsonrpc: "2.0",
    id: 3,
    error: {
      code: -32603,
      message:
        "Internal error: Failed to authenticate: OAuth session expired and could not be refreshed",
      data: { errorKind: "authentication_failed" },
    },
  });
  assert.equal(replies[3].method, "session/update");
  assert.equal(replies[3].params.update.content.text, "idle\nHello from the stub.");
  assert.deepEqual(replies[4], { jsonrpc: "2.0", id: 4, result: { stopReason: "end_turn" } });
});

test("the env picks the code and drops the data", async () => {
  const replies = await speak(
    { AI_BUDDY_STUB_ERROR_CODE: "-32000", AI_BUDDY_STUB_ERROR_KIND: "" },
    [prompt(1)],
  );

  assert.deepEqual(replies, [
    {
      jsonrpc: "2.0",
      id: 1,
      error: {
        code: -32000,
        message:
          "Internal error: Failed to authenticate: OAuth session expired and could not be refreshed",
      },
    },
  ]);
});
