// Composer placeholder for the Chat surface. Its own module because chat.js
// reaches window.__TAURI__ as it loads and cannot be imported outside a
// webview; this can, so it has a test. #544.

export function composerPlaceholder(opening) {
  // Same gate as attached(): a login command means the Harness is attached
  // but cannot answer yet (#563). Ask {name} is only for a live composer.
  if (opening.configured && opening.enabled && !opening.login) {
    return `Ask ${opening.name}…`;
  }
  return "Nothing can answer yet";
}
