// Connect-landing copy and the ready gate. chat.js reaches window.__TAURI__
// as it loads and cannot be imported outside a webview; this can, so it has
// a test. One module so attached() and the composer cannot disagree about
// whether a named Harness can answer (#726).

const DISPLAY_NAMES = {
  claude: "Claude Code",
  codex: "Codex",
  "cursor-agent": "Cursor",
  grok: "Grok",
  opencode: "OpenCode",
  hermes: "Hermes",
  pi: "Pi",
};

export function harnessDisplayName(opening) {
  const key = opening?.harness_name || opening?.harness?.name;
  return DISPLAY_NAMES[key] || key || "The Harness";
}

// Configured is not ready. A named Harness whose child never came up, or
// whose launcher is missing, must not enable Ask {name} the way a live
// session does. HTTP Completer mode has no harness object.
export function canAnswer(opening) {
  if (!opening?.configured || !opening.enabled || opening.login) {
    return false;
  }
  const harness = opening.harness;
  if (!harness) {
    return true;
  }
  return harness.alive && !harness.missing;
}

export function composerPlaceholder(opening) {
  if (canAnswer(opening)) {
    return `Ask ${opening.name}…`;
  }
  return "Nothing can answer yet";
}

// Strings the landing paints. Facts come from the opening; this file owns
// the sentences so Chat and the tests cannot drift.
export function landingCopy(opening) {
  const name = harnessDisplayName(opening);
  const harness = opening?.harness;
  const missing = harness?.missing;

  if (opening?.configured && opening.enabled && opening.login) {
    return {
      title: `${name} needs login`,
      lede: `${name} needs login, or you can switch to a different Harness:`,
      command: opening.login,
      hint: "Or run this in your terminal:",
    };
  }

  if (!opening?.configured) {
    return {
      title: "Connect a Harness to get started",
      lede: "Choose an agent runtime to power this chat. Each signs in on its own — no credentials stored here.",
      command: null,
      hint: null,
    };
  }

  if (!opening.enabled) {
    return {
      title: "Chat is switched off",
      lede: "Turn AI back on in Settings, or connect a Harness below.",
      command: null,
      hint: null,
    };
  }

  if (missing) {
    return {
      title: `${name} needs \`${missing}\``,
      lede: `\`${missing}\` is not installed. ai-buddy does not bundle \`${missing}\`. Install it, then press ${name} again, or pick a different Harness below.`,
      command: null,
      hint: null,
    };
  }

  if (harness && !harness.alive) {
    return {
      title: `${name} is not running`,
      lede: `${name} is set but has not come up. Static weights answer until it does. Pick a different Harness below.`,
      command: null,
      hint: null,
    };
  }

  return {
    title: "Chat is switched off",
    lede: "Turn AI back on in Settings, or connect a Harness below.",
    command: null,
    hint: null,
  };
}
