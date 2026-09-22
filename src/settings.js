// One interpreter of `form::describe()`, in two halves. `controls(tab, values)`
// is pure: the tab as a flat list of {role, id, label, value, frozen} in render
// order, the same rows `ax-settings.swift dump` reads, so tests need no window.
// `render(root, tab, values, emit)` is the DOM half; a redraw is render() again.

// `emit` routes events to the `settings_event` Tauri command. A secure field
// emits `set_text` rather than a verb of its own because `FormRow::SecureField`
// writes a `TextField`.

// `values` carries one scalar per row id, so a list of Instances or excluded
// applications arrives as lines, the way the AppKit block shows them. An array
// is taken as already split.
function listItems(value) {
  return Array.isArray(value) ? value : String(value ?? "").split("\n").filter(Boolean);
}

export function controls(tab, values) {
  const out = [];
  const push = (role, id, label, value, frozen) => out.push({ role, id, label, value, frozen });

  for (const section of tab.sections) {
    push("heading", null, section.heading, null, false);

    for (const row of section.rows) {
      switch (row.type) {
        case "Checkbox":
          push("checkbox", row.id, row.label, Boolean(values[row.id]), row.frozen);
          break;
        case "TextField":
          push("textfield", row.id, row.label ?? null, values[row.id] ?? "", row.frozen);
          break;
        case "SecureField":
          // A stored key never reaches a renderer: what `values` holds for this
          // row describes whether one is set, and it belongs in the placeholder.
          push("securefield", row.id, row.label ?? null, "", row.frozen);
          break;
        case "Popup":
          push("popup", row.id, row.label ?? null, choice(row, values), row.frozen);
          break;
        case "Multiline":
          push("textarea", row.id, row.label ?? null, values[row.id] ?? "", !row.editable);
          break;
        case "InspectBlock":
          push("statictext", row.id, row.label ?? null, values[row.id] ?? "", true);
          break;
        case "InspectPath":
          push("statictext", row.id, null, values[row.id] ?? "", true);
          break;
        case "List":
          for (const item of listItems(values[row.id])) {
            push("statictext", row.id, item, item, false);
            push("button", row.id, row.dismiss_label, item, false);
          }
          break;
        case "Composite":
          for (const control of row.controls) {
            if (control.type === "Button") {
              push("button", control.id, control.label, null, control.frozen);
            } else if (control.type === "Popup") {
              push("popup", control.id, null, choice(control, values), control.frozen);
            } else {
              push("textfield", control.id, null, values[control.id] ?? "", false);
            }
          }
          break;
        default:
          // A variant added to `FormRow` and not to this switch shows up as a
          // row the tests can name, rather than as a row that quietly vanishes.
          push("unknown", row.id ?? null, row.type, null, false);
      }
    }
  }

  return out;
}

function choice(row, values) {
  return values[row.id] ?? row.options[0] ?? "";
}

// `options` empty is `describe()`'s contract: the form leaves those choices to
// the renderer, which is how the Character popup gets the installed packages.
// Until the snapshot carries them, the value in force is the only option.
function options(row, values) {
  return row.options.length > 0 ? row.options : [choice(row, values)];
}

// --- DOM -------------------------------------------------------------------

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "text") node.textContent = value;
    else if (value === true) node.setAttribute(key, "");
    else node.setAttribute(key, value);
  }
  node.append(...children.filter((child) => child !== null && child !== undefined));
  return node;
}

// "What is this?" is a <details>: the browser owns open and closed, the closed
// state reserves no space, and a reader announces it as a disclosure.
function disclosure(text) {
  return text
    ? el("details", { class: "set-disclosure" }, el("summary", { text: "What is this?" }), el("p", { text }))
    : null;
}

function help(text) {
  return text ? el("p", { class: "set-help", text }) : null;
}

function status(text) {
  return text ? el("p", { class: "set-status", text }) : null;
}

function notes(row) {
  return [help(row.help), status(row.status), disclosure(row.disclosure)];
}

// The control sits inside its label, the way a checkbox row already does.
// WebKit publishes a label holding nothing but text as an AXStaticText carrying
// that text with its own text run beneath, so the row's name reached the
// accessibility tree twice and a dump read the copy as the row's control. A
// label holding a control is an AXGroup instead. #706.
function labelled(row, control, extra = []) {
  const id = `set-f-${row.id}`;
  control.id = id;
  return el(
    "div",
    { class: `set-row${row.frozen ? " set-is-frozen" : ""}`, "data-row": row.id },
    row.label ? el("label", { for: id }, el("span", { text: row.label }), control) : control,
    ...extra,
  );
}

function popup(row, values, frozen) {
  const select = el("select", { disabled: frozen });
  const value = values[row.id];
  for (const option of options(row, values)) {
    select.append(el("option", { value: option, selected: option === value, text: option }));
  }
  select.dataset.id = row.id;
  return select;
}

function drawRow(row, values, emit) {
  switch (row.type) {
    case "Checkbox": {
      const input = el("input", { type: "checkbox", disabled: row.frozen });
      input.checked = Boolean(values[row.id]);
      input.addEventListener("change", () => emit({ set_bool: row.id, value: input.checked }));
      return el(
        "div",
        { class: `set-row set-check${row.frozen ? " set-is-frozen" : ""}`, "data-row": row.id },
        el("label", {}, input, el("span", { text: row.label })),
        ...notes(row),
      );
    }
    case "TextField": {
      const input = el("input", {
        type: "text",
        placeholder: row.placeholder,
        readonly: row.frozen,
        "aria-readonly": row.frozen ? "true" : null,
      });
      input.value = values[row.id] ?? "";
      // Batched rows stay in the widget until Apply. A blur here would write
      // the file and retarget before Cancel could restore the row (#663).
      if (!row.batched) {
        input.addEventListener("blur", () => emit({ set_text: row.id, value: input.value, batched: row.batched }));
      }
      return labelled(row, input, notes(row));
    }
    case "SecureField": {
      // What the snapshot carries for a secure row is whether a key is stored,
      // under `<id>_placeholder` — the row's own value is always empty, because
      // the secret never leaves the store (ADR-0010).
      const input = el("input", {
        type: "password",
        placeholder: values[`${row.id}_placeholder`] ?? "",
        readonly: row.frozen,
        autocomplete: "off",
      });
      input.addEventListener("blur", () => emit({ set_text: row.id, value: input.value }));
      return labelled(row, input, [status(row.status)]);
    }
    case "Popup": {
      const select = popup(row, values, row.frozen);
      if (!row.batched) {
        select.addEventListener("change", () => emit({ set_text: row.id, value: select.value }));
      }
      return labelled(row, select, notes(row));
    }
    case "Multiline": {
      const area = el("textarea", {
        rows: "4",
        readonly: !row.editable,
        "aria-readonly": row.editable ? null : "true",
      });
      area.value = values[row.id] ?? "";
      area.addEventListener("blur", () => emit({ set_text: row.id, value: area.value }));
      return labelled({ ...row, frozen: !row.editable }, area, [help(row.help), disclosure(row.disclosure)]);
    }
    case "InspectBlock": {
      // tabindex, because the block scrolls and a keyboard has to reach it.
      const block = el("pre", { class: "set-inspect", tabindex: "0", text: values[row.id] ?? "" });
      return el(
        "div",
        { class: "set-row", "data-row": row.id },
        row.label ? el("span", { class: "set-label", text: row.label }) : null,
        block,
        ...notes(row),
      );
    }
    case "InspectPath":
      return el(
        "div",
        { class: "set-row", "data-row": row.id },
        el("p", { class: "set-path", text: values[row.id] ?? "" }),
      );
    case "List": {
      const list = el("ul", { class: "set-list" });
      for (const item of listItems(values[row.id])) {
        const dismiss = el("button", { type: "button", text: row.dismiss_label, "data-item": item });
        dismiss.addEventListener("click", () => emit({ dismiss: row.id, value: item }));
        list.append(el("li", {}, el("span", { text: item }), dismiss));
      }
      return el(
        "div",
        { class: "set-row", "data-row": row.id },
        list,
        help(row.help),
        disclosure(row.disclosure),
      );
    }
    case "Composite": {
      const line = el("div", { class: "set-controls" });
      for (const control of row.controls) {
        if (control.type === "Button") {
          const button = el("button", {
            type: "button",
            disabled: control.frozen,
            text: control.label,
            "data-id": control.id,
          });
          button.addEventListener("click", () => {
            const payload = { press: control.id };
            if (control.id === "director_apply") {
              const root = button.closest('[role="tabpanel"]') ?? button.getRootNode();
              payload.draft = directorDraft(root);
            }
            emit(payload);
          });
          line.append(button);
        } else if (control.type === "Popup") {
          const select = popup(control, values, control.frozen);
          select.addEventListener("change", () =>
            emit({ pick: control.id, value: select.value, fills: control.fills?.row ?? null }),
          );
          line.append(select);
        } else {
          const input = el("input", { type: "text", placeholder: control.placeholder, "data-id": control.id });
          input.value = values[control.id] ?? "";
          input.addEventListener("blur", () => emit({ set_text: control.id, value: input.value }));
          line.append(input);
        }
      }
      return el(
        "div",
        { class: "set-row", "data-row": row.id },
        line,
        help(row.help),
        disclosure(row.disclosure),
      );
    }
    default:
      return el("div", { class: "set-row set-unknown", text: `unrendered row type ${row.type}` });
  }
}

function rowValue(root, id) {
  const row = root.querySelector?.(`[data-row="${id}"]`);
  const control = row?.querySelector("input, select, textarea");
  return control ? control.value : "";
}

function directorDraft(root) {
  return {
    director_base_url: rowValue(root, "director_base_url"),
    director_model: rowValue(root, "director_model"),
    director_api_key: rowValue(root, "director_api_key"),
    harness: rowValue(root, "harness"),
    harness_command: rowValue(root, "harness_command"),
  };
}

export function render(root, tab, values, emit = () => {}) {
  root.replaceChildren();
  const footer = document.getElementById("set-footer");
  if (footer) footer.replaceChildren();

  for (const section of tab.sections) {
    const node = el("section", { class: "set-section" }, el("h2", { text: section.heading }));
    if (section.comment) node.append(el("p", { class: "set-comment", text: section.comment }));
    if (section.status) node.append(status(section.status));
    if (section.disclosure) node.append(disclosure(section.disclosure));
    for (const row of section.rows) {
      if (row.type === "Composite" && row.id === "director_actions") {
        if (footer) footer.append(drawRow(row, values, emit));
      } else {
        node.append(drawRow(row, values, emit));
      }
    }
    root.append(node);
  }
}

// Process a settings_event response into an outcome the page can act on.
// Exported for testing; the page wires it through invokeSettingsEvent.
export function processResponse(response) {
  switch (response.action) {
    case "refresh":
      return true;
    case "fill":
      return { fill: { id: response.id, value: response.value } };
    case "clear_key":
      return { clearKey: true };
    case "reset":
      return { reset: true };
    case "run":
      return { run: response.operation };
    case "nothing":
    default:
      return false;
  }
}

export async function handleEvent(payload) {
  const response = await invokeSettingsEvent(payload);
  return processResponse(response);
}

// Native Settings copies in the window controller. The webview only gets
// Outcome::Run, so the page writes the string it already shows. #855.
const COPY_RUN_FIELDS = Object.freeze({
  copy_byo_snippet: "byo_snippet",
  copy_byo_token: "byo_token",
});

const COPY_PRESS_RUN = Object.freeze({
  byo_copy: "copy_byo_snippet",
  byo_copy_token: "copy_byo_token",
});

export async function writeRunClipboard(run, values, writeText) {
  const field = COPY_RUN_FIELDS[run];
  if (field === undefined) return;
  const text = values[field] ?? "";
  if (text === "") return;
  await writeText(text);
}

export async function applyEventOutcome(outcome, values, writeText) {
  if (!outcome) return false;
  await writeRunClipboard(outcome.run, values, writeText);
  return true;
}

export function copyRunForPress(payload) {
  return payload && typeof payload.press === "string" ? COPY_PRESS_RUN[payload.press] : undefined;
}

// --- The tab shell ---------------------------------------------------------

export function tabTitles(form) {
  return form.tabs.map((tab) => tab.title);
}

// Which tab a title selects. A title the form does not carry selects the first
// tab rather than nothing: a renamed tab would otherwise leave the page with a
// tablist and no panel.
export function selectTab(form, title) {
  const at = form.tabs.findIndex((tab) => tab.title === title);
  return at === -1 ? 0 : at;
}

// Wiring to Tauri command.
async function invokeSettingsEvent(payload) {
  if (typeof window.__TAURI_INTERNALS__ === "undefined") {
    return { action: "nothing" };
  }
  return await window.__TAURI_INTERNALS__.invoke("settings_event", { payload });
}

// What keeps a press instead of moving the window. `label` and `textarea` are
// here because this page renders both: a checkbox row is a <label> wrapping its
// input, and a Multiline row is a <textarea> whose drag has to select text.
export const CONTROL_SELECTOR = "input, textarea, select, button, summary, pre, label";

// Alt-drag gate predicate: drag begins only when modifier is held AND target is background.
export function shouldBeginDrag(event) {
  if (!event.altKey) return false;
  const isControl = event.target.closest(CONTROL_SELECTOR);
  return !isControl;
}

// Snapshot + settings-refresh once Tauri is in the page; tab clicks still
// work without it so the shell does not sit dead in a non-Tauri load.
if (typeof document !== "undefined") {
  const tablist = document.querySelector('[role="tablist"]');
  const panel = document.querySelector('[role="tabpanel"]');

  let currentForm = null;
  let currentValues = null;
  let currentTabIndex = 0;
  let lastSnapshotPromise = null;

  function showError(message, onRetry) {
    if (!panel) return;
    panel.replaceChildren();
    const errorDiv = document.createElement("div");
    errorDiv.className = "set-error";
    errorDiv.style.cssText = "padding: 2rem; text-align: center;";

    const errorText = document.createElement("p");
    errorText.textContent = message;
    errorText.style.marginBottom = "1rem";
    errorDiv.appendChild(errorText);

    if (onRetry) {
      const retryButton = document.createElement("button");
      retryButton.textContent = "Retry";
      retryButton.type = "button";
      retryButton.addEventListener("click", onRetry);
      errorDiv.appendChild(retryButton);
    }

    panel.appendChild(errorDiv);
  }

  async function loadSnapshot() {
    const currentLoad = (async () => {
      try {
        const snapshot = await window.__TAURI__.core.invoke("settings_snapshot");
        if (lastSnapshotPromise === currentLoad) {
          currentForm = snapshot.form;
          currentValues = snapshot.view;
          renderCurrentTab();
        }
      } catch (err) {
        if (lastSnapshotPromise === currentLoad) {
          showError("Could not load settings. Check that the app is running.", () => loadSnapshot());
        }
      }
    })();
    lastSnapshotPromise = currentLoad;
    return currentLoad;
  }

  // handleEvent already invokes settings_event; a truthy outcome (refresh,
  // fill, reset, clearKey, run) means the page's snapshot is stale. Copy
  // starts writeText in this click turn: awaiting settings_event first
  // drops the user gesture WebKit requires for the clipboard.
  async function emitEvent(payload) {
    const writeText = (text) => navigator.clipboard.writeText(text);
    const hinted = copyRunForPress(payload);
    const early =
      hinted !== undefined ? writeRunClipboard(hinted, currentValues, writeText) : Promise.resolve();
    try {
      const outcome = await handleEvent(payload);
      await early;
      if (hinted !== undefined) {
        if (outcome) await loadSnapshot();
      } else if (await applyEventOutcome(outcome, currentValues, writeText)) {
        await loadSnapshot();
      }
    } catch (err) {
      showError("Could not save changes. Check your connection.", () => {
        emitEvent(payload);
      });
    }
  }

  function renderCurrentTab() {
    if (!currentForm || !currentValues || !panel) return;
    const tab = currentForm.tabs[currentTabIndex];
    if (tab) {
      render(panel, tab, currentValues, emitEvent);
    }
  }

  if (tablist && panel) {
    const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
    for (let i = 0; i < tabs.length; i++) {
      const tab = tabs[i];
      tab.addEventListener("click", () => {
        currentTabIndex = i;
        for (let j = 0; j < tabs.length; j++) {
          tabs[j].setAttribute("aria-selected", String(j === i));
        }
        panel.setAttribute("aria-label", tab.textContent);
        renderCurrentTab();
      });
    }
  }

  if (typeof window.__TAURI__ !== "undefined") {
    const { listen } = window.__TAURI__.event;
    // The handle comes from getCurrentWebviewWindow, which is what this global
    // exports and what chat.js and main.js already call. Reaching for the
    // `window` module's name instead leaves both handlers below throwing.
    const settingsWindow = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();

    // Alt-drag to move the window, gated on modifier held and target is background.
    document.addEventListener("mousedown", (event) => {
      if (!shouldBeginDrag(event)) return;
      event.preventDefault();
      settingsWindow.startDragging();
    });

    // Escape closes the window.
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        settingsWindow.close();
      }
    });

    // Enter commits the active control (blur triggers its handler).
    document.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && document.activeElement) {
        const active = document.activeElement;
        if (active.matches("input, textarea") && !active.matches('[type="checkbox"]')) {
          event.preventDefault();
          active.blur();
        }
      }
    });

    // Tauri's listen() returns a Promise<UnlistenFn>. Window destruction does
    // not guarantee cleanup of window-scoped listeners, so we unlisten on unload.
    // Evidence: Tauri v2 docs state "listeners need to be manually unlistened"
    // and the returned unlisten function exists for this reason.
    let unlistenRefresh = null;
    listen("settings-refresh", () => {
      loadSnapshot();
    }).then((unlisten) => {
      unlistenRefresh = unlisten;
    });

    window.addEventListener("beforeunload", () => {
      if (unlistenRefresh) {
        unlistenRefresh();
      }
    });

    loadSnapshot();
  }
}
