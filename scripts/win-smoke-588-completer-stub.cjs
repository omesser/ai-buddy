#!/usr/bin/env node
// Completer stub for the #588 Windows watch smoke (scripts/win-smoke-588-openchat.ps1).
//
// The overlay speaks whatever the Director's Completer returns, and #588 only
// draws its "Open chat" control when a turn runs past the bubble's six lines.
// So this stub answers every wake with one long turn: a Behavior name the
// Character declares, then a paragraph long enough to truncate. The model path
// reads the reply's first line as the Behavior and joins the rest as the
// spoken line (crates/core/src/director.rs::parse_proposal), so the shape here
// is `<behavior>\n<long line>`.
//
// It speaks both wire shapes the Endpoint can ask for (src-tauri/src/model.rs):
// Server-Sent Events when the request body sets `stream: true`, and a whole
// chat-completions JSON otherwise. The Endpoint tries the stream first and
// falls back to the whole body, so answering both keeps the first wake from
// costing a wasted round trip.
//
// Env:
//   AI_BUDDY_SMOKE_PORT      port to listen on (default 18765)
//   AI_BUDDY_SMOKE_BEHAVIOR  Behavior name on line one (default "stroll")
//   AI_BUDDY_SMOKE_LINE      the long spoken line (default below)

const http = require("node:http");

const PORT = Number(process.env.AI_BUDDY_SMOKE_PORT || 18765);
const HOST = "127.0.0.1";
const BEHAVIOR = process.env.AI_BUDDY_SMOKE_BEHAVIOR || "stroll";

// One paragraph, no newlines: parse_proposal would otherwise fold newlines into
// spaces anyway, and a single long run is the plainest thing that outruns six
// wrapped lines at the bubble's 260px cap. Kept well past that so a different
// font or DPI on the watch machine still truncates.
const LINE =
  process.env.AI_BUDDY_SMOKE_LINE ||
  "Right, let me talk you through the whole plan because there is quite a lot " +
    "to get through here and I do not want to leave anything out along the way: " +
    "first I will stroll across the desk to stretch my legs, then I will check " +
    "every window you have open, summarise what each one is doing, flag the two " +
    "that look risky, propose a tidier layout, and finally suggest the next " +
    "three things worth your attention before you lose the thread of it entirely.";

const CONTENT = `${BEHAVIOR}\n${LINE}`;

function log(...args) {
  process.stderr.write(`[stub] ${args.join(" ")}\n`);
}

// The chat-completions JSON the whole-body path reads
// (model.rs::content_from_body reads choices[0].message.content).
function wholeBody() {
  return JSON.stringify({
    id: "smoke-stub",
    object: "chat.completion",
    choices: [
      {
        index: 0,
        message: { role: "assistant", content: CONTENT },
        finish_reason: "stop",
      },
    ],
  });
}

// The SSE frames the stream path reads (model.rs::read_stream / read_event:
// text under choices[0].delta.content, a finish_reason to mark the end, then
// the [DONE] sentinel).
function streamFrames() {
  const frame = (obj) => `data: ${JSON.stringify(obj)}\n\n`;
  return [
    frame({ choices: [{ delta: { role: "assistant" }, finish_reason: null }] }),
    frame({ choices: [{ delta: { content: `${BEHAVIOR}\n` }, finish_reason: null }] }),
    frame({ choices: [{ delta: { content: LINE }, finish_reason: null }] }),
    frame({ choices: [{ delta: {}, finish_reason: "stop" }] }),
    "data: [DONE]\n\n",
  ].join("");
}

const server = http.createServer((req, res) => {
  // The pre-flight probe (model.rs) hangs /v1/models off the origin; answer it
  // so a run that probes does not read the stub as a dead host.
  if (req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ object: "list", data: [{ id: "smoke-stub" }] }));
    return;
  }

  let raw = "";
  req.on("data", (chunk) => {
    raw += chunk;
  });
  req.on("end", () => {
    let wantsStream = false;
    try {
      wantsStream = JSON.parse(raw || "{}").stream === true;
    } catch {
      wantsStream = false;
    }
    log(`${req.method} ${req.url} stream=${wantsStream}`);
    if (wantsStream) {
      res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      });
      res.end(streamFrames());
    } else {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(wholeBody());
    }
  });
});

server.listen(PORT, HOST, () => {
  log(`listening on http://${HOST}:${PORT} behavior=${BEHAVIOR} line=${LINE.length} chars`);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    server.close(() => process.exit(0));
  });
}
