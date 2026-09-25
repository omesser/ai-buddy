import { askSays } from "./chat-ask.js";

export function drawAskDetails(body, ask) {
  const says = askSays(ask);
  const append = (tag, className, text) => {
    const node = body.ownerDocument.createElement(tag);
    node.className = className;
    node.textContent = text;
    body.append(node);
  };

  if (says.title) {
    append("div", "ask-title", says.title);
  }
  for (const detail of says.details) {
    append(detail.code ? "code" : "div", detail.code ? "ask-code" : "ask-prose", detail.text);
  }
  if (says.metadata) {
    append("div", "ask-metadata", says.metadata);
  }
}
