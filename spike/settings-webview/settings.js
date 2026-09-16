// One interpreter of the form description. Two halves, split on purpose:
//
// `controls(tab, view)` is pure: the tab as a flat list of {role, id, label,
// value, frozen} — the same rows `scripts/ax-settings.swift dump` reads out of
// AppKit today, so the assertions verify-settings-macos.sh makes against an AX
// dump become `node --test` assertions against this list (see settings.test.js).
//
// `render(root, tab, view)` turns the same description into DOM. Nothing here
// reads the description twice: a redraw is render() again, which is why the
// #625 class of bug (frozen applied at build, never at draw) has no place to live.

export function controls(tab, view) {
  const out = [];
  for (const section of tab.sections) {
    out.push({ role: "heading", id: null, label: section.heading, value: null, frozen: false });
    for (const row of section.rows) {
      switch (row.type) {
        case "Checkbox":
          out.push({ role: "checkbox", id: row.id, label: row.label, value: Boolean(view[row.id]), frozen: row.frozen });
          break;
        case "TextField":
          out.push({ role: "textfield", id: row.id, label: row.label, value: view[row.id] ?? "", frozen: row.frozen });
          break;
        case "SecureField":
          out.push({ role: "securefield", id: row.id, label: row.label, value: "", frozen: row.frozen });
          break;
        case "Popup":
          out.push({ role: "popup", id: row.id, label: row.label, value: view[row.id] ?? row.options[0], frozen: row.frozen });
          break;
        case "InspectBlock":
          out.push({ role: "statictext", id: row.id, label: row.label, value: view[row.id] ?? "", frozen: true });
          break;
        case "Composite":
          for (const c of row.controls) {
            const role = c.type === "Button" ? "button" : c.type === "Popup" ? "popup" : "textfield";
            out.push({ role, id: c.id, label: c.label ?? null, value: view[c.id] ?? null, frozen: c.frozen });
          }
          break;
      }
    }
  }
  return out;
}

// --- DOM -------------------------------------------------------------------

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "text") node.textContent = v;
    else if (v === true) node.setAttribute(k, "");
    else node.setAttribute(k, v);
  }
  node.append(...children.filter((c) => c !== null && c !== undefined));
  return node;
}

// "What is this?" is a <details>: the browser owns open/closed, the closed
// state reserves no space, and a reader announces it as a disclosure. #642.
function disclosure(text) {
  return text
    ? el("details", { class: "disclosure" }, el("summary", { text: "What is this?" }), el("p", { text }))
    : null;
}

function help(text) {
  return text ? el("p", { class: "help", text }) : null;
}

function status(text) {
  return text ? el("p", { class: "status", text }) : null;
}

function labelled(row, control, extra = []) {
  const id = `f-${row.id}`;
  control.id = id;
  return el(
    "div",
    { class: `row${row.frozen ? " is-frozen" : ""}`, "data-row": row.id },
    row.label ? el("label", { for: id, text: row.label }) : null,
    control,
    ...extra,
  );
}

function popup(id, options, value, frozen) {
  const select = el("select", { disabled: frozen });
  for (const o of options) select.append(el("option", { value: o, selected: o === value, text: o }));
  select.dataset.id = id;
  return select;
}

function row(r, view, emit) {
  switch (r.type) {
    case "Checkbox": {
      const input = el("input", { type: "checkbox", disabled: r.frozen });
      input.checked = Boolean(view[r.id]);
      input.addEventListener("change", () => emit({ set_bool: r.id, value: input.checked }));
      const box = el("div", { class: `row check${r.frozen ? " is-frozen" : ""}`, "data-row": r.id },
        el("label", {}, input, el("span", { text: r.label })),
        help(r.help), status(r.status), disclosure(r.disclosure));
      return box;
    }
    case "TextField": {
      const input = el("input", { type: "text", placeholder: r.placeholder, readonly: r.frozen, "aria-readonly": r.frozen ? "true" : null });
      input.value = view[r.id] ?? "";
      input.addEventListener("blur", () => emit({ set_text: r.id, value: input.value, batched: r.batched }));
      return labelled(r, input, [help(r.help), status(r.status), disclosure(r.disclosure)]);
    }
    case "SecureField": {
      const input = el("input", { type: "password", placeholder: view.api_key_placeholder ?? "", readonly: r.frozen, autocomplete: "off" });
      return labelled(r, input, [status(r.status)]);
    }
    case "Popup": {
      const select = popup(r.id, r.options, view[r.id], r.frozen);
      select.addEventListener("change", () => emit({ set_text: r.id, value: select.value }));
      return labelled(r, select, [help(r.help), status(r.status), disclosure(r.disclosure)]);
    }
    case "InspectBlock": {
      const pre = el("pre", { class: "inspect", tabindex: "0", text: view[r.id] ?? "" });
      return el("div", { class: "row", "data-row": r.id }, r.label ? el("span", { class: "label", text: r.label }) : null, pre, help(r.help), disclosure(r.disclosure));
    }
    case "Composite": {
      const line = el("div", { class: "controls" });
      for (const c of r.controls) {
        if (c.type === "Button") {
          const b = el("button", { type: "button", disabled: c.frozen, text: c.label, "data-id": c.id });
          b.addEventListener("click", () => emit({ press: c.id }));
          line.append(b);
        } else if (c.type === "Popup") {
          const s = popup(c.id, c.options, view[c.id], c.frozen);
          s.addEventListener("change", () => emit({ pick: c.id, value: s.value, fills: c.fills?.row ?? null }));
          line.append(s);
        }
      }
      return el("div", { class: "row", "data-row": r.id }, line, help(r.help), disclosure(r.disclosure));
    }
    default:
      return el("div", { class: "row unknown", text: `unrendered row type ${r.type}` });
  }
}

export function render(root, tab, view, emit = () => {}) {
  root.replaceChildren();
  for (const s of tab.sections) {
    const section = el("section", {}, el("h2", { text: s.heading }));
    if (s.comment) section.append(el("p", { class: "comment", text: s.comment }));
    if (s.status) section.append(status(s.status));
    if (s.disclosure) section.append(disclosure(s.disclosure));
    for (const r of s.rows) section.append(row(r, view, emit));
    root.append(section);
  }
}
