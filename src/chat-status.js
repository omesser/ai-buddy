// What the Chat surface's status bar draws, from one push and the clock
// (ADR-0010). Its own module because chat.js reaches window.__TAURI__ as it
// loads and cannot be imported outside a webview; this can, so it has a test.

// What a cell says when there is nothing to say — the dash the Shell's
// `engine:` trace writes. A blank cell reads as a bar that broke.
const NONE = "—";

// Milliseconds until the next ambient wake, in the largest unit that still
// counts. Rounded up, and `due` past zero: a wake the Engine has not got to
// yet is due, not late. Changing unit rather than growing keeps this to three
// characters, which is what makes the bar's width budget fit — `Pace` reaches
// two hours, and `wake 7200s` is both too wide and unreadable as a duration.
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
// `status` is null until the first arrives, and every cell says so.
export function statusCells(status, msLeft) {
  return {
    behavior: status?.behavior ?? NONE,
    primitive: status?.primitive ?? NONE,
    animation: status?.animation ?? NONE,
    state: status?.state ?? NONE,
    // Nothing rather than an arrow before the first push: a sprite nothing has
    // reported on faces no way, and a guessed arrow is a lie.
    facing: status ? (status.facing < 0 ? "←" : "→") : "",
    // One fact, not both: this cell held the widest pair on the line. It costs
    // nothing — a wake resets the ambient pace as it starts, so the countdown
    // under a turn on the wire is always the full wait over again.
    director: status?.asking ? "thinking" : `wake ${untilWake(status ? msLeft : null)}`,
    happened: status?.happened ?? NONE,
  };
}
