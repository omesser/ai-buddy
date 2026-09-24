#!/usr/bin/env node
// A stand-in ACP agent on stdio. Answers `initialize` and `session/new`,
// refuses the first N `session/prompt`s with the JSON-RPC error the env
// names, and answers every later one. An auth refusal mid-conversation
// (#991) is a token dying on the Harness's side, which no test can force, so
// this is how it is driven on demand:
//   AI_BUDDY_HARNESS="node scripts/stub-acp-agent.cjs" scripts/probe-harness.sh
// Env:
//   AI_BUDDY_STUB_FAIL_PROMPTS   prompts refused before the stub answers (default 1)
//   AI_BUDDY_STUB_ERROR_CODE     JSON-RPC code of the refusal (default -32603)
//   AI_BUDDY_STUB_ERROR_MESSAGE  its message (default the wall #991 reported)
//   AI_BUDDY_STUB_ERROR_KIND     `data.errorKind`, claude-code-acp's field for
//                                the kind of failure; empty sends no `data`

const readline = require("node:readline");

const FAIL_PROMPTS = Number(process.env.AI_BUDDY_STUB_FAIL_PROMPTS ?? 1);
const ERROR_CODE = Number(process.env.AI_BUDDY_STUB_ERROR_CODE ?? -32603);
const ERROR_MESSAGE =
  process.env.AI_BUDDY_STUB_ERROR_MESSAGE ??
  "Internal error: Failed to authenticate: OAuth session expired and could not be refreshed";
const ERROR_KIND = process.env.AI_BUDDY_STUB_ERROR_KIND ?? "authentication_failed";

const SESSION = "stub-session";
let prompts = 0;

function say(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function refusal() {
  const error = { code: ERROR_CODE, message: ERROR_MESSAGE };
  if (ERROR_KIND) {
    error.data = { errorKind: ERROR_KIND };
  }
  return error;
}

function answer(id) {
  say({
    jsonrpc: "2.0",
    method: "session/update",
    params: {
      sessionId: SESSION,
      update: {
        sessionUpdate: "agent_message_chunk",
        content: { type: "text", text: "idle\nHello from the stub." },
      },
    },
  });
  say({ jsonrpc: "2.0", id, result: { stopReason: "end_turn" } });
}

readline.createInterface({ input: process.stdin }).on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  const { id, method } = message;
  switch (method) {
    case "initialize":
      say({
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: 1,
          agentInfo: { name: "stub-acp-agent", version: "0" },
          agentCapabilities: { loadSession: false },
          authMethods: [{ id: "stub", name: "Stub login", description: "stub --login" }],
        },
      });
      return;
    case "session/new":
      say({ jsonrpc: "2.0", id, result: { sessionId: SESSION } });
      return;
    case "session/prompt":
      prompts += 1;
      if (prompts <= FAIL_PROMPTS) {
        say({ jsonrpc: "2.0", id, error: refusal() });
      } else {
        answer(id);
      }
      return;
    default:
      if (id !== undefined) {
        say({ jsonrpc: "2.0", id, error: { code: -32601, message: `Method not found: ${method}` } });
      }
  }
});
