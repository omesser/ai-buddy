// The cue machine: five interactions, each with its own visual and its own
// sound (#277).
//
// Ported value for value from the reviewed proposal in docs/design/cues.html.
// Every frequency, envelope, gain and duration below was decided there; the
// keyframes and easing curves that go with them are in main.css.
//
// Which cue plays is not decided here — the Engine decides it, being the only
// place that knows both the verbs and the `Dragged` transitions, and sends the
// name on the frame. This draws and sounds what it is told. Every visual is one
// element added and removed on `animationend`; every sound is one oscillator
// with a gain envelope. Nothing is loaded, nothing persists.

const MASTER = 0.5;

// How long a Poke cue is treated as in flight, so a Summon can cancel it.
// `input.rs`'s DOUBLE_CLICK_MS: past it no Summon can belong to that Poke, and
// the record is only holding an element and a gain node alive.
export const POKE_WINDOW_MS = 400;

let context = null;
// A machine with no AudioContext, or one that throws, stays silent forever
// after the first failure: retrying every cue would be the same error sixty
// times a second. Visuals still play. #292.
let audioFailed = false;
let audioWarned = false;

function warnAudio(why) {
  if (audioWarned) return;
  audioWarned = true;
  console.warn("ai-buddy: cue audio is unavailable (" + why + "). Visuals still play.");
}

function audio() {
  if (audioFailed) return null;
  try {
    if (!context) {
      const Ctx = window.AudioContext || window.webkitAudioContext;
      if (!Ctx) {
        audioFailed = true;
        warnAudio("no AudioContext");
        return null;
      }
      context = new Ctx();
    }
    // Suspended until the page has been interacted with. Every cue follows a
    // press on the sprite, which the overlay receives whenever click-through is
    // off — so by the time one plays, the gesture that unlocks this has landed.
    if (context.state === "suspended") context.resume();
    return context;
  } catch (err) {
    audioFailed = true;
    warnAudio(err && err.message ? err.message : String(err));
    return null;
  }
}

// A voice is a gain node the cue can be cut through. Summon cuts a Poke that
// is still sounding; the cut is a 15 ms ramp so it does not click.
function voice() {
  const ac = audio();
  if (!ac) return null;
  const g = ac.createGain();
  g.gain.value = 0;
  g.connect(ac.destination);
  return {
    ac,
    g,
    cut() {
      const t = ac.currentTime;
      g.gain.cancelScheduledValues(t);
      g.gain.setValueAtTime(g.gain.value, t);
      g.gain.linearRampToValueAtTime(0, t + 0.015);
    },
  };
}

function tone(
  v,
  { type = "sine", from, to = from, at = 0, attack = 0.004, hold = 0, decay = 0.14, peak = 0.25, lowpass = null },
) {
  const { ac, g } = v;
  const t0 = ac.currentTime + at;
  const osc = ac.createOscillator();
  osc.type = type;
  osc.frequency.setValueAtTime(from, t0);
  if (to !== from) osc.frequency.exponentialRampToValueAtTime(to, t0 + attack + hold + decay);
  let out = osc;
  if (lowpass) {
    const f = ac.createBiquadFilter();
    f.type = "lowpass";
    f.frequency.value = lowpass;
    osc.connect(f);
    out = f;
  }
  // Per-note envelope on its own gain so two notes in one voice do not fight;
  // the voice gain above them is the one a cancel cuts.
  const env = ac.createGain();
  out.connect(env);
  env.connect(g);
  env.gain.setValueAtTime(0, t0);
  env.gain.linearRampToValueAtTime(peak * MASTER, t0 + attack);
  env.gain.setValueAtTime(peak * MASTER, t0 + attack + hold);
  env.gain.exponentialRampToValueAtTime(0.0005, t0 + attack + hold + decay);
  g.gain.setValueAtTime(1, t0);
  osc.start(t0);
  osc.stop(t0 + attack + hold + decay + 0.02);
}

const SOUNDS = {
  poke: (v) => tone(v, { from: 660, to: 600, decay: 0.14, peak: 0.25 }),
  summon: (v) => {
    tone(v, { type: "triangle", from: 523.25, attack: 0.008, decay: 0.13, peak: 0.22 });
    tone(v, { type: "triangle", from: 783.99, attack: 0.008, decay: 0.16, peak: 0.22, at: 0.11 });
  },
  menu: (v) => {
    tone(v, { type: "square", from: 320, attack: 0.002, decay: 0.035, peak: 0.12, lowpass: 1200 });
    tone(v, { type: "square", from: 240, attack: 0.002, decay: 0.035, peak: 0.08, lowpass: 1200, at: 0.06 });
  },
  pickup: (v) => tone(v, { from: 280, to: 720, attack: 0.006, decay: 0.14, peak: 0.2 }),
  drop: (v) => tone(v, { from: 160, to: 55, attack: 0.004, decay: 0.16, peak: 0.3 }),
  throw: (v) => tone(v, { from: 220, to: 55, attack: 0.004, decay: 0.2, peak: 0.48 }),
};

function el(cls, x, y) {
  const e = document.createElement("div");
  e.setAttribute("class", "cue " + cls);
  e.style.left = x + "px";
  e.style.top = y + "px";
  return e;
}

function reap(node, child) {
  // Remove on the last animation to end, so a stagger does not cut its siblings.
  let pending = child ? child.length : 1;
  const done = () => {
    if (--pending <= 0) node.remove();
  };
  (child || [node]).forEach((c) => c.addEventListener("animationend", done, { once: true }));
}

const VISUALS = {
  poke(stage, c) {
    const r = el("cue-ring", c.x, c.y);
    stage.append(r);
    reap(r);
    return [r];
  },
  summon(stage, c) {
    const a = el("cue-ring in", c.x, c.y);
    const b = el("cue-ring in second", c.x, c.y);
    stage.append(a, b);
    reap(a);
    reap(b);
    return [a, b];
  },
  menu(stage, c) {
    // Whole pixels: a ring this thin smears when its centre sits between them.
    const x = Math.round(c.x);
    const y = Math.round(c.y);
    const fill = el("cue-wedge", x, y);
    const edge = el("cue-wedge-edge", x, y);
    stage.append(fill, edge);
    // Advance the sweep by hand: a custom property cannot be keyframed
    // without @property, and this stays portable to the GTK WebKit.
    const t0 = performance.now();
    const sweepMs = 240;
    const ease = (p) => 1 - Math.pow(1 - p, 3);
    const step = (now) => {
      const p = Math.min(1, (now - t0) / sweepMs);
      const deg = (ease(p) * 360).toFixed(1) + "deg";
      fill.style.setProperty("--sweep", deg);
      edge.style.setProperty("--sweep", deg);
      if (p < 1) requestAnimationFrame(step);
      else {
        fill.classList.add("fading");
        edge.classList.add("fading");
        reap(fill);
        reap(edge);
      }
    };
    requestAnimationFrame(step);
    return [fill, edge];
  },
  pickup(stage, c) {
    const ds = [-30, -10, 10, 30].map((dx, i) => {
      const d = el("cue-dash", c.feetX + dx, c.feetY - 12);
      d.style.animationDelay = i * 25 + "ms";
      return d;
    });
    const g = document.createElement("div");
    g.className = "cue";
    g.append(...ds);
    stage.append(g);
    reap(g, ds);
    return [g];
  },
  drop(stage, c) {
    const p = el("cue-puff", c.feetX, c.feetY - 2);
    stage.append(p);
    reap(p);
    return [p];
  },
  throw(stage, c) {
    const p = el("cue-puff hard", c.feetX, c.feetY - 2);
    stage.append(p);
    reap(p);
    return [p];
  },
};

// Where a cue hangs off the sprite: the click cues at its centre, because a
// click landed on the art; pickup and drop at its feet, because that is the
// contact the gesture broke and made. Overlay coordinates, the same space the
// bubble is placed in.
export function cueAnchor(spriteRect) {
  return {
    x: spriteRect.x + spriteRect.width / 2,
    y: spriteRect.y + spriteRect.height / 2,
    feetX: spriteRect.x + spriteRect.width / 2,
    feetY: spriteRect.y + spriteRect.height,
  };
}

// The DOM and Web Audio halves, bound to one Instance's cue layer and to
// whatever it is currently anchored to. Kept apart from the machine below so
// node can drive the bookkeeping without a window or a sound card.
export function cueIo(layer, anchorOf) {
  return {
    draw(name) {
      const nodes = VISUALS[name](layer, anchorOf());
      return () => nodes.forEach((node) => node.remove());
    },
    sound(name) {
      // A throw here used to kill the frame listener, so a mute machine also
      // lost the visual. Swallow it: silence is the decided behaviour, not an
      // accident. #292.
      try {
        const v = voice();
        if (!v) return () => {};
        SOUNDS[name](v);
        return () => v.cut();
      } catch (err) {
        warnAudio(err && err.message ? err.message : String(err));
        return () => {};
      }
    },
  };
}

// The cue decisions, apart from the pixels and the oscillators. Two rules the
// transport and the double-click force:
//
// - A cue rides exactly one Engine tick, like `dialogue`, so it is latched
//   where every delivery is seen rather than read in the draw loop.
// - A double-click is two releases: the first emits a Poke before anything can
//   know a second is coming, so a Poke cue is always ~200 ms into playing when
//   the Summon lands. The Summon cancels it, visual and sound both, and one
//   double-click is seen and heard as one cue.
export function createCueMachine(io) {
  const schedule = io.schedule ?? ((fn, ms) => setTimeout(fn, ms));
  const cancel = io.cancel ?? ((id) => clearTimeout(id));

  // At most one, because only a Poke is ever cancelled and only the newest one
  // can still be sounding.
  let poke = null;

  function forget() {
    if (poke === null) return;
    cancel(poke.timer);
    poke = null;
  }

  return {
    // `placement` is this overlay's view of one tick: `cue` names the cue or is
    // null, `visible` is whether the Character is on screen at all, and `sound`
    // is whether it may be heard as well as seen.
    event(placement) {
      const name = placement.cue;
      // A hidden Character produces no cue of either kind: a sound with nothing
      // on screen to explain it is worse than silence (#84).
      if (!name || !placement.visible) return;

      if (name === "summon" && poke) {
        poke.stop();
        forget();
      }

      const stopVisual = io.draw(name);
      // Sound is gated and the visual is not. Do Not Disturb is already folded
      // into this flag, along with the Settings mute (#280) — a cue that cannot
      // be heard still has to be seen, or a muted buddy stops answering.
      // A throw from the audio graph is the same gate: keep the visual. #292.
      let cutSound = () => {};
      if (placement.sound) {
        try {
          cutSound = io.sound(name) || (() => {});
        } catch {
          // Mute-machine: silence, once, rather than taking the overlay with it.
        }
      }

      if (name === "poke") {
        // A second Poke replaces the record rather than joining it: the older
        // one is past cancelling by the time a Summon could arrive for this one.
        forget();
        poke = {
          stop: () => {
            stopVisual();
            cutSound();
          },
          timer: schedule(forget, POKE_WINDOW_MS),
        };
      }
    },
  };
}
