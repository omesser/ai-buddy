// Hover-to-compose above a Character. The overlay owns the DOM; this decides
// when the composer is up and whether a press still belongs to the pet.

import { placeBubble } from "./bubble.js";

export const HOVER_DELAY_MS = 2500;

// A click that stays put is a poke. Past this, the same press is a drag.
export const DRAG_DISMISS_PX = 4;

export function crossedDrag(dx, dy) {
  return dx * dx + dy * dy >= DRAG_DISMISS_PX * DRAG_DISMISS_PX;
}

export function createQuickMessage({ schedule, clear, send, onChange }) {
  let visible = false;
  let text = "";
  let focused = false;
  let claimFocus = false;
  let disposed = false;
  let hoverTimer = null;
  let overSprite = false;

  function changed() {
    if (!disposed) onChange?.();
  }

  function cancelHover() {
    if (hoverTimer === null) return;
    clear(hoverTimer);
    hoverTimer = null;
  }

  function hide() {
    cancelHover();
    const was = visible;
    visible = false;
    focused = false;
    claimFocus = false;
    if (was) changed();
  }

  function show() {
    if (visible || disposed) return;
    visible = true;
    // The caret is the typing hold. Claiming it here, not after a later
    // focus event, stops the walk on the tick the pill appears.
    focused = true;
    claimFocus = true;
    changed();
  }

  function dismissOpen() {
    // A click away also abandons a dwell that has not opened yet, so the
    // pill cannot appear after the pointer has already gone.
    cancelHover();
    if (!visible) return;
    text = "";
    hide();
  }

  function submit() {
    const line = text.trim();
    if (!visible || !line) return false;
    text = "";
    hide();
    send(line);
    return true;
  }

  return {
    get visible() {
      return visible;
    },
    get text() {
      return text;
    },
    get typing() {
      return visible && focused;
    },
    takeFocus() {
      if (!claimFocus) return false;
      claimFocus = false;
      return true;
    },
    enterSprite() {
      overSprite = true;
      if (visible || hoverTimer !== null) return;
      hoverTimer = schedule(() => {
        hoverTimer = null;
        if (disposed || !overSprite) return;
        show();
      }, HOVER_DELAY_MS);
    },
    leaveSprite() {
      overSprite = false;
      // Leaving never dismisses. It only abandons a dwell that has not fired.
      cancelHover();
    },
    setText(value) {
      text = value;
    },
    focus() {
      if (focused) return;
      focused = true;
      if (visible) changed();
    },
    blur() {
      if (!focused) return;
      focused = false;
      if (visible) changed();
    },
    keydown(key, mods = {}) {
      if (!visible) return false;
      if (key === "Enter" && !mods.shiftKey && !mods.composing) return submit();
      return false;
    },
    // A press on the composer is text, not a Poke. The character still is.
    press(where, report) {
      if (where === "composer") return;
      report();
    },
    outside: dismissOpen,
    drag: dismissOpen,
    summon: dismissOpen,
    submit,
    restore(value) {
      text = value;
      visible = true;
      focused = true;
      claimFocus = true;
      changed();
    },
    dismiss() {
      text = "";
      hide();
    },
    dispose() {
      disposed = true;
      cancelHover();
    },
  };
}

// Same seat as Speech. When that bubble is already there, step clear of it
// so a hover does not cover the line the Character is saying.
export function placeQuickMessage(spriteRect, size, bounds, speechRect) {
  const pos = placeBubble(spriteRect, size, bounds);
  if (!speechRect) return pos;
  const overlaps =
    pos.x < speechRect.x + speechRect.width &&
    pos.x + size.width > speechRect.x &&
    pos.y < speechRect.y + speechRect.height &&
    pos.y + size.height > speechRect.y;
  if (!overlaps) return pos;
  const gap = 10;
  let y = speechRect.y - size.height - gap;
  if (y < bounds.y) y = speechRect.y + speechRect.height + gap;
  y = Math.max(bounds.y, Math.min(y, bounds.y + bounds.height - size.height));
  return { ...pos, y };
}
