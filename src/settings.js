// One interpreter of `form::describe()`, in two halves on purpose.
//
// `controls(tab, values)` is pure: the tab as a flat list of {role, id, label,
// value, frozen} in render order — the same rows `scripts/ax-settings.swift
// dump` reads out of AppKit. That is what lets every assertion
// scripts/verify-settings-macos.sh makes against a live accessibility dump be
// made here without a window (tests/settings.test.js).
//
// `render(root, tab, values, emit)` is the DOM half. A redraw is render()
// again, reading the same description, so a control cannot be built frozen and
// drawn unfrozen.
//
// `emit` is a stub here and the `settings_event` command in #706 is what it
// becomes, so the payloads below are provisional. A secure field emits
// `set_text` rather than a verb of its own because `FormRow::SecureField`
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

// `options` empty is `describe()`'s contract, not an omission: the form leaves
// those choices to the renderer, which is how the Character popup gets the
// installed packages — a list the form cannot see. Nothing in the snapshot
// carries them yet, so the value in force is the only option. The list grows
// when `SettingsView::installed` reaches the page.
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
// state reserves no space, and a reader announces it as a disclosure. #642.
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

function labelled(row, control, extra = []) {
  const id = `set-f-${row.id}`;
  control.id = id;
  return el(
    "div",
    { class: `set-row${row.frozen ? " set-is-frozen" : ""}`, "data-row": row.id },
    row.label ? el("label", { for: id, text: row.label }) : null,
    control,
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
      input.addEventListener("blur", () => emit({ set_text: row.id, value: input.value, batched: row.batched }));
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
      select.addEventListener("change", () => emit({ set_text: row.id, value: select.value }));
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
          button.addEventListener("click", () => emit({ press: control.id }));
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

export function render(root, tab, values, emit = () => {}) {
  root.replaceChildren();
  for (const section of tab.sections) {
    const node = el("section", { class: "set-section" }, el("h2", { text: section.heading }));
    if (section.comment) node.append(el("p", { class: "set-comment", text: section.comment }));
    if (section.status) node.append(status(section.status));
    if (section.disclosure) node.append(disclosure(section.disclosure));
    for (const row of section.rows) node.append(drawRow(row, values, emit));
    root.append(node);
  }
}

// --- The tab shell ---------------------------------------------------------

export function tabTitles(form) {
  return form.tabs.map((tab) => tab.title);
}

// Which tab a title selects.
//
// A title the form does not carry selects the first tab rather than nothing:
// a renamed tab would otherwise leave the page with a tablist and no panel.
export function selectTab(form, title) {
  const at = form.tabs.findIndex((tab) => tab.title === title);
  return at === -1 ? 0 : at;
}

// The page draws no snapshot in this step: nothing feeds `form::describe()`
// across the boundary until `invoke` arrives (#706). The shell still has to
// behave, so the tabs select and the panel stays empty until a snapshot is set.
if (typeof document !== "undefined") {
  const tablist = document.querySelector('[role="tablist"]');
  const panel = document.querySelector('[role="tabpanel"]');

  if (tablist && panel) {
    for (const tab of tablist.querySelectorAll('[role="tab"]')) {
      tab.addEventListener("click", () => {
        for (const other of tablist.querySelectorAll('[role="tab"]')) {
          other.setAttribute("aria-selected", String(other === tab));
        }
        panel.setAttribute("aria-label", tab.textContent);
        panel.replaceChildren();
      });
    }
  }
}
