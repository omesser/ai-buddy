// The bubble's arithmetic and decisions. main.js owns the DOM that draws them.

const MIN_DURATION_MS = 2000;
const MAX_DURATION_MS = 8000;
const BASE_DURATION_MS = 900;
const MS_PER_CHAR = 55;
const MAX_LINES = 6;

export function bubbleDuration(text) {
  const duration = BASE_DURATION_MS + text.length * MS_PER_CHAR;
  return Math.max(MIN_DURATION_MS, Math.min(MAX_DURATION_MS, duration));
}

// The lines the bubble draws, and whether the turn ran past them.
//
// The flag is the whole point of returning an object: a turn that did not fit
// is one the user can only finish on the Chat surface, and #547 puts a control
// in the bubble to get there. Reading it back off the trailing "…" would call
// a line that ends in one truncated.
export function wrapText(text, maxWidth, measureFn) {
  const lines = [];
  let truncated = false;

  for (const paragraph of text.split("\n")) {
    if (lines.length >= MAX_LINES) {
      truncated = true;
      break;
    }

    let currentLine = "";
    for (const word of paragraph.split(" ")) {
      if (lines.length >= MAX_LINES) {
        truncated = true;
        break;
      }

      const testLine = currentLine ? `${currentLine} ${word}` : word;
      if (measureFn(testLine).width > maxWidth && currentLine) {
        lines.push(currentLine);
        currentLine = word;
      } else {
        currentLine = testLine;
      }
    }

    if (currentLine) {
      if (lines.length < MAX_LINES) {
        lines.push(currentLine);
      } else {
        truncated = true;
      }
    }
  }

  if (truncated) {
    lines[MAX_LINES - 1] = lines[MAX_LINES - 1].trimEnd() + "…";
  }

  return { lines, truncated };
}

export const THINKING_GRACE_MS = 250;
export const THINKING_MIN_HOLD_MS = 600;

// The bubble decisions, apart from the DOM that displays them, so node can
// drive the machine through tick orderings a display never reproduces on
// demand. Three rules the transport and the reader force:
//
// - `dialogue` rides exactly one Engine tick, and the renderer keeps only the
//   newest placement while it waits for its next animation frame. The Engine
//   ticks slightly faster than a display refreshes, so some ticks are only
//   ever seen by `event`; a pulse read from the newest placement instead
//   would vanish there, silently dropping the line and leaving the thinking
//   indicator to die by timer. `event` runs per delivery and latches it.
// - A response ends the thinking indicator the same frame it shows, min-hold
//   notwithstanding: the hold exists to keep a briefly-shown indicator from
//   flickering when a turn ends *silently*, never to sit beside an answer.
// - Speech and the indicator are mutually exclusive, and speech wins: while
//   a line is displayed — its whole reading window — the indicator never
//   shows, whatever a new turn does. When the line hides, a turn still in
//   flight starts its grace from that moment, so a reply landing right then
//   never flashes the indicator.
export function createBubbleMachine(io) {
  const schedule = io.schedule ?? ((fn, ms) => setTimeout(fn, ms));
  const cancel = io.cancel ?? ((id) => clearTimeout(id));

  let pendingDialogue = null;
  let speechTimer = null;
  let speechShowing = false;
  let graceTimer = null;
  let minHoldTimer = null;
  let thinkingShown = false;
  let thinking = false;

  function hideThinkingNow() {
    if (graceTimer !== null) {
      cancel(graceTimer);
      graceTimer = null;
    }
    if (minHoldTimer !== null) {
      cancel(minHoldTimer);
      minHoldTimer = null;
    }
    if (thinkingShown) {
      thinkingShown = false;
      io.hideThinking();
    }
  }

  function armGrace() {
    graceTimer = schedule(() => {
      graceTimer = null;
      if (!thinking || speechShowing) return;
      thinkingShown = true;
      io.showThinking();
      minHoldTimer = schedule(() => {
        minHoldTimer = null;
        if (!thinking) hideThinkingNow();
      }, THINKING_MIN_HOLD_MS);
    }, THINKING_GRACE_MS);
  }

  return {
    // Every delivered placement, straight from the event listener.
    event(placement) {
      if (placement.dialogue) pendingDialogue = placement.dialogue;
    },

    // The newest placement, once per drawn frame.
    frame(placement) {
      const dialogue = pendingDialogue;
      pendingDialogue = null;

      // A hidden sprite speaks to nobody; the pulse is consumed, not queued,
      // or the line would pop up whenever the sprite next fades in.
      if (dialogue && placement.visible) {
        hideThinkingNow();
        if (speechTimer !== null) cancel(speechTimer);
        speechShowing = true;
        io.showSpeech(dialogue);
        speechTimer = schedule(() => {
          speechTimer = null;
          speechShowing = false;
          io.hideSpeech();
          // Only now may a turn still in flight surface its indicator.
          if (thinking && graceTimer === null && !thinkingShown) armGrace();
        }, bubbleDuration(dialogue));
      }

      thinking = Boolean(placement.thinking && placement.visible);
      if (thinking) {
        if (!thinkingShown && graceTimer === null && !speechShowing) {
          armGrace();
        }
      } else if (graceTimer !== null) {
        cancel(graceTimer);
        graceTimer = null;
      } else if (thinkingShown && minHoldTimer === null) {
        hideThinkingNow();
      }
    },

    // The hide hotkey's instant answer: nothing may stay or come back.
    hideAllNow() {
      hideThinkingNow();
      if (speechTimer !== null) {
        cancel(speechTimer);
        speechTimer = null;
      }
      speechShowing = false;
      pendingDialogue = null;
      io.hideSpeech();
    },
  };
}

// The bubble sits above the head (ADR-0013, amended by #441). At the ceiling,
// when the clamp would cover the Character's face (#546), invert: put the bubble
// under the Character at the same mirrored vertical distance.
export function placeBubble(spriteRect, bubbleSize, displayBounds) {
  const spriteCenterX = spriteRect.x + spriteRect.width / 2;
  const gap = 10;

  let x = spriteCenterX - bubbleSize.width / 2;
  let y = spriteRect.y - bubbleSize.height - gap;

  const wouldClampToTop = y < displayBounds.y;
  const clampedY = displayBounds.y;
  const wouldCoverSprite = wouldClampToTop && (clampedY + bubbleSize.height > spriteRect.y);

  if (wouldCoverSprite) {
    y = spriteRect.y + spriteRect.height + gap;
  }

  x = Math.max(displayBounds.x, Math.min(x, displayBounds.x + displayBounds.width - bubbleSize.width));
  y = Math.max(displayBounds.y, Math.min(y, displayBounds.y + displayBounds.height - bubbleSize.height));

  const tailOffset = spriteCenterX - (x + bubbleSize.width / 2);

  return { x, y, tailOffset };
}

// A placement as this overlay may act on it: the shell names one bubble owner
// per Instance (#178, `bubble_owner`), and stripping the bubble fields before
// the placement is latched or drawn keeps a losing overlay from arming the
// thinking grace off a `thinking` it was never meant to show.
//
// The cue goes with them, for a louder reason (#277): every overlay is told
// where every sprite is, so a cue every overlay played would be drawn once per
// display the sprite touches and — worse — heard once per display.
export function forOverlay(placement) {
  if (placement.bubble) return placement;
  return { ...placement, dialogue: null, thinking: false, cue: null };
}
