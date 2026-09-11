// The one transient line above the Chat composer, as a rule rather than as
// DOM, so node can drive the orderings a window does not reproduce on demand.
//
// ADR-0025 gave the strip to the Harness's thinking. The strip is for
// thinking only; the truncation mark rides in the remembered text (session +
// Chat history), not here (#610).
//
// Precedence:
//
// - A thought wins while there is one. A thought means a turn is running.
// - The empty thought that ends a turn clears the thought.
// - The user's next line clears the thought.

export function createStrip(show) {
  let thought = "";

  function draw() {
    show(thought || "");
  }

  return {
    // A thought, or the empty line that says the turn stopped thinking.
    thinking(latest) {
      thought = latest ?? "";
      draw();
    },

    // The next question, typed or from a session that was replaced. Whatever
    // the last turn left here has been read.
    asked() {
      thought = "";
      draw();
    },
  };
}
