export function mountChatAppearance(root, systemPreference) {
  const media =
    systemPreference ?? window.matchMedia("(prefers-color-scheme: light)");
  let selected = "system";
  let observed = Boolean(media.matches);

  function palette() {
    if (selected === "light") {
      return "light";
    }
    if (selected === "dark") {
      return "dark";
    }
    return observed ? "light" : "dark";
  }

  function commit() {
    root.dataset.chatPalette = palette();
  }

  function onMediaChange(event) {
    observed = Boolean(event && typeof event.matches === "boolean" ? event.matches : media.matches);
    commit();
  }

  if (typeof media.addEventListener === "function") {
    media.addEventListener("change", onMediaChange);
  }

  commit();

  return function apply(wire) {
    selected = wire === "light" || wire === "dark" ? wire : "system";
    commit();
  };
}
