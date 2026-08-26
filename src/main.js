// The webview draws the Character and owns none of its state. It asks the Rust
// side where the sprite goes, then gets out of the way.

const sprite = document.getElementById("sprite");

window.__TAURI__.core
  .invoke("placement")
  .then((p) => {
    sprite.src = p.src;
    sprite.style.left = `${p.x}px`;
    sprite.style.top = `${p.y}px`;
    sprite.style.width = `${p.width}px`;
    sprite.style.height = `${p.height}px`;
    sprite.style.visibility = "visible";
  })
  .catch((err) => {
    // No sprite means nothing to hit-test, so say so loudly rather than showing
    // an empty overlay that looks like a hung app.
    console.error("ai-buddy could not place the Character:", err);
  });
