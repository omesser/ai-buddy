// Connect-landing copy and the ready gate. chat.js reaches window.__TAURI__
// as it loads and cannot be imported outside a webview; this can, so it has
// a test. One module so attached() and the composer cannot disagree about
// whether a named Harness can answer (#726).

const DISPLAY_NAMES = {
  claude: "Claude Code",
  codex: "Codex",
  copilot: "GitHub Copilot",
  "cursor-agent": "Cursor",
  goose: "Goose",
  grok: "Grok",
  opencode: "OpenCode",
  hermes: "Hermes",
  pi: "Pi",
};

export function harnessDisplayName(opening) {
  const key = opening?.harness_name || opening?.harness?.name;
  return DISPLAY_NAMES[key] || key || "The Harness";
}

function installUrlFor(missing, harnessName) {
  if (missing === "npx") {
    return "https://nodejs.org/";
  }
  const urls = {
    hermes: "https://hermes-agent.nousresearch.com/",
    goose: "https://goose-docs.ai/docs/getting-started/installation/",
    copilot: "https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli",
    "cursor-agent": "https://www.cursor.com/",
    grok: "https://x.ai/",
    opencode: "https://opencode.ai/",
  };
  return urls[harnessName] || null;
}

// No login means no actions, even if `sign_in` is still on the opening.
// `kind` is not a button. The shell already decided which methods are.
export function loginPresentation(opening) {
  if (!opening?.configured || !opening.enabled || !opening.login) {
    return { command: null, actions: [] };
  }
  const raw = Array.isArray(opening.sign_in) ? opening.sign_in : [];
  const actions = [];
  for (const item of raw) {
    if (!item || typeof item.id !== "string" || item.id === "") {
      continue;
    }
    if (typeof item.label !== "string" || item.label === "") {
      continue;
    }
    actions.push({ id: item.id, label: item.label });
  }
  return { command: opening.login, actions };
}

// Configured is not ready. A named Harness whose child never came up, or
// whose launcher is missing, must not enable Ask {name} the way a live
// session does. HTTP Completer mode has no harness object.
// Initializing also gates: ACP handshake is in progress and turns would fail.
export function canAnswer(opening) {
  if (!opening?.configured || !opening.enabled || opening.login) {
    return false;
  }
  const harness = opening.harness;
  if (!harness) {
    return true;
  }
  return harness.alive && !harness.missing && !harness.initializing;
}

export function composerPlaceholder(opening) {
  if (canAnswer(opening)) {
    return `Ask ${opening.name}…`;
  }
  // The composer is the one surface a user types into, so a wait that ends on
  // its own says so rather than reading as a dead end (#949).
  if (opening?.harness?.initializing) {
    return `Starting ${harnessDisplayName(opening)}…`;
  }
  return "Nothing can answer yet";
}

// Strings the landing paints. Facts come from the opening; this file owns
// the sentences so Chat and the tests cannot drift.
export function landingCopy(opening) {
  const name = harnessDisplayName(opening);
  const harness = opening?.harness;
  const missing = harness?.missing;

  // Buttons are the first tier and the command the second. With no button,
  // the command is the only path, so it drops the "Or".
  if (opening?.configured && opening.enabled && opening.login) {
    const signIn = loginPresentation(opening).actions;
    const buttons = signIn.length > 0;
    return {
      title: `${name} needs login`,
      lede: `${name} needs login, or you can switch to a different Harness:`,
      command: opening.login,
      signInLabel: buttons ? "Login using:" : null,
      hint: buttons ? "Or run this in a terminal:" : "Run this in a terminal:",
      // The Harness opens the browser itself and may prefill a code there that
      // never reaches us (grok's inline flow). The click is what the user vouches for.
      signInWaiting: buttons
        ? `Finish signing in in your browser. Any code on that page came from ${name}, so continue only if you just clicked this button.`
        : null,
      signIn,
    };
  }

  if (!opening?.configured) {
    return {
      title: "Connect a Harness to get started",
      lede: "Choose an agent runtime to power this chat. Each signs in on its own — no credentials stored here.",
      command: null,
      signInLabel: null,
      hint: null,
      signIn: [],
    };
  }

  if (!opening.enabled) {
    return {
      title: "Chat is switched off",
      lede: "Turn AI back on in Settings, or connect a Harness below.",
      command: null,
      signInLabel: null,
      hint: null,
      signIn: [],
    };
  }

  if (missing) {
    const installUrl = installUrlFor(missing, harness?.name);
    const installHint = installUrl ? ` Install from ${installUrl}.` : "";
    return {
      title: `${name} needs \`${missing}\``,
      lede: `\`${missing}\` is not installed. ai-buddy does not bundle \`${missing}\`.${installHint} Then press ${name} again, or pick a different Harness below.`,
      command: null,
      signInLabel: null,
      hint: null,
      signIn: [],
    };
  }

  if (harness?.initializing) {
    return {
      title: `Initializing ${name}…`,
      lede: `${name} is starting up. Chat will be ready in a moment.`,
      command: null,
      signInLabel: null,
      hint: null,
      signIn: [],
    };
  }

  // The launcher ran and died before it answered. The Shell's sentence names
  // it, its exit status, and for `npx` the Node.js check.
  if (harness?.failed) {
    return {
      title: `${name} failed to start`,
      lede: `${harness.failed} Pick a different Harness below.`,
      command: null,
      signInLabel: null,
      hint: null,
      signIn: [],
    };
  }

  if (harness && !harness.alive) {
    return {
      title: `${name} is not running`,
      lede: `${name} is set but has not come up. Static weights answer until it does. Pick a different Harness below.`,
      command: null,
      signInLabel: null,
      hint: null,
      signIn: [],
    };
  }

  return {
    title: "Chat is switched off",
    lede: "Turn AI back on in Settings, or connect a Harness below.",
    command: null,
    signInLabel: null,
    hint: null,
    signIn: [],
  };
}
