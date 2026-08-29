// Where the sprite is drawn, between the two placements the renderer holds.
//
// This is arithmetic rather than rendering, so it lives on its own and is unit
// tested — docs/SPEC.md: "Arithmetic is never exempt, wherever it lives."

/**
 * Interpolate between two placements at time `now`.
 *
 * The renderer draws one sample behind: at the instant `latest` arrives this
 * returns `previous`, and it reaches `latest` one sample-interval later. The
 * cost is that lag; the gain is that the sprite moves at a constant rate
 * between samples instead of jumping whenever one lands.
 *
 * That trade is worth making because the Engine does not tick at the display's
 * rate. It sleeps 16ms and then does a tick's work, which measures at about
 * 44Hz with gaps from 16ms to 38ms — every one of them longer than a 60Hz
 * display frame. Drawn directly, most display frames would repeat the previous
 * position and the next would jump two frames' worth.
 *
 * Those numbers come from running the app with `AI_BUDDY_TRACE_FRAMES=1` and
 * taking the deltas between the timestamps it emits. Re-measure them there
 * before changing what this does.
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

  // Clamped, so a display frame that arrives late draws the sprite at the
  // latest placement rather than extrapolating past it. Overshooting is worse
  // than lagging: it puts the sprite somewhere the Engine never said it was,
  // and on a landing that reads as a bounce that did not happen.
  const t = Math.min(Math.max((now - latest.at) / span, 0), 1);
  return {
    x: previous.x + (latest.x - previous.x) * t,
    y: previous.y + (latest.y - previous.y) * t,
  };
}
