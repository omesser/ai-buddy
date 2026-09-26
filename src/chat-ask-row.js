// Draws what an ask says into the consent row. Apart from chat.js for the
// reason chat-ask.js gives: chat.js cannot load outside a webview, and this
// can, so the DOM shape has a test.

import { askSays } from "./chat-ask.js";

// One element per part. Code is a real <code> so it shares the paint reply
// code gets, and every part reaches the DOM through textContent only.
const ELEMENT = {
  title: ["div", "ask-title"],
  code: ["code", "ask-code"],
  prose: ["div", "ask-prose"],
  metadata: ["div", "ask-metadata"],
};

export function drawAskDetails(body, ask) {
  for (const { kind, text } of askSays(ask)) {
    const [tag, className] = ELEMENT[kind];
    const node = body.ownerDocument.createElement(tag);
    node.className = className;
    node.textContent = text;
    body.append(node);
  }
}
