// The Chat UI design class on the root element. Its own module because
// chat.js cannot be imported outside a webview; this can, so it has a test.

const DESIGNS = ["minimal", "terminal", "glass"];

// Allowlist known Chat UI designs; map unknowns to Minimal so arbitrary
// strings from settings or events cannot become class names.
export function normalizeChatUi(value) {
  return DESIGNS.includes(value) ? value : "minimal";
}

export function applyChatUiClass(root, design) {
  root.classList.remove(...DESIGNS.map((name) => `chat-ui-${name}`));
  root.classList.add(`chat-ui-${normalizeChatUi(design)}`);
}
