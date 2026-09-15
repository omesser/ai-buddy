// Deliberate misreads. Every line below should be a tsc error.
const { listen } = window.__TAURI__.event;
const { invoke } = window.__TAURI__.core;
listen("chat-opening", ({ payload }) => {
  console.log(payload.harness.missing);   // ChatHarness has no `missing` (#659's gap)
  console.log(payload.harnes_name);       // typo
});
listen("chat", ({ payload }) => console.log(payload.reactingTo)); // wrong case
listen("chat-permision", () => {});       // unknown event name
invoke("select_harness", { harnes: "codex" }); // wrong arg name
invoke("select_harness", { harness: "codex" }).then((login) => login.trim()); // ok: string
invoke("chat_ready", { instance: "a" }).then((x) => x.length); // void has no length
