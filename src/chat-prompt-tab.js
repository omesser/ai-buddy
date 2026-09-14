// What the Prompt tab claims is in force, from one opening. Its own module
// because chat.js reaches window.__TAURI__ as it loads and cannot be imported
// outside a webview; this can, so it has a test. #680.

// Authored layers are in force only when the opening said Blank AI is off.
// Missing the bit is not "off": that is how the tab used to draw a Character
// Prompt the wire had already stripped.
export function promptTabFromOpening(opening) {
  return { authored: opening?.blank === false };
}
