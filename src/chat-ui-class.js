// The Chat UI design class on the root element. Its own module because
// chat.js cannot be imported outside a webview; this can, so it has a test.

// Allowlist known Chat UI designs; map unknowns to Minimal so arbitrary
// strings from settings or events cannot become class names.
export function normalizeChatUi(value) {
  const allowed = ["minimal", "terminal", "glass"];
  return allowed.includes(value) ? value : "minimal";
}

// Apply a Chat UI design to the root element, removing all others first.
export function applyChatUiClass(root, design) {
  const chatUi = normalizeChatUi(design);
  root.classList.remove("chat-ui-minimal", "chat-ui-terminal", "chat-ui-glass");
  root.classList.add(`chat-ui-${chatUi}`);
}
