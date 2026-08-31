// Bubble rendering logic for speech and thinking indicators.
//
// Pure functions: main.js calls them, the webview owns timing and display.

const MIN_DURATION_MS = 2000;
const MAX_DURATION_MS = 8000;
const BASE_DURATION_MS = 900;
const MS_PER_CHAR = 55;
const MAX_LINES = 6;

/**
 * How long to show a speech bubble, in milliseconds.
 * 900ms base + 55ms per character, clamped to 2-8s.
 */
export function bubbleDuration(text) {
  const duration = BASE_DURATION_MS + text.length * MS_PER_CHAR;
  return Math.max(MIN_DURATION_MS, Math.min(MAX_DURATION_MS, duration));
}

/**
 * Wrap text at maxWidth logical pixels, truncating past MAX_LINES with ellipsis.
 * Returns an array of lines.
 * 
 * Takes an optional measureFn for testing; in production main.js passes
 * ctx.measureText.bind(ctx).
 */
export function wrapText(text, maxWidth, measureFn) {
  // Default measure function for browser environment
  const measure = measureFn || ((text) => {
    if (typeof document === 'undefined') {
      // Fallback for test environment: rough approximation
      return { width: text.length * 8 };
    }
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d");
    ctx.font = "14px system-ui, sans-serif";
    return ctx.measureText(text);
  });

  const lines = [];
  const paragraphs = text.split("\n");

  for (const paragraph of paragraphs) {
    if (lines.length >= MAX_LINES) break;

    const words = paragraph.split(" ");
    let currentLine = "";

    for (const word of words) {
      if (lines.length >= MAX_LINES) break;

      const testLine = currentLine ? `${currentLine} ${word}` : word;
      const metrics = measure(testLine);

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

/**
 * Calculate bubble position relative to sprite.
 * Returns {x, y} in the same coordinate space as spriteRect.
 * 
 * @param spriteRect {x, y, width, height} - sprite position
 * @param bubbleSize {width, height} - measured bubble dimensions
 * @param displayBounds {x, y, width, height} - usable display area
 * @param ceilingClearance - minimum distance from top (CEILING_CLEARANCE = 128)
 */
export function placeBubble(spriteRect, bubbleSize, displayBounds, ceilingClearance) {
  const spriteHeadY = spriteRect.y;
  const spriteBottomY = spriteRect.y + spriteRect.height;
  const spriteCenterX = spriteRect.x + spriteRect.width / 2;

  let x = spriteCenterX - bubbleSize.width / 2;
  let y = spriteHeadY - bubbleSize.height - 10;
  let flipped = false;

  // Flip below sprite when near ceiling
  if (spriteHeadY < ceilingClearance) {
    y = spriteBottomY + 10;
    flipped = true;
  }

  // Clamp horizontally to display bounds
  x = Math.max(displayBounds.x, Math.min(x, displayBounds.x + displayBounds.width - bubbleSize.width));

  // Clamp vertically to display bounds
  y = Math.max(displayBounds.y, Math.min(y, displayBounds.y + displayBounds.height - bubbleSize.height));

  return { x, y, flipped };
}
