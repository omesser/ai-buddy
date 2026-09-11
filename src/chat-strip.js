// The one transient line above the Chat composer, as a rule rather than as
// DOM, so node can drive the orderings a window does not reproduce on demand.
//
// ADR-0025 gave the strip to the Harness's thinking. It also carries the mark
// for a reply the token cap ended, which widens that decision from "what the
// model is thinking" to "status about the turn" — @omesser took that call
// knowingly and called it provisional (#610).
//
// Two things share one line, so the precedence has to be written down:
//
// - A thought wins while there is one. A thought means a turn is running, and
//   what is running matters more than how the last one ended.
// - The empty thought that ends a turn clears the thought and never the mark.
//   That clear fires at the same moment the mark appears — it is the end of
//   the very turn that was cut off — so a mark that raced it would be swept
//   away by it every time.
// - Nothing else takes the mark down. It stays, like any other line drawn
//   here, until the next turn starts: its first thought, or the user's next
//   line.
import { TRUNCATED_MARK } from "./bubble.js";

export function createStrip(show) {
  let thought = "";
  let mark = null;

  function draw() {
    show(thought || mark || "");
  }

  return {
    // A thought, or the empty line that says the turn stopped thinking.
    thinking(latest) {
      if (latest) {
        mark = null;
      }
      thought = latest ?? "";
      draw();
    },

    // A turn came back. Its words are a row of the log; this is the part that
    // says whether there is any more of them.
    answered(truncated) {
      thought = "";
      mark = truncated ? TRUNCATED_MARK : null;
      draw();
    },

    // The next question, typed or from a session that was replaced. Whatever
    // the last turn left here has been read.
    asked() {
      thought = "";
      mark = null;
      draw();
    },
  };
}
