// The one transient line above the Chat composer, as a rule rather than as
// DOM, so node can drive the orderings a window does not reproduce on demand.
// The strip is for the Harness's thinking only (ADR-0025).

// Precedence: a thought wins while there is one; the empty thought that ends a
// turn clears it; the user's next line clears it.

const COLLAPSED_KEY = "ai-buddy.chat.thought-collapsed";

export function createStrip(show, collapsed = false) {
  let view = { line: "", collapsed };

  function draw() {
    show({ ...view });
  }

  return {
    // A thought, or the empty line that says the turn stopped thinking.
    thinking(latest) {
      view = { ...view, line: latest ?? "" };
      draw();
    },

    // The next question, typed or from a session that was replaced. Whatever
    // the last turn left here has been read.
    asked() {
      view = { ...view, line: "" };
      draw();
    },

    toggle() {
      view = { ...view, collapsed: !view.collapsed };
      draw();
      return view.collapsed;
    },
  };
}

export function mountThoughtStrip(root, storage) {
  const text = root.querySelector(".thought-text");
  const toggle = root.querySelector(".thought-toggle");
  let collapsed = false;
  try {
    collapsed = storage?.getItem(COLLAPSED_KEY) === "true";
  } catch {
    // A blocked preference store should not hide a live thought.
  }

  const strip = createStrip((view) => {
    root.hidden = !view.line;
    root.classList.toggle("is-collapsed", view.collapsed);
    text.textContent = view.line;
    toggle.textContent = view.collapsed ? "Expand" : "Collapse";
    toggle.setAttribute("aria-expanded", String(!view.collapsed));
  }, collapsed);

  toggle.addEventListener("click", () => {
    const next = strip.toggle();
    try {
      storage?.setItem(COLLAPSED_KEY, String(next));
    } catch {
      // The control still works for this window when storage is unavailable.
    }
  });

  return strip;
}
