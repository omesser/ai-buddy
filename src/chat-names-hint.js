// The window-names notice. Its own module because chat.js reaches
// window.__TAURI__ as it loads and cannot be imported under node --test.

export const HEADING = "Window names are off";

export const BODY =
  "The buddy knows where your windows are, not what they are. One switch in Settings turns on titles and application names together.";

export const BUTTONS = [
  { action: "open-settings", label: "Open Settings" },
  { action: "dismiss", label: "Don't show this again" },
];

const HIDDEN = { visible: false, heading: "", body: "", buttons: [] };

const SHOWN = { visible: true, heading: HEADING, body: BODY, buttons: BUTTONS };

function viewOf(hint) {
  return hint === "due" ? SHOWN : HIDDEN;
}

export function createNamesNotice({ act }) {
  let generation = 0;
  let hint = null;

  function receive(payload) {
    const nextHint = typeof payload === "string" ? payload : payload?.hint;
    const nextGen =
      payload !== null && typeof payload === "object" ? payload.generation : undefined;

    if (typeof nextGen === "number" && nextGen < generation) {
      return { changed: false, view: viewOf(hint) };
    }

    if (typeof nextGen === "number") {
      if (nextGen === generation && nextHint === hint) {
        return { changed: false, view: viewOf(hint) };
      }
      generation = nextGen;
    } else if (typeof nextHint === "string") {
      // A button answer has no generation. Count it ahead of the last push
      // so a replay of that push cannot put the notice back.
      generation += 1;
    }

    const changed = nextHint !== hint;
    hint = nextHint;
    return { changed, view: viewOf(hint) };
  }

  return {
    receive,

    async press(action) {
      return receive(await act(action));
    },
  };
}
