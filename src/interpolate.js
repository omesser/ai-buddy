// Where the sprite is drawn, between the two placements the renderer holds.
// Arithmetic rather than rendering, so it lives on its own and is unit tested.

/**
 * Interpolate between two placements at time `now`. The renderer draws one
 * sample behind: the sprite moves at a constant rate between samples instead
 * of jumping whenever one lands. Worth it because the Engine ticks at about
 * 44Hz with gaps of 16ms to 38ms, all longer than a 60Hz display frame; measure
 * again with `AI_BUDDY_TRACE_FRAMES=1` before changing this.
 *
 * @param {{x: number, y: number, at: number}} previous
 * @param {{x: number, y: number, at: number}} latest
 * @param {number} now - a `performance.now()` reading
 * @returns {{x: number, y: number}}
 */
export function interpolate(previous, latest, now) {
  const span = latest.at - previous.at;
  // A resumed app or two placements in the same millisecond: nothing to
  // interpolate across, so draw what we were last told.
  if (!(span > 0)) {
    return { x: latest.x, y: latest.y };
  }

  // Clamped, so a late display frame draws the sprite at the latest placement
  // rather than extrapolating past it: overshooting puts the sprite somewhere
  // the Engine never said it was, and on a landing reads as a bounce.
  const t = Math.min(Math.max((now - latest.at) / span, 0), 1);
  return {
    x: previous.x + (latest.x - previous.x) * t,
    y: previous.y + (latest.y - previous.y) * t,
  };
}

/**
 * Whether the sprite has reached `latest`, so a further display frame of its
 * own would draw it where the last one did.
 *
 * The renderer arms its loop on this rather than looping unconditionally
 * (#741). Read it against `interpolate` above: it is false exactly while `t`
 * is still short of 1 and the two placements are somewhere to travel between.
 *
 * @param {{x: number, y: number, at: number} | null} previous
 * @param {{x: number, y: number, at: number}} latest
 * @param {number} now - a `performance.now()` reading
 * @returns {boolean}
 */
export function arrived(previous, latest, now) {
  if (!previous) return true;
  if (previous.x === latest.x && previous.y === latest.y) return true;
  return now >= latest.at + (latest.at - previous.at);
}
