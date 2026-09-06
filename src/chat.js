// The Chat surface: one window per Summoned Character Instance, drawn by
// ai-buddy rather than by whatever answers (ADR-0010). Four kinds of line —
// the user's turns, the answer as it arrives, a line the user drew out without
// typing, labelled with what it was reacting to, and a forwarded permission
// request with its options as buttons — plus the Shell's own note about a turn
// that produced nothing. ADR-0010's tool-call one-liner waits on the Action Log
// getting a reader. It holds no authoritative state, like the overlay: the log
// is what has been said in this window, and the Shell owns the session behind
// it.

import { statusCells } from "./chat-status.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const chat = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();

// The label is `chat-` and the Instance's id, which is how the Shell addressed
// this window and how everything sent from it names its Instance. Read from
// the label rather than passed in, because a webview cannot be handed
// arguments at creation and an event emitted then would reach no listener.
const instance = chat.label.replace(/^chat-/, "");

const log = document.getElementById("log");
const empty = document.getElementById("empty");
const composer = document.getElementById("composer");
const line = document.getElementById("line");
const send = document.getElementById("send");

// The status bar's cells, by the name `statusCells` gives each.
const cells = Object.fromEntries(
  ["behavior", "primitive", "animation", "state", "facing", "director", "happened"].map(
    (name) => [name, document.getElementById(`s-${name}`)],
  ),
);

// The WHO label on the Instance's own turns, filled in once the Shell says who
// this window belongs to.
let them = "";

// The last thing the Shell said about the Spatial Layer, and when the ambient
// wake it named falls due. The Shell pushes that deadline once rather than a
// number every second: the seconds between are arithmetic, and arithmetic in
// here costs the frame loop nothing.
let status = null;
let wakeAt = null;

function paint() {
  const left = wakeAt === null ? null : Math.max(0, wakeAt - performance.now());
  const drawn = statusCells(status, left);
  for (const [name, node] of Object.entries(cells)) {
    node.textContent = drawn[name];
  }
}

// Turns waiting on an answer, oldest first. The Shell answers them in the
// order it took them and refuses a line typed while one is still waiting, so
// the oldest row takes the next answer and the newest takes a refusal.
const waiting = [];

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

function said(who, text, cls) {
  const row = el(`row ${cls}`);
  const label = el("who-label");
  label.textContent = who;
  const body = el("said");
  body.textContent = text;
  row.append(label, body);
  return add(row);
}

// A turn's answer, opened empty with a blinking caret and appended to as the
// answer arrives. The Shell hands over a finished Wake today, so it arrives in
// one piece and the append runs once; when the session grows chunks
// (the Harness client), each lands here and the caret stays until the last.
function opening_answer() {
  const row = said(them, "", "them");
  const caret = el("caret", "span");
  caret.textContent = "▍";
  row.querySelector(".said").append(caret);
  return row;
}

// Inserted before the caret rather than assigned over the line so far: the
// caret is a child of the same element, and writing textContent would take it
// out on the first chunk.
function arrived(row, text) {
  const body = row.querySelector(".said");
  body.insertBefore(document.createTextNode(text), body.querySelector(".caret"));
  log.scrollTop = log.scrollHeight;
}

function settled(row) {
  row.querySelector(".caret")?.remove();
}

function note(text) {
  const row = el("note");
  row.textContent = text;
  return add(row);
}

// A permission request the Harness asked, drawn as the options it offered.
// Nothing is chosen here or in the Shell: a click is the only answer, and
// a turn that times out first is cancelled by the Shell, not decided
// (ADR-0010). The buttons stay disabled after the click so the row reads as
// what was decided.
//
// ponytail: buttons freeze on the click, not on the Shell's cancel; a row the
// user never answered keeps live buttons whose click goes nowhere. A settled
// event would fix that, once there is more than this one row to settle.
function asked(ask) {
  const row = el("row ask");
  const label = el("who-label");
  label.textContent = `${them} · asks`;
  const body = el("said");
  body.textContent = ask.kind ? `${ask.kind}: ${ask.title}` : ask.title;
  const buttons = el("options");
  for (const option of ask.options) {
    const button = el("", "button");
    button.type = "button";
    button.textContent = option.name || option.id;
    button.addEventListener("click", () => {
      for (const other of buttons.querySelectorAll("button")) {
        other.disabled = true;
      }
      button.classList.add("chosen");
      invoke("permission_answer", { request: ask.request, option: option.id }).catch((why) => {
        console.error("chat: the answer did not reach the Harness:", why);
        note("That answer did not get through.");
      });
    });
    buttons.append(button);
  }
  body.append(buttons);
  row.append(label, body);
  return add(row);
}

// The login command the log last named, so re-asking `chat_opening` on every
// send does not repeat it.
let loginSaid = null;

// Whether anything can answer, and what to say when nothing can.
//
// SPEC gives this window the job of explaining how to connect something
// rather than failing, and the two reasons nothing can answer need different
// sentences: never configured is a thing to attach, switched off is a thing to
// turn back on. Both are fixed in Settings, and neither stops the buddy
// moving. The composer is disabled rather than hidden, so the window reads as
// waiting rather than as broken.
function attached(opening) {
  const ready = opening.configured && opening.enabled;
  empty.hidden = ready;
  line.disabled = !ready;
  send.disabled = !ready;
  line.placeholder = ready ? `Ask ${opening.name}…` : "Nothing can answer yet";

  // Which of the two states it is in, rather than which words to write: the
  // copy is markup, and what to attach is not what to switch back on.
  document.getElementById("empty-none").hidden = opening.configured;
  document.getElementById("empty-off").hidden = !opening.configured;

  // The third state: attached, and the Harness has nobody signed in. Said
  // once per command, in the log, because the fix is a command for the user's
  // own terminal and never a prompt of ours (ADR-0010).
  if (opening.login && opening.login !== loginSaid) {
    loginSaid = opening.login;
    note(`${opening.name} is attached but not signed in. Run \`${opening.login}\` in a terminal.`);
  }
  return ready;
}


function showWho(opening) {
  them = opening.name;
  document.getElementById("name").textContent = opening.name;
  document.getElementById("character").textContent = opening.character;
  for (const node of document.querySelectorAll(".i-name")) {
    node.textContent = opening.name;
  }
  for (const node of document.querySelectorAll(".i-character")) {
    node.textContent = opening.character;
  }
  if (!line.disabled) {
    line.placeholder = `Ask ${opening.name}…`;
  }
}

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
      if (!attached(opening)) {
        return;
      }
      line.value = "";
      const turn = { you: said("You", text, "you"), them: opening_answer() };
      waiting.push(turn);
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
  const at = waiting.indexOf(turn);
  if (at >= 0) {
    waiting.splice(at, 1);
  }
  turn.you.remove();
  turn.them.remove();
}

async function start() {
  // Addressed to this window's label. Not optional: a listener registered with
  // no target is an `Any` listener that hears every emit, so two Chat surfaces
  // would each render the other's answers.
  await listen(
    "chat",
    ({ payload }) => {
      if (payload.busy) {
        const refused = waiting.pop();
        if (refused) {
          drop(refused);
        }
        note("Still answering the last one — ask again when it lands.");
        return;
      }
      if (payload.reacting_to) {
        // A line the user did not type, which reaches the log as well as the
        // Speech bubble so the conversation has one place to be read
        // (ADR-0010). The label names what drew it out — a Summon, a Poke, or
        // nobody at all — because the log's grammar is a question with its
        // answer under it: an unlabelled line here reads as the answer to
        // whatever is above it, and a line labelled as unasked-for reads as a
        // bug when the user just double-clicked the sprite. It takes no
        // waiting turn for the same reason: that caret is on a question this
        // did not answer. The Shell writes the words, out of the vocabulary
        // the status bar draws below.
        said(`${them} · ${payload.reacting_to}`, payload.said, "them");
        return;
      }
      const turn = waiting.shift();
      if (!turn) {
        // An answer with no question in this window: the Instance was asked
        // somewhere else, or this window opened after the line was sent.
        said(them, payload.said ?? "", "them");
        return;
      }
      settled(turn.them);
      if (payload.said) {
        arrived(turn.them, payload.said);
      } else {
        // A turn that produced no line: the call failed and static weights
        // took over, which are silent by contract, or Do Not Disturb refused
        // the dialogue. Said out loud, because a log that stops is
        // indistinguishable from one still waiting.
        turn.them.remove();
        note("No answer came back.");
      }
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
    "chat-status",
    ({ payload }) => {
      status = payload;
      const ms = payload.wake_ms ?? null;
      wakeAt = ms === null ? null : performance.now() + ms;
      paint();
    },
    { target: chat.label },
  );

  // Name and Character only. The command is one-shot at start() for
  // whether anything can answer; a Character switch has to reach a
  // window that is already listening. #375.
  await listen(
    "chat-opening",
    ({ payload }) => {
      showWho(payload);
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
