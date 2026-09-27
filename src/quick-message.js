// Hover-to-compose above a Character. The overlay owns the DOM; this decides
// when the composer is up and whether a press still belongs to the pet.

import { placeBubble } from "./bubble.js";

export const HOVER_DELAY_MS = 200;

// placeBubble leaves 10px of air that is not an element, and the overlay
// passes clicks through there. Hiding on the sprite's leave drops the
// composer before the pointer can cross onto it.
export const LEAVE_GRACE_MS = 120;

export function createQuickMessage({ schedule, clear, send, onChange }) {
  let visible = false;
  let text = "";
  let focused = false;
  let disposed = false;
  let hoverTimer = null;
  let leaveTimer = null;
  let overSprite = false;
  let overComposer = false;

  function changed() {
    if (!disposed) onChange?.();
  }

  function engaged() {
    return focused || text.trim().length > 0;
  }

  function zone() {
    return overSprite || overComposer;
  }

  function cancelHover() {
    if (hoverTimer === null) return;
    clear(hoverTimer);
    hoverTimer = null;
  }

  function cancelLeave() {
    if (leaveTimer === null) return;
    clear(leaveTimer);
    leaveTimer = null;
  }

  function hide() {
    cancelHover();
    cancelLeave();
    const was = visible;
    visible = false;
    focused = false;
    if (was) changed();
  }

  function submit() {
    const line = text.trim();
    if (!visible || !line) return false;
    text = "";
    hide();
    send(line);
    return true;
  }

  function considerLeave() {
    if (zone() || engaged() || !visible) {
      cancelLeave();
      return;
    }
    // A later leave must not restart the wait. The overlay reports leave
    // again on the next frame, and resetting it would keep the composer up.
    if (leaveTimer !== null) return;
    leaveTimer = schedule(() => {
      leaveTimer = null;
      if (!zone() && !engaged()) hide();
    }, LEAVE_GRACE_MS);
  }

  return {
    get visible() {
      return visible;
    },
    get text() {
      return text;
    },
    enterSprite() {
      overSprite = true;
      cancelLeave();
      if (visible || hoverTimer !== null) return;
      hoverTimer = schedule(() => {
        hoverTimer = null;
        if (disposed || (!overSprite && !overComposer)) return;
        visible = true;
        changed();
      }, HOVER_DELAY_MS);
    },
    leaveSprite() {
      overSprite = false;
      if (!zone()) cancelHover();
      considerLeave();
    },
    enterComposer() {
      overComposer = true;
      cancelLeave();
    },
    leaveComposer() {
      overComposer = false;
      considerLeave();
    },
    setText(value) {
      text = value;
      if (!engaged()) considerLeave();
    },
    focus() {
      focused = true;
      cancelLeave();
    },
    blur() {
      focused = false;
      considerLeave();
    },
    keydown(key, mods = {}) {
      if (!visible) return false;
      if (key === "Escape") {
        if (text.trim()) return false;
        hide();
        return true;
      }
      if (key === "Enter" && !mods.shiftKey && !mods.composing) return submit();
      return false;
    },
    // A press on the composer is text, not a Poke. The character still is.
    press(where, report) {
      if (where === "composer") return;
      report();
    },
    submit,
    restore(value) {
      text = value;
      visible = true;
      changed();
    },
    dismiss() {
      text = "";
      hide();
    },
    dispose() {
      disposed = true;
      cancelHover();
      cancelLeave();
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
