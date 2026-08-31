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
