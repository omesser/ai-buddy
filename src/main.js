// The webview draws the Character and owns none of its state. Every tick the
// Rust side sends the Engine's frame; this draws it and remembers nothing.

const sprite = document.getElementById("sprite");

window.__TAURI__.event
  .listen("frame", ({ payload: p }) => {
    // Assigning the same src every tick would ask the loader sixty times a
    // second for a file that has not changed.
    if (sprite.dataset.src !== p.src) {
      sprite.dataset.src = p.src;
      sprite.src = p.src;
    }
    sprite.style.left = `${p.x}px`;
    sprite.style.top = `${p.y}px`;
    sprite.style.width = `${p.width}px`;
    sprite.style.height = `${p.height}px`;
    // Nothing draws these yet — the placeholder Character is a single frame —
    // but they are what the Engine is saying, and they are observable from
    // outside once real Animations arrive with the Character Manifest.
    sprite.dataset.animation = p.animation;
    sprite.dataset.frameIndex = p.frame_index;
    sprite.style.visibility = "visible";
  })
  .catch((err) => {
    // No frames means nothing to draw and nothing to hit-test, so say so loudly
    // rather than showing an empty overlay that looks like a hung app.
    console.error("ai-buddy could not listen for the Character's frames:", err);
  });
