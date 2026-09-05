// What the Chat surface's status bar draws, worked out from one push and the
// clock (ADR-0010).
//
// Its own module because it is the only part of that bar with an answer that
// can be wrong: the Shell pushes a deadline and the window subtracts, so the
// countdown is arithmetic here rather than traffic there. chat.js reaches
// window.__TAURI__ as it loads and cannot be imported outside a webview; this
// can, which is what gives it a test.

// What a cell says when there is nothing to say. The Shell writes the same
// dash in its `engine:` trace, and for the same reason: an empty cell reads as
// a bar that broke rather than as a Behavior nobody proposed.
const NONE = "—";

// Milliseconds until the next ambient wake, in the largest unit that still
// counts. Rounded up, so the bar reaches 0 when the wake is actually due
// rather than a second before it; `due` past that, because a wake the Engine
// has not got to yet is due, not late.
//
// Seconds only under a minute. `Pace` grows the ambient wait to two hours, and
// `wake 7200s` is both the widest thing this cell can hold and a number nobody
// reads as a duration. Three characters is the most any of these takes, which
// is what makes the bar's arithmetic below fit.
export function untilWake(ms) {
  if (ms === null || ms === undefined) {
    return NONE;
  }
  const secs = Math.ceil(ms / 1000);
  if (secs <= 0) {
    return "due";
  }
  if (secs < 60) {
    return `${secs}s`;
  }
  const mins = Math.ceil(secs / 60);
  return mins < 60 ? `${mins}m` : `${Math.ceil(mins / 60)}h`;
}

// One push, plus how much of its countdown is left, as the text of each cell.
//
// `status` is null before the first push arrives — the window can open between
// two changes — and every cell says so rather than sitting blank.
export function statusCells(status, msLeft) {
  return {
    behavior: status?.behavior ?? NONE,
    primitive: status?.primitive ?? NONE,
    animation: status?.animation ?? NONE,
    state: status?.state ?? NONE,
    // Nothing rather than an arrow before the first push: a sprite that has
    // not been reported on faces no way, and a guessed arrow is a lie the
    // width of the cell.
    facing: status ? (status.facing < 0 ? "←" : "→") : "",
    // One fact at a time, not both, because the window is 420 points wide and
    // this cell held the widest pair on the line. It costs nothing to say the
    // one that matters: a wake resets the ambient pace as it starts, so the
    // countdown under a turn on the wire is always the full wait over again
    // and tells the reader nothing they did not know a moment ago.
    director: status?.asking ? "thinking" : `wake ${untilWake(status ? msLeft : null)}`,
    happened: status?.happened ?? NONE,
  };
}
