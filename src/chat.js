// The Chat surface: one window per Summoned Character Instance, drawn by
// ai-buddy rather than by whatever answers (ADR-0018). Four kinds of line —
// the user's turns, the answer as it arrives, a line the user drew out without
// typing, labelled with what it was reacting to, and a forwarded permission
// request with its options as buttons — plus the Shell's own notes, about a
// turn that produced nothing and about the boundary where the session behind
// this window was replaced. ADR-0018's tool-call one-liner waits on the Action
// Log getting a reader. It holds no authoritative state, like the overlay: the
// log is this session and only this session, including lines said before this
// window existed, and the Shell owns the session behind it. The Harness's
// thinking is the one thing drawn here that is not a line of the log, and
// ADR-0025 says why it is a strip above the composer instead.

import { stampWhen } from "./chat-stamp.js";
import { mindLine, statusCells } from "./chat-status.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const chat = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();

// The label is `chat-` and the Instance's id, which is how the Shell addressed
// this window and how everything sent from it names its Instance. Read from
// the label rather than passed in, because a webview cannot be handed
// arguments at creation and an event emitted then would reach no listener.
const instance = chat.label.replace(/^chat-/, "");

const log = document.getElementById("log");
const thought = document.getElementById("thought");
const empty = document.getElementById("empty");
const composer = document.getElementById("composer");
const line = document.getElementById("line");
const send = document.getElementById("send");

const promptTab = document.getElementById("prompt");
const promptText = document.getElementById("prompt-text");
const promptSaid = document.getElementById("prompt-said");
const promptSave = document.getElementById("prompt-save");
const promptConfirm = document.getElementById("prompt-confirm");
const promptCancel = document.getElementById("prompt-cancel");

// The status bar's cells, by the name `statusCells` gives each.
const cells = Object.fromEntries(
  ["behavior", "primitive", "animation", "state", "facing", "director", "happened"].map(
    (name) => [name, document.getElementById(`s-${name}`)],
  ),
);

// The WHO label on the Instance's own turns, filled in once the Shell says who
// this window belongs to.
let them = "";

// Last stamped instant in this window, so a line after midnight can say the
// new day once. #445.
let previousAt = null;

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
  const body = el("said");
  body.textContent = text;
  row.append(cluster, body);
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

// What the Harness is thinking, while the turn runs (ADR-0025). One line that
// each thought replaces, and that the turn's own end takes away: it is not a
// row, so it never joins the log, and nothing here is kept.
//
// The Shell sends the line to draw rather than the chunk it arrived in, and
// sends an empty one when the turn ends, so this window never has to work out
// whether a Harness is still thinking.
function thinking(latest) {
  thought.textContent = latest ?? "";
  thought.hidden = !latest;
}

function note(text) {
  const row = el("note");
  row.append(when(), document.createTextNode(text));
  return add(row);
}

// The rows still offering buttons, by request id. One request reaches every
// open window and only one of them takes the click, so the Shell's settled
// event is what retires the rest.
const asks = new Map();

// A request nothing can answer any more: answered here or in another window,
// cancelled with its turn, or gone with the Harness. The buttons go dead
// rather than the row, so the log still says what was asked.
//
// `option` is the one that won, which is not necessarily the one clicked here:
// two surfaces can draw one request and the wire drops every answer after the
// first, so a window that marked its own click would show a decision that was
// never taken. Marked here, on the Shell's word, or not at all.
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

// A permission request the Harness asked, drawn as the options it offered.
// Nothing is chosen here or in the Shell: a click is the only answer, and
// a turn that times out first is cancelled by the Shell, not decided
// (ADR-0018). The buttons stay disabled after the click, and the row reads as
// what was decided once the Shell says which option took it.
//
// Ignored the second time a request arrives: the Shell hands an unsettled
// request to a window that opens after it was asked, and this window may
// already have drawn it.
function asked(ask) {
  if (asks.has(ask.request)) {
    return null;
  }
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
  body.append(buttons);
  row.append(label, body);
  asks.set(ask.request, buttons);
  return add(row);
}

// Login commands for the three named Harnesses. Shown when the user clicks a
// connect button, since Settings owns which one is actually attached.
const LOGIN_COMMANDS = {
  claude: "claude /login",
  hermes: "hermes login",
  opencode: "opencode login",
};

// Whether anything can answer, and what to say when nothing can.
//
// SPEC gives this window the job of explaining how to connect something
// rather than failing, and four reasons nothing can answer need different
// messages: never configured is a thing to attach, switched off is a thing to
// turn back on, not signed in names the Harness and shows the login command,
// and ready hides the empty state. The composer is disabled rather than
// hidden, so the window reads as waiting rather than as broken.
function attached(opening) {
  // Fourth state: configured, enabled, but needs authentication (-32000).
  const needsAuth = opening.configured && opening.enabled && opening.login;
  const ready = opening.configured && opening.enabled && !opening.login;
  
  empty.hidden = ready;
  line.disabled = !ready;
  send.disabled = !ready;
  line.placeholder = ready ? `Ask ${opening.name}…` : "Nothing can answer yet";

  // Which of the four states: none configured, switched off, needs auth, or ready.
  document.getElementById("empty-none").hidden = opening.configured;
  document.getElementById("empty-off").hidden = !opening.configured || opening.enabled;
  document.getElementById("empty-auth").hidden = !needsAuth;

  // Fourth state: fill in the Harness name and login command. The command is
  // for the user's own terminal; ai-buddy never collects a credential (ADR-0010).
  if (needsAuth) {
    for (const node of document.querySelectorAll("#empty-auth .harness-name")) {
      node.textContent = opening.harness_name || opening.name;
    }
    const cmd = document.getElementById("login-command");
    if (cmd) {
      cmd.textContent = opening.login;
    }
  }
  
  return ready;
}

// Connect button clicks: spawn the login command for that Harness.
// The Harness authenticates itself; ai-buddy never collects a credential.
for (const btn of document.querySelectorAll(".connect-btn")) {
  btn.addEventListener("click", () => {
    const harness = btn.dataset.harness;
    const label = btn.querySelector(".connect-label").textContent;
    
    invoke("harness_login", { harness })
      .then(() => {
        note(`Starting ${label} login. Sign in through the ${label} window, then set AI_BUDDY_HARNESS=${harness} at launch.`);
      })
      .catch((why) => {
        console.error(`harness_login failed:`, why);
        note(`Could not start ${label} login: ${why}. Run \`${LOGIN_COMMANDS[harness]}\` in a terminal.`);
      });
  });
}


// Which tab is showing. The conversation and the prompt behind it are the two
// things this window holds, and they do not fit one above the other at 420
// points (ADR-0012).
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
// save. Saving throws the session away, so the second click is the one that
// does it — and never a keystroke, which would wipe the conversation
// mid-sentence (ADR-0012).
function askingToSave(asking) {
  promptSave.hidden = asking;
  promptConfirm.hidden = !asking;
  promptCancel.hidden = !asking;
}

function showPrompt(opening) {
  document.getElementById("personality").textContent =
    opening.personality || "This Character ships no personality.";
  // Said before it is hit as well as in the refusal after: the Shell owns the
  // number, so the tab reads it rather than restating it.
  document.getElementById("prompt-limit").textContent = opening.prompt_limit;
  if (promptText.value === savedPrompt) {
    promptText.value = opening.instance_prompt;
  }
  savedPrompt = opening.instance_prompt;
}

promptSave.addEventListener("click", () => {
  promptSaid.textContent = "";
  askingToSave(true);
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
      // that event is the one place a replacement is drawn (#476, ADR-0012).
    })
    .catch((why) => {
      promptSaid.textContent = String(why);
    });
});

function showWho(opening) {
  them = opening.name;
  document.getElementById("name").textContent = opening.name;
  document.getElementById("character").textContent = opening.character;
  // Refilled on every opening, not only the first: #480 pushes one when the
  // Completer source moves, and a mode label that keeps the mode it opened
  // with is the lie this was written to stop (#474).
  document.getElementById("mind-text").textContent = mindLine(opening);
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
      // The header too, not only whether anything can answer: this opening is
      // the one place a Harness that came up after the window opened is
      // noticed, and a header still saying `not running` over a live session
      // is the lie #474 is about.
      showWho(opening);
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

// The session behind this window was replaced, and the Shell says why (#476).
//
// The rows go with it. A transcript sitting above the composer is a claim that
// what is about to answer has read it, and after a reset that claim is false —
// the user writes a follow-up on three turns of context and is answered by
// something holding none. The rest of what this window was holding is just as
// stale: turns waiting on an answer that was abandoned with the session, and
// permission asks the old Harness will never hear back about.
//
// A note in their place, because a log that empties itself with nothing said
// reads as the app losing the conversation rather than as a new one starting.
// The Action Log is where the removed turns survive.
function newSession(why) {
  // Keeping `empty` is not tidiness: the empty-state panel is a child of the
  // log, and `attached()` reaches into it by id on every opening. Sweeping it
  // out with the rows leaves that lookup dereferencing null.
  log.replaceChildren(empty);
  thinking(null);
  waiting.length = 0;
  asks.clear();
  // A boundary is where a stamp should say the hour again rather than count
  // minutes from a line that is no longer on screen.
  previousAt = null;
  // The note that carried it went with the rest, so it may be owed again.
  loginSaid = null;
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
        // (ADR-0018). The label names what drew it out — a Summon, a Poke, or
        // nobody at all — because the log's grammar is a question with its
        // answer under it: an unlabelled line here reads as the answer to
        // whatever is above it, and a line labelled as unasked-for reads as a
        // bug when the user just double-clicked the sprite. It takes no
        // waiting turn for the same reason: that caret is on a question this
        // did not answer. The Shell writes the words, out of the vocabulary
        // the status bar draws below.
        said(`${them} · ${payload.reacting_to}`, payload.said, "them", payload.at);
        return;
      }
      const turn = waiting.shift();
      if (!turn) {
        // An answer with no question in this window: the Instance was asked
        // somewhere else, or this window opened after the line was sent.
        said(them, payload.said ?? "", "them", payload.at);
        return;
      }
      settled(turn.them);
      if (payload.said) {
        arrived(turn.them, payload.said);
      } else if (payload.error) {
        // The Harness answered, and the answer was an error — a model the
        // installed CLI will not serve, a signed-out agent. Static weights
        // took the turn either way, so the row looks like the one below; the
        // error is the only part the user can act on, and #514 is a day of
        // wakes spent because it was never said (ADR-0008).
        turn.them.remove();
        note(`The Harness reported an error: ${payload.error}`);
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
    "chat-thought",
    ({ payload }) => {
      thinking(payload);
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
  // #375, #473.
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
