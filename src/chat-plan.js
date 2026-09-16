// The agent's plan, as the rows the list above the composer draws. Its own
// module for the reason chat-ask.js gives: chat.js reaches window.__TAURI__ as
// it loads and cannot be imported outside a webview; this can, so it has a test.

// `content` is untrusted Harness text and goes through the same sanitizer an
// ask does. `status` and `priority` pass through as the wire's own spellings,
// so CSS owns every appearance and nothing here decides how a status looks.

import { clamp, flat } from "./chat-ask.js";

// How much of one step the list may draw, in characters: about two wrapped
// lines in a 420-point window, enough for a step and short enough that a plan
// of a dozen stays compact beside the transcript.
const STEP_LIMIT = 120;

export function planSteps(entries) {
  if (!Array.isArray(entries)) {
    return [];
  }
  return entries.map((entry) => ({
    text: clamp(flat(entry.content ?? ""), STEP_LIMIT),
    status: entry.status ?? "",
    priority: entry.priority ?? "",
  }));
}
