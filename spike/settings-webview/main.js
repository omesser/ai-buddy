import { STATES } from "./ai-tab.js";
import { render } from "./settings.js";

const form = document.getElementById("form");
const picker = document.getElementById("state");
const wanted = new URLSearchParams(location.search).get("state");
let current = wanted in STATES ? wanted : "modelApi";

// Where `invoke("settings_event", …)` would go. The log is the payload
// the Rust side would fold into a SettingsPatch.
const emit = (event) => console.log("settings event", JSON.stringify(event));

function draw() {
  const { tab, view } = STATES[current];
  render(form, tab, view, emit);
  for (const b of picker.children) b.setAttribute("aria-pressed", String(b.dataset.state === current));
}

for (const [key, { label }] of Object.entries(STATES)) {
  const b = document.createElement("button");
  b.type = "button";
  b.dataset.state = key;
  b.textContent = label;
  b.addEventListener("click", () => { current = key; draw(); });
  picker.append(b);
}
draw();
