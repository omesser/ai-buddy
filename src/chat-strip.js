// The one transient line above the Chat composer, as a rule rather than as
// DOM, so node can drive the orderings a window does not reproduce on demand.
// The strip is for the Harness's thinking only (ADR-0025).

// Precedence: a thought wins while there is one; the empty thought that ends a
// turn clears it; the user's next line clears it.

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
