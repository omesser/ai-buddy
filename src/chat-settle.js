// What a `chat` payload does to a waiting caret. Its own module because
// chat.js reaches window.__TAURI__ as it loads and cannot be imported outside
// a webview; this can, so it has a test. #681.

export const MISSING_ANSWER = "No answer came back.";

export function createChatTurns() {
  const waiting = [];

  return {
    typed() {
      const turn = { speechDrawn: false, alreadyHasSpeechAhead: false };
      waiting.push(turn);
      return turn;
    },

    drop(turn) {
      const at = waiting.indexOf(turn);
      if (at >= 0) {
        waiting.splice(at, 1);
      }
    },

    popNewest() {
      return waiting.pop();
    },

    clear() {
      waiting.length = 0;
    },

    settle(payload) {
      const turn = waiting.shift();
      if (!turn) {
        return { action: "orphan", turn: null, said: payload.said ?? "" };
      }
      if (payload.said) {
        turn.speechDrawn = true;
        for (const leftover of waiting) {
          leftover.alreadyHasSpeechAhead = true;
        }
        return { action: "speech", turn, said: payload.said };
      }
      if (payload.error) {
        return {
          action: "error",
          turn,
          note: `The Harness reported an error: ${payload.error}`,
        };
      }
      // #681: empty is not "no answer" when Speech for this question is
      // already drawn, when a leftover caret sits behind that Speech, or when
      // the Shell named the settle as superseded rather than silent-failure.
      if (turn.speechDrawn || turn.alreadyHasSpeechAhead || payload.superseded) {
        return { action: "silent", turn };
      }
      return { action: "missing", turn, note: MISSING_ANSWER };
    },
  };
}
