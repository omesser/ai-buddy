// Bubble rendering logic for speech and thinking indicators.

export const CEILING_CLEARANCE = 128;

const MIN_DURATION_MS = 2000;
const MAX_DURATION_MS = 8000;
const BASE_DURATION_MS = 900;
const MS_PER_CHAR = 55;
const MAX_LINES = 6;

export function bubbleDuration(text) {
  const duration = BASE_DURATION_MS + text.length * MS_PER_CHAR;
  return Math.max(MIN_DURATION_MS, Math.min(MAX_DURATION_MS, duration));
}

export function wrapText(text, maxWidth, measureFn) {
  const lines = [];
  const paragraphs = text.split("\n");

  for (const paragraph of paragraphs) {
    if (lines.length >= MAX_LINES) break;

    const words = paragraph.split(" ");
    let currentLine = "";

    for (const word of words) {
      if (lines.length >= MAX_LINES) break;

      const testLine = currentLine ? `${currentLine} ${word}` : word;
      const metrics = measureFn(testLine);

      if (metrics.width > maxWidth && currentLine) {
        lines.push(currentLine);
        currentLine = word;
      } else {
        currentLine = testLine;
      }
    }

    if (currentLine && lines.length < MAX_LINES) {
      lines.push(currentLine);
    }
  }

  if (lines.length === MAX_LINES && (lines.length < paragraphs.length ||
      paragraphs[paragraphs.length - 1].split(" ").length > lines[lines.length - 1].split(" ").length)) {
    lines[MAX_LINES - 1] = lines[MAX_LINES - 1].trimEnd() + "…";
  }

  return lines.slice(0, MAX_LINES);
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

export function placeBubble(spriteRect, bubbleSize, displayBounds, ceilingClearance) {
  const spriteHeadY = spriteRect.y;
  const spriteBottomY = spriteRect.y + spriteRect.height;
  const spriteCenterX = spriteRect.x + spriteRect.width / 2;

  let x = spriteCenterX - bubbleSize.width / 2;
  let y = spriteHeadY - bubbleSize.height - 10;
  let flipped = false;

  if (spriteHeadY < ceilingClearance) {
    y = spriteBottomY + 10;
    flipped = true;
  }

  const unclamped = x;
  x = Math.max(displayBounds.x, Math.min(x, displayBounds.x + displayBounds.width - bubbleSize.width));
  y = Math.max(displayBounds.y, Math.min(y, displayBounds.y + displayBounds.height - bubbleSize.height));

  const tailOffset = spriteCenterX - (x + bubbleSize.width / 2);

  return { x, y, flipped, tailOffset };
}
