const invoke = window.__TAURI__.core.invoke;

function $(id) {
  return document.getElementById(id);
}

function fillSelect(select, names, current) {
  select.replaceChildren();
  for (const name of names) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    option.selected = name === current;
    select.append(option);
  }
}

async function load() {
  const state = await invoke("settings_state");
  $("director_enabled").checked = state.director_enabled;
  $("ambient_wakes").checked = state.ambient_wakes;
  $("do_not_disturb").checked = state.do_not_disturb;
  $("hidden").checked = state.hidden;
  $("hide_in_fullscreen").checked = state.hide_in_fullscreen;
  $("hide_hotkey").value = state.hide_hotkey;
  $("launch_at_login").checked = state.launch_at_login;
  $("excluded").value = state.excluded_applications.join("\n");
  $("memory_path").textContent = state.memory_path;
  $("payload").textContent = state.last_payload || "Nothing sent yet.";

  fillSelect($("character"), state.installed, state.character);
  fillSelect($("new_character"), state.installed, state.character);

  const list = $("instances");
  list.replaceChildren();
  for (const instance of state.instances) {
    const row = document.createElement("li");
    const name = document.createElement("span");
    name.textContent = `${instance.name} (${instance.character})`;
    const dismiss = document.createElement("button");
    dismiss.type = "button";
    dismiss.textContent = "Dismiss";
    dismiss.addEventListener("click", async () => {
      await invoke("instance_dismiss", { id: instance.id });
      await load();
    });
    row.append(name, dismiss);
    list.append(row);
  }
}

function bindToggle(id, key) {
  $(id).addEventListener("change", async (event) => {
    await invoke("settings_patch", { patch: { [key]: event.target.checked } });
    await load();
  });
}

bindToggle("director_enabled", "director_enabled");
bindToggle("ambient_wakes", "ambient_wakes");
bindToggle("do_not_disturb", "do_not_disturb");
bindToggle("hidden", "hidden");
bindToggle("hide_in_fullscreen", "hide_in_fullscreen");
bindToggle("launch_at_login", "launch_at_login");

$("hide_hotkey").addEventListener("change", async (event) => {
  await invoke("settings_patch", { patch: { hide_hotkey: event.target.value } });
  await load();
});

$("character").addEventListener("change", async (event) => {
  await invoke("settings_patch", { patch: { character: event.target.value } });
  await load();
});

$("excluded").addEventListener("change", async (event) => {
  const apps = event.target.value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  await invoke("settings_patch", { patch: { excluded_applications: apps } });
});

$("memory_open").addEventListener("click", () => invoke("memory_open"));
$("memory_wipe").addEventListener("click", async () => {
  if (window.confirm("Wipe Memory? A backup is kept beside the file.")) {
    await invoke("memory_wipe");
    await load();
  }
});

$("spawn").addEventListener("click", async () => {
  const name = $("new_name").value.trim();
  const character = $("new_character").value;
  await invoke("instance_spawn", { character, name });
  $("new_name").value = "";
  await load();
});

load().catch((err) => {
  console.error("settings", err);
  $("payload").textContent = String(err);
});

setInterval(() => {
  invoke("director_payload")
    .then((inspect) => {
      $("payload").textContent = inspect.last_payload || "Nothing sent yet.";
    })
    .catch(() => {});
}, 2000);
