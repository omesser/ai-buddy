// What the Chat surface's status bar draws, from one push and the clock
// (ADR-0010), and the header's line about which mind answers. Its own module
// because chat.js reaches window.__TAURI__ as it loads and cannot be imported
// outside a webview; this can, so it has a test.

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

// Which mind answers this window, for the header beside who you are talking
// to (#474). A statement and never a control: ADR-0010 leaves the bar for what
// our own layers are doing right now, and this is neither that nor something
// to press.
//
// The branches are `settings::harness_state`'s, in its order, so the two
// windows cannot disagree — and the order is what makes it honest. A Harness
// that is set and never came up is the state the user cannot otherwise see,
// and it is named before the session that a live one would show. The login
// command for an attached-but-not-signed-in Harness is named once here and
// never run; ADR-0010's seventh rule covers the endpoint's key.
export function mindLine(opening) {
  if (!opening) {
    return "";
  }
  if (!opening.enabled) {
    return "static weights";
  }
  const harness = opening.harness;
  if (!harness) {
    // Nothing rather than a bare separator: the Shell sends empty strings only
    // when it could not read its own inspect, and ` · ` is punctuation
    // pretending to be a fact.
    return opening.model && opening.host ? `${opening.model} · ${opening.host}` : "";
  }
  if (harness.login) {
    return `${harness.name} · not signed in — \`${harness.login}\``;
  }
  if (!harness.alive) {
    return `${harness.name} · not running`;
  }
  if (!harness.session) {
    return `${harness.name} · no session yet`;
  }
  // The head of the id, not the whole of it: what the session proves here is
  // that a live one exists, and a full UUID pushes the Instance's own name off
  // a 420-point header. Settings draws it in full, and the head of an id
  // Settings shows in full cannot disagree with it.
  return `${harness.name} · session ${harness.session.slice(0, 8)}`;
}
