// The Chat surface: one window per Summoned Character Instance, drawn by
// ai-buddy rather than by whatever answers (ADR-0018). Like the overlay it
// holds no authoritative state; the Shell owns the session behind it.

import { askSays, elicitSays } from "./chat-ask.js";
import { canAnswer, landingCopy } from "./chat-connect.js";
import { composerPlaceholder } from "./chat-placeholder.js";
import { planSteps } from "./chat-plan.js";
import { MISSING_ANSWER, createChatTurns } from "./chat-settle.js";
import { createStrip } from "./chat-strip.js";
import { stampWhen } from "./chat-stamp.js";
import { mindLine, plainStatus, statusCells } from "./chat-status.js";
import { appendReply, drawReply } from "./markdown.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const chat = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();

// The label is `chat-` and the Instance's id, which is how the Shell addresses
// this window. Read from the label because a webview cannot be handed arguments
// at creation, and an event emitted then would reach no listener.
const instance = chat.label.replace(/^chat-/, "");

const log = document.getElementById("log");
const thought = document.getElementById("thought");
const plan = document.getElementById("plan");
const empty = document.getElementById("empty");
const composer = document.getElementById("composer");
const line = document.getElementById("line");
const send = document.getElementById("send");

const promptTab = document.getElementById("prompt");
const promptText = document.getElementById("prompt-text");
const promptSaid = document.getElementById("prompt-said");
const promptSave = document.getElementById("prompt-save");
const promptDiscard = document.getElementById("prompt-discard");
const promptConfirm = document.getElementById("prompt-confirm");
const promptCancel = document.getElementById("prompt-cancel");

const cells = Object.fromEntries(
  ["behavior", "primitive", "animation", "state", "facing", "director", "happened"].map(
    (name) => [name, document.getElementById(`s-${name}`)],
  ),
);

const plainEl = document.getElementById("s-plain");

// The WHO label on the Instance's own turns, filled in once the Shell says who
// this window belongs to.
let them = "";

// Last stamped instant in this window, so a line after midnight can say the
// new day once.
let previousAt = null;

// The last thing the Shell said about the Spatial Layer, and when the ambient
// wake it named falls due. The Shell pushes that deadline once rather than a
// number every second; the arithmetic between costs the frame loop nothing.
let status = null;
let wakeAt = null;

function paint() {
  const left = wakeAt === null ? null : Math.max(0, wakeAt - performance.now());
  const drawn = statusCells(status, left);
  for (const [name, node] of Object.entries(cells)) {
    node.textContent = drawn[name];
  }
  plainEl.textContent = plainStatus(status, left);
}

// Turns waiting on an answer, oldest first. The Shell answers them in the
// order it took them and refuses a line typed while one is still waiting, so
// the oldest row takes the next answer and the newest takes a refusal.
const turns = createChatTurns();

function el(cls, tag) {
  const node = document.createElement(tag || "div");
  node.className = cls;
  return node;
}

function add(node) {
  log.append(node);
  log.scrollTop = log.scrollHeight;
  return node;
}

function when(at) {
  const instant = typeof at === "number" ? new Date(at) : new Date();
  const stamp = stampWhen(instant, previousAt);
  previousAt = instant;
  const node = el("when", "time");
  node.dateTime = stamp.datetime;
  node.title = stamp.title;
  node.textContent = stamp.label;
  return node;
}

function said(who, text, cls, at) {
  const row = el(`row ${cls}`);
  const cluster = el("who");
  const label = el("who-label");
  label.textContent = who;
  cluster.append(label, when(at));
  // Only what answered gets its Markdown drawn. The user's own turn
  // stays the characters they typed: they wrote punctuation, not a document.
  const body = el(cls === "them" ? "said md" : "said");
  if (cls === "them") {
    drawReply(body, text);
  } else {
    body.textContent = text;
  }
  row.append(cluster, body);
  return add(row);
}

// A turn's answer, opened empty with a blinking caret and appended to as it
// arrives. Today the Shell hands over a finished Wake, so the append runs once;
// with chunks, each lands here and the caret stays until the last.
function opening_answer() {
  const row = said(them, "", "them");
  const caret = el("caret", "span");
  caret.textContent = "▍";
  row.querySelector(".said").append(caret);
  return row;
}

function arrived(row, text) {
  appendReply(row.querySelector(".said"), text);
  log.scrollTop = log.scrollHeight;
}

function settled(row) {
  row.querySelector(".caret")?.remove();
}

// What the Harness is thinking, while the turn runs (ADR-0025). Not a row, so
// it never joins the log. The Shell sends the line to draw, and an empty one
// when the turn ends, so this never decides whether a Harness is still thinking.
const strip = createStrip((line) => {
  thought.textContent = line;
  thought.hidden = !line;
});

// The agent's steps, replaced whole on every update because that is how ACP
// sends them (#697). The current step is scrolled to, or a plan longer than
// the cap would leave the reader looking at step one.
function showPlan(steps) {
  plan.replaceChildren(
    ...steps.map((step) => {
      const row = el("step");
      row.dataset.status = step.status;
      row.dataset.priority = step.priority;
      row.textContent = step.text;
      return row;
    }),
  );
  plan.hidden = !steps.length;
  plan.querySelector('[data-status="in_progress"]')?.scrollIntoView({ block: "nearest" });
}

function note(text) {
  const row = el("note");
  row.append(when(), document.createTextNode(text));
  return add(row);
}

// A link in a reply opens in the user's browser. One listener on the log, not
// one per link: every reply redraws its row as chunks arrive. `open_link`
// decides the accepted scheme in Rust; `data-href` is untrusted text.
log.addEventListener("click", (event) => {
  const link = event.target.closest?.(".md-link[data-href]");
  if (!link) {
    return;
  }
  const where = link.dataset.href;
  invoke("open_link", { url: where }).catch((why) => {
    console.error("chat: that link did not open:", why);
    note(`That link did not open: ${why}.`);
  });
});

// The rows still offering buttons, by request id. One request reaches every
// open window and only one of them takes the click, so the Shell's settled
// event is what retires the rest.
const asks = new Map();

// A request nothing can answer any more. The buttons go dead, not the row, so
// the log still says what was asked. `option` is the winner on the Shell's word:
// the wire drops every answer after the first, so a local click may have lost.
function retire(request, option) {
  const buttons = asks.get(request);
  if (!buttons) {
    return;
  }
  asks.delete(request);
  for (const button of buttons.querySelectorAll("button")) {
    button.disabled = true;
    if (option && button.dataset.option === option) {
      button.classList.add("chosen");
    }
  }
}

// A permission request, drawn as the options the Harness offered. A click is
// the only answer; a timed-out turn is cancelled by the Shell, not decided
// (ADR-0018). A repeat arrives when a late-opening window is handed unsettled asks.
function asked(ask) {
  if (asks.has(ask.request)) {
    return null;
  }
  const row = el("row ask");
  const label = el("who-label");
  label.textContent = `${them} · asks`;
  const body = el("said");
  // Every word of this is untrusted and arrives as text, never as markup.
  // `chat-ask.js` decides what an ask says and how much of it.
  body.textContent = askSays(ask);
  const buttons = el("options");
  for (const option of ask.options) {
    const button = el("", "button");
    button.type = "button";
    button.textContent = option.name || option.id;
    button.dataset.option = option.id;
    button.addEventListener("click", () => {
      // Disabled at once so a second click cannot be sent, but nothing is
      // marked chosen until the Shell says what won: see `retire`.
      for (const other of buttons.querySelectorAll("button")) {
        other.disabled = true;
      }
      invoke("permission_answer", { request: ask.request, option: option.id }).catch((why) => {
        console.error("chat: the answer did not reach the Harness:", why);
        note("That answer did not get through.");
      });
    });
    buttons.append(button);
  }
  // Beside `.said`, not inside it: wrap-anywhere inherited there shrinks a
  // button to one character, and Yes and No become circles. #908.
  row.append(label, body, buttons);
  asks.set(ask.request, buttons);
  return add(row);
}

// One `elicitation/create` form. Options are the schema's first enum; Decline
// is always offered, because the protocol treats it as a valid answer.
function elicited(form) {
  if (asks.has(form.request)) {
    return null;
  }
  const row = el("row ask");
  const label = el("who-label");
  label.textContent = `${them} · asks`;
  const body = el("said");
  body.textContent = elicitSays(form);
  const buttons = el("options");
  for (const option of form.options ?? []) {
    const button = el("", "button");
    button.type = "button";
    button.textContent = option.name || option.value;
    button.dataset.option = option.value;
    button.addEventListener("click", () => {
      for (const other of buttons.querySelectorAll("button")) {
        other.disabled = true;
      }
      invoke("elicitation_answer", { request: form.request, value: option.value }).catch((why) => {
        console.error("chat: the answer did not reach the Harness:", why);
        note("That answer did not get through.");
      });
    });
    buttons.append(button);
  }
  const decline = el("", "button");
  decline.type = "button";
  decline.textContent = "Decline";
  decline.dataset.option = "decline";
  decline.addEventListener("click", () => {
    for (const other of buttons.querySelectorAll("button")) {
      other.disabled = true;
    }
    invoke("elicitation_answer", { request: form.request, value: null }).catch((why) => {
      console.error("chat: the answer did not reach the Harness:", why);
      note("That answer did not get through.");
    });
  });
  buttons.append(decline);
  row.append(label, body, buttons);
  asks.set(form.request, buttons);
  return add(row);
}

// Whether anything can answer, and what to say when nothing can. Ready is
// `canAnswer`: configured is not enough when the launcher is missing or the
// child never came up (#726). The composer is disabled rather than hidden,
// so it reads as waiting.
function attached(opening) {
  const ready = canAnswer(opening);
  const isHttpMode = opening.configured && !opening.harness_name;

  empty.hidden = ready;
  line.disabled = !ready;
  send.disabled = !ready;
  line.placeholder = composerPlaceholder(opening);

  const landing = document.getElementById("landing");
  const httpEmpty = document.getElementById("empty-http");
  const httpOff = document.getElementById("empty-http-off");

  landing.hidden = true;
  httpEmpty.hidden = true;
  httpOff.hidden = true;

  if (ready) {
    return true;
  }

  if (isHttpMode) {
    if (opening.enabled) {
      httpEmpty.hidden = false;
    } else {
      httpOff.hidden = false;
    }
  } else {
    landing.hidden = false;

    const title = document.getElementById("landing-title");
    const lede = document.getElementById("landing-lede");
    const command = document.getElementById("landing-command");
    const hint = document.getElementById("landing-hint");
    const copy = landingCopy(opening);

    title.textContent = copy.title;
    lede.textContent = copy.lede;
    if (copy.command) {
      command.textContent = copy.command;
      command.hidden = false;
      hint.textContent = copy.hint;
      hint.hidden = false;
    } else {
      command.hidden = true;
      hint.hidden = true;
    }
  }

  return ready;
}

// Connect button: make that Harness the Completer source. The landing
// and header paint from the opening `ReloadChat` pushes. The click never
// starts the sign-in: the Harness authenticates itself in the user's own
// terminal, and ai-buddy holds no credential.
for (const btn of document.querySelectorAll(".connect-btn")) {
  btn.addEventListener("click", () => {
    const harness = btn.dataset.harness;
    const label = btn.querySelector(".connect-label").textContent;

    // Nothing is repainted here: the pick goes through `SettingsSession::apply`,
    // whose `ReloadChat` pushes a full opening to the `chat-opening` listener.
    // A second read from this side would race that push.
    invoke("select_harness", { harness }).catch((why) => {
      console.error(`connect failed:`, why);
      note(`Could not connect to ${label}: ${why}.`);
    });
  });
}

const settingsBtn = document.getElementById("settings-btn");
if (settingsBtn) {
  settingsBtn.addEventListener("click", () => {
    invoke("show_settings").catch((err) => {
      console.error("Failed to open Settings:", err);
    });
  });
}


// Which tab is showing. The conversation and the prompt behind it are the two
// things this window holds, and they do not fit one above the other at 420
// points (`main.rs`).
function showTab(name) {
  const prompt = name === "prompt";
  log.hidden = prompt;
  composer.hidden = prompt;
  promptTab.hidden = !prompt;
  for (const [id, on] of [
    ["tab-chat", !prompt],
    ["tab-prompt", prompt],
  ]) {
    const tab = document.getElementById(id);
    tab.setAttribute("aria-selected", String(on));
  }
  if (prompt) {
    promptText.focus();
  }
}

document.getElementById("tab-chat").addEventListener("click", () => showTab("chat"));
document.getElementById("tab-prompt").addEventListener("click", () => showTab("prompt"));

// The Instance Prompt as the Shell last told us it stands. An opening pushed
// while the user is mid-sentence must not take the sentence: only text that
// still matches what was saved is replaced.
let savedPrompt = "";

// Whether the Save button is asking for confirmation rather than offering to
// save. Saving throws the session away, so the second click does it, and never
// a keystroke, which would wipe the conversation mid-sentence (ADR-0012).
function askingToSave(asking) {
  promptSave.hidden = asking;
  promptConfirm.hidden = !asking;
  promptCancel.hidden = !asking;
  // Discard is a dirty-pair twin of Save, not of Cancel: Cancel aborts the
  // confirm, Discard would restore the field mid-ask and compete with it.
  promptDiscard.hidden = asking;
  syncPromptActions();
}

function promptDirty() {
  return promptText.value !== savedPrompt;
}

// Clean Save used to open the confirm and wipe the session for no change.
// Disabled until the field differs from what the Shell last saved.
function syncPromptActions() {
  const confirming = !promptConfirm.hidden;
  const dirty = promptDirty();
  promptSave.disabled = confirming || !dirty;
  promptDiscard.disabled = confirming || !dirty;
}

function fillFrozen(id, text) {
  const el = document.getElementById(id);
  const written = (text ?? "").trim();
  el.classList.toggle("is-empty", !written);
  // "Empty", not a collapsed box: Blank AI empties a layer rather than hiding it.
  el.textContent = written || "Empty";
}

function showPrompt(opening) {
  fillFrozen("instructions", opening.instructions);
  fillFrozen("personality", opening.personality);
  // Said before it is hit as well as in the refusal after: the Shell owns the
  // number, so the tab reads it rather than restating it.
  document.getElementById("prompt-limit").textContent = opening.prompt_limit;
  if (promptText.value === savedPrompt) {
    promptText.value = opening.instance_prompt;
  }
  savedPrompt = opening.instance_prompt;
  syncPromptActions();
}

promptText.addEventListener("input", syncPromptActions);
promptText.addEventListener("change", syncPromptActions);

promptSave.addEventListener("click", () => {
  if (!promptDirty()) {
    return;
  }
  promptSaid.textContent = "";
  askingToSave(true);
});

promptDiscard.addEventListener("click", () => {
  promptText.value = savedPrompt;
  promptSaid.textContent = "";
  askingToSave(false);
});

promptCancel.addEventListener("click", () => askingToSave(false));

promptConfirm.addEventListener("click", () => {
  askingToSave(false);
  // Refused by the Shell rather than cut here: the bound is one number, in one
  // place, and the words that did not fit are still in the box to be cut by
  // the person who wrote them.
  invoke("chat_prompt", { instance, text: promptText.value })
    .then(() => {
      savedPrompt = promptText.value.trim();
      promptText.value = savedPrompt;
      promptSaid.textContent = "Saved.";
      // The log is cleared by `chat-session`: saving reopens the session, and
      // that event is the one place a replacement is drawn (ADR-0012).
      syncPromptActions();
    })
    .catch((why) => {
      promptSaid.textContent = String(why);
      syncPromptActions();
    });
});

function showWho(opening) {
  them = opening.name;
  document.getElementById("name").textContent = opening.name;
  document.getElementById("character").textContent = opening.character;
  // Refilled on every opening, not only the first: the Completer source can
  // move, and a mode label that keeps the mode it opened with is a lie.
  document.getElementById("mind-text").textContent = mindLine(opening);
  for (const node of document.querySelectorAll(".i-name")) {
    node.textContent = opening.name;
  }
  for (const node of document.querySelectorAll(".i-character")) {
    node.textContent = opening.character;
  }
  if (!line.disabled) {
    line.placeholder = composerPlaceholder(opening);
  }
}

// A textarea does not submit on Enter. Enter still sends, the whole muscle
// memory of this window; Shift+Enter types the newline. `isComposing` is the
// IME's Enter accepting a candidate, and sending there would cut the word.
line.addEventListener("keydown", (event) => {
  if (event.key !== "Enter" || event.shiftKey || event.isComposing) {
    return;
  }
  // Otherwise the newline lands in the field as well as sending the turn.
  event.preventDefault();
  composer.requestSubmit();
});

composer.addEventListener("submit", (event) => {
  event.preventDefault();
  const text = line.value.trim();
  if (!text) {
    return;
  }

  // Asked again on every send rather than subscribed to: a Completer attached
  // in Settings while this window is open has to reach it, and nothing else
  // here needs to know the moment it changes.
  invoke("chat_opening", { instance })
    .then((opening) => {
      // The header too, not only whether anything can answer: this opening is
      // where a Harness that came up after the window opened is noticed, and a
      // header still saying `not running` over a live session is a lie.
      showWho(opening);
      if (!attached(opening)) {
        return;
      }
      line.value = "";
      // The last turn's mark has been read; this is the next question.
      strip.asked();
      const turn = turns.typed();
      turn.you = said("You", text, "you");
      turn.them = opening_answer();
      return invoke("chat_send", { instance, text }).catch((why) => {
        drop(turn);
        throw why;
      });
    })
    .catch((why) => {
      console.error("chat: the line did not reach the frame loop:", why);
      note("That did not get through.");
    });
});

// A line that will never be answered takes its rows with it, rather than
// leaving the user looking at a question in the log that nothing is working on.
function drop(turn) {
  turns.drop(turn);
  turn.you.remove();
  turn.them.remove();
}

// The session behind this window was replaced, and the Shell says why. The rows
// go: a transcript above the composer claims that what answers next has read
// it. A note in their place, or the log reads as the app losing the conversation.
function newSession(why) {
  // Keeping `empty` is not tidiness: the empty-state panel is a child of the
  // log, and `attached()` reaches into it by id on every opening. Sweeping it
  // out with the rows leaves that lookup dereferencing null.
  log.replaceChildren(empty);
  strip.asked();
  // Not a child of the log, so replacing the rows above does not clear it.
  showPlan([]);
  turns.clear();
  asks.clear();
  // A boundary is where a stamp should say the hour again rather than count
  // minutes from a line that is no longer on screen.
  previousAt = null;
  note(`New session — ${why}. Nothing said earlier is in it.`);
}

async function start() {
  // Addressed to this window's label. Not optional: a listener registered with
  // no target is an `Any` listener that hears every emit, so two Chat surfaces
  // would each render the other's answers.
  await listen(
    "chat",
    ({ payload }) => {
      if (payload.you) {
        said("You", payload.said ?? "", "you", payload.at);
        return;
      }
      // A refusal is a note, not the strip: the strip is for thinking only (ADR-0025).
      if (payload.busy) {
        const refused = turns.popNewest();
        if (refused) {
          drop(refused);
        }
        note("Still answering the last one — ask again when it lands.");
        return;
      }
      if (payload.reacting_to) {
        // A line the user did not type, in the log as well as the bubble so the
        // conversation has one place to be read (ADR-0018). Labelled with what
        // drew it out, or it reads as the answer to whatever is above it.
        said(`${them} · ${payload.reacting_to}`, payload.said, "them", payload.at);
        return;
      }
      const outcome = turns.settle(payload);
      if (outcome.action === "orphan") {
        // An answer with no question in this window: the Instance was asked
        // somewhere else, or this window opened after the line was sent.
        said(them, outcome.said, "them", payload.at);
        return;
      }
      const turn = outcome.turn;
      settled(turn.them);
      if (outcome.action === "speech") {
        arrived(turn.them, outcome.said);
      } else if (outcome.action === "error") {
        // The Harness answered with an error. Static weights took the turn
        // either way, so the row looks like the one below; the error is the
        // only part the user can act on (ADR-0008).
        turn.them.remove();
        note(outcome.note);
      } else if (outcome.action === "silent") {
        turn.them.remove();
      } else {
        // No line: the call failed and static weights took over, silent by
        // contract, or Do Not Disturb refused. Said out loud, because a log
        // that stops is indistinguishable from one still waiting.
        turn.them.remove();
        note(MISSING_ANSWER);
      }
    },
    { target: chat.label },
  );

  await listen(
    "chat-thought",
    ({ payload }) => {
      strip.thinking(payload);
    },
    { target: chat.label },
  );

  await listen(
    "chat-plan",
    ({ payload }) => {
      showPlan(planSteps(payload));
    },
    { target: chat.label },
  );

  await listen(
    "chat-permission",
    ({ payload }) => {
      asked(payload);
    },
    { target: chat.label },
  );

  await listen(
    "chat-elicitation",
    ({ payload }) => {
      elicited(payload);
    },
    { target: chat.label },
  );

  await listen(
    "chat-permission-settled",
    ({ payload }) => {
      retire(payload.request, payload.option);
    },
    { target: chat.label },
  );

  await listen(
    "chat-status",
    ({ payload }) => {
      status = payload;
      const ms = payload.wake_ms ?? null;
      wakeAt = ms === null ? null : performance.now() + ms;
      paint();
    },
    { target: chat.label },
  );

  await listen(
    "chat-session",
    ({ payload }) => {
      newSession(payload);
    },
    { target: chat.label },
  );

  // Full opening, not only name and Character: a Director or Completer-source
  // change has to re-run `attached()` on a window that is already listening.
  await listen(
    "chat-opening",
    ({ payload }) => {
      showWho(payload);
      attached(payload);
      showPrompt(payload);
    },
    { target: chat.label },
  );

  // Both listeners are up, so the state as it stands can be asked for. The bar
  // is pushed on change and a window opened between two of them would sit at
  // dashes until the sprite next did something different.
  invoke("chat_ready", { instance }).catch((why) => {
    console.error("chat: the status bar could not ask for a first push:", why);
  });

  const opening = await invoke("chat_opening", { instance });
  showWho(opening);
  attached(opening);
  showPrompt(opening);
  line.focus();
}

// Only the countdown moves between pushes, and it moves once a second.
paint();
setInterval(paint, 1000);

start().catch((why) => {
  // Not knowing who this window belongs to or whether anything can answer
  // makes it a field that takes lines nobody reads, so say so rather than
  // showing an empty log.
  console.error("ai-buddy could not open the Chat surface:", why);
  note("This window could not reach ai-buddy.");
});
