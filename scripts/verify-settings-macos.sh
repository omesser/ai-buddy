#!/usr/bin/env bash
# macOS Settings Window smoke test, the AppKit counterpart to
# verify-settings-win.ps1.
#
# Checks what the AppKit renderer actually built: the AI tab's section order,
# the labels and help lines, and - the part no unit test can reach - whether
# the HTTP Completer rows freeze and unfreeze when the AI source popup
# changes in a window that was not rebuilt (#629).
#
# Usage:
#   ./scripts/verify-settings-macos.sh
#   AI_BUDDY_VERIFY_BIN=path/to/ai-buddy ./scripts/verify-settings-macos.sh
#   AI_BUDDY_VERIFY_HARNESS=grok ./scripts/verify-settings-macos.sh
#   AI_BUDDY_CHARACTERS=path/to/characters ./scripts/verify-settings-macos.sh
#
# Expects a built debug binary; it does not cargo build. One throwaway HOME
# so the run does not read your own settings or Keychain. Output under
# .verify/settings-macos-<stamp>/.
#
# That throwaway HOME has no keychain to find, so every dump carries
# "Unavailable: Platform secure storage failure" where a stored key would be.
# It is the price of the isolation, not a defect in the window: the row is
# still editable, which is all these checks read.
#
# Needs an Accessibility grant for the terminal running it - see
# scripts/ax-settings.swift. The Harness switch needs that Harness installed
# and signed in; those freeze checks are skipped, not failed, when it never
# answers. CI does not run this script.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

bin="${AI_BUDDY_VERIFY_BIN:-$root/target/debug/ai-buddy}"
harness="${AI_BUDDY_VERIFY_HARNESS:-claude}"
out="$root/.verify/settings-macos-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"

failures=0
info() { printf '\033[36m[INFO]\033[0m %s\n' "$*"; }
pass() { printf '\033[32m[PASS]\033[0m %s\n' "$*"; }
skip() { printf '\033[33m[SKIP]\033[0m %s\n' "$*"; }
fail() {
  printf '\033[31m[FAIL]\033[0m %s\n' "$*"
  failures=$((failures + 1))
}

[ -x "$bin" ] || {
  fail "no binary at $bin - cargo build first, or set AI_BUDDY_VERIFY_BIN"
  exit 1
}

# The Swift helper is compiled once per run into the output directory. Keeping
# it out of the repository means no build step to forget and no stale binary to
# debug when the source moves ahead of it.
ax="$out/ax-settings"
info "Output: $out"
swiftc -O "$root/scripts/ax-settings.swift" -o "$ax" || {
  fail "could not compile scripts/ax-settings.swift"
  exit 1
}

app_pid=""
# Inline rather than a cleanup function: shellcheck cannot see that a trap
# calls one, and the warning it raises for that has a different number on the
# version Ubuntu CI pins than on the one Homebrew ships.
trap '[ -n "$app_pid" ] && kill "$app_pid" 2> /dev/null' EXIT

home="$out/home"
mkdir -p "$home"
# The throwaway HOME isolates ai-buddy's own state, which lives under
# Library/Application Support - but a Harness CLI keeps its onboarding in
# a dotfile, and one that cannot find it never finishes starting and the
# freeze pass silently degrades to "set but not running". Link the
# dotfiles across; add a line here for a Harness that keeps its own
# somewhere else.
link=""
for link in .claude .claude.json .codex .config; do
  [ -e "$HOME/$link" ] && ln -sfn "$HOME/$link" "$home/$link"
done

log="$out/app.log"
# HOME is overridden so settings.json and the Action Log are this run's and
# not your own, while the Harness keeps its Keychain credentials, which is
# where a signed-in CLI actually stores them.
# No AI_BUDDY_HARNESS: the source switch has to happen in this window.
#
# AI_BUDDY_DIRECTOR_API_KEY is dropped rather than inherited. direnv exports
# it in this repository, and a key that arrives from the environment freezes
# the API key row on purpose (#272) - so an inherited one adds failures about
# rows that are doing exactly what they were built to do, and the dump says
# "Overridden by env" in a line nobody reads.
#
# AI_BUDDY_CHARACTERS is defaulted because package::search_paths() looks in
# HOME and in the Tauri resource directory, and a bare binary has neither: the
# throwaway HOME is empty and there is no bundle around the executable. No
# Character loads, the app exits before it draws a status item, and the script
# reports "no status item after 20s", which reads as a launch that hung rather
# than one that had nothing to draw. Pointing at the repository's own
# characters/ is what a bundle would have supplied. An explicit value still
# wins, for pointing a run at a Character somewhere else.
env -u AI_BUDDY_DIRECTOR_API_KEY \
  HOME="$home" AI_BUDDY_CAPTURABLE=1 AI_BUDDY_CHARACTER=timber-wolf \
  AI_BUDDY_CHARACTERS="${AI_BUDDY_CHARACTERS:-$root/characters}" \
  "$bin" > "$log" 2>&1 &
app_pid=$!

still() {
  local name="$1"
  local rect
  rect=$("$ax" frame "$app_pid" 2> /dev/null) || return 0
  [ -n "$rect" ] && screencapture -x -o -R "$rect" "$out/${name}.png" 2> /dev/null
}

dump_window() {
  local target="$1"
  "$ax" dump "$app_pid" > "$target" || return 1
  still "$(basename "$target" .txt)"
}

# Whether the control for a row is live. A row is addressed by the text you can
# read on it, and this window draws two shapes of row. Most are a label and the
# control beside it: the tree is in render order, so the control for "Base URL"
# is the line straight after the static text that says it, and that text sits
# in the value column, because value is where an AXStaticText keeps what it
# displays. A button is the other shape - it carries its own text in the title
# column and has no label line above it at all, so the answer for "Clear key"
# has to come off the very line that matched rather than off the one after it.
# Matching the value column alone left two assertions that could not pass
# whatever the window did (#732).
#
# Only a role that can be live answers for its own title. The toolbar is in the
# same tree as the form and its tabs are titled too, so an unqualified title
# match would let a tab shadow a row that happens to share a word with it -
# first line in render order wins, and the toolbar comes first.
#
# The role is not fixed, which is the trap. AppKit demotes a non-editable
# NSTextField from AXTextField to plain AXStaticText, so a frozen row and its
# label look alike and a matcher that waits for AXTextField walks straight past
# it into the next row. Take the next line whatever its role, and read the
# column that means "live" for that role: settability for a field, enabled for
# a popup or a button, and nothing at all for the demoted static text, which is
# frozen by definition. AXEnabled on a text field stays true either way.
#
# scripts/test_verify_settings_row_live.sh pins this against recorded dumps,
# including a button that is genuinely disabled: a looser matcher would call
# that one live too.
row_live() {
  awk -F'|' -v want="$2" '
    function live() {
      if ($1 ~ /AXTextField/) { return $6 }
      if ($1 ~ /AXPopUpButton|AXButton/) { return $5 }
      return "false"
    }
    found { print live(); exit }
    $1 ~ /AXPopUpButton|AXButton/ && $2 == want { print live(); exit }
    $3 == want { found = 1 }
  ' "$1"
}

# The endpoint picker is the popup that sits above the Base URL field in tree
# order, under the Model / API heading.
picker_live() {
  awk -F'|' '
    $1 ~ /AXPopUpButton/ { lastpop = $5 }
    $3 == "Base URL" { print lastpop; exit }
  ' "$1"
}

has_line() { grep -Fq "$2" "$1"; }

section_order() {
  # Deduplicated because "AI source" is both a heading and the label of the
  # row under it, and only the first is a section.
  grep -E '^AXStaticText\|\|(AI|AI source|Model / API|Last user turn)\|' "$1" |
    cut -d'|' -f3 | awk '!seen[$0]++' | paste -sd'>' - | sed 's/>/ > /g'
}

expect_live() {
  local dump="$1" label="$2" when="$3"
  if [ "$(row_live "$dump" "$label")" = "true" ]; then
    pass "$label is live $when"
  else
    fail "$label should be editable $when"
  fi
}

expect_frozen() {
  local dump="$1" label="$2" when="$3"
  if [ "$(row_live "$dump" "$label")" = "false" ]; then
    pass "$label is frozen $when"
  else
    fail "$label must be frozen $when (#452)"
  fi
}

expect_picker() {
  local dump="$1" want="$2" when="$3"
  if [ "$(picker_live "$dump")" = "$want" ]; then
    if [ "$want" = true ]; then
      pass "the endpoint picker is live $when"
    else
      pass "the endpoint picker is frozen $when"
    fi
  elif [ "$want" = true ]; then
    fail "the endpoint picker should be enabled $when"
  else
    fail "the endpoint picker must be frozen $when"
  fi
}

expect_http_live() {
  local dump="$1" when="$2"
  local label=""
  for label in "Base URL" "Model" "API key" "Clear key"; do
    expect_live "$dump" "$label" "$when"
  done
  expect_picker "$dump" true "$when"
}

expect_http_frozen() {
  local dump="$1" when="$2"
  local label=""
  for label in "Base URL" "Model" "API key" "Clear key"; do
    expect_frozen "$dump" "$label" "$when"
  done
  expect_picker "$dump" false "$when"
}

count_attached() {
  grep -c "harness: $harness attached" "$log" 2> /dev/null || true
}

wait_attached() {
  local need="$1"
  local waited=0
  while [ "$(count_attached)" -lt "$need" ] && [ "$waited" -lt 90 ]; do
    sleep 1
    waited=$((waited + 1))
  done
  [ "$(count_attached)" -ge "$need" ]
}

# Exact popup titles after #593. pick matches AXTitle, not a substring.
model_api="Model API"
harness_title="Harness · $harness"

"$ax" open "$app_pid" || {
  fail "could not open Settings from the tray"
  exit 1
}

info "Walking tabs"
tab=""
for tab in Presence Character AI Privacy Development; do
  if "$ax" tab "$app_pid" "$tab" && dump_window "$out/tab-${tab}.txt"; then
    pass "tab $tab dumped"
  else
    fail "could not dump the $tab tab"
  fi
done

"$ax" tab "$app_pid" AI || {
  fail "could not return to the AI tab"
  exit 1
}

plain="$out/no-harness.txt"
dump_window "$plain" || {
  fail "could not read the Settings window without a Harness"
  exit 1
}

info "Section order: $(section_order "$plain")"
if [ "$(section_order "$plain")" = "AI > AI source > Model / API > Last user turn" ]; then
  pass "AI tab section order matches director_sections"
else
  fail "AI tab section order was $(section_order "$plain")"
fi

expect_http_live "$plain" "with no Harness attached"

if has_line "$plain" "The HTTP endpoint below is the AI brain."; then
  pass 'the state line says "AI brain"'
else
  fail 'the state line no longer says "AI brain"'
fi

if has_line "$plain" "is the Director's mind"; then
  fail '"mind" is back in the state line'
else
  pass '"mind" is gone from the state line'
fi

# Window is hidden, not rebuilt. A second open is the same controller (#629).
info "Close and reopen"
"$ax" open "$app_pid" || fail "could not reopen Settings via the tray"
dump_window "$out/reopened.txt" || fail "could not dump Settings after reopen"

info "Runtime switch: $harness_title"
# A pick that does not take is the bug this script exists to catch (#634),
# not an environment it cannot run in - `form::harness_options` is static, so
# every option is in the popup on every machine. The skips further down are
# the real environment ones: they fire after the pick took, for a Harness that
# is installed but not signed in.
if ! "$ax" pick "$app_pid" "AI source" "$harness_title"; then
  fail "could not pick $harness_title in the source popup"
else
  wait_attached 1 || true
  driven="$out/harness.txt"
  dump_window "$driven" || fail "could not dump after picking $harness_title"

  if grep -Eq "$harness attached[,;]" "$driven"; then
    expect_http_frozen "$driven" "while $harness drives"
  else
    # Set but never answering is a real state, and freezing on it would leave
    # no reachable Completer at all - so it is not a failure here, just not
    # the state these three checks are about (#452).
    skip "$harness never answered; the freeze checks need a signed-in Harness"
    grep -F "$harness" "$driven" | head -3
  fi

  info "Runtime switch: $model_api"
  if ! "$ax" pick "$app_pid" "AI source" "$model_api"; then
    fail "could not pick $model_api in the source popup"
  else
    waited=0
    off="$out/off-again.txt"
    dump_window "$off" || fail "could not dump after picking $model_api"
    while [ "$(row_live "$off" "Base URL")" != "true" ] && [ "$waited" -lt 30 ]; do
      sleep 1
      waited=$((waited + 1))
      dump_window "$off" || break
    done
    expect_http_live "$off" "after picking $model_api"
  fi

  info "Runtime switch: $harness_title again"
  if ! "$ax" pick "$app_pid" "AI source" "$harness_title"; then
    fail "could not pick $harness_title a second time"
  else
    wait_attached 2 || true
    again="$out/harness-again.txt"
    dump_window "$again" || fail "could not dump after picking $harness_title again"
    if grep -Eq "$harness attached[,;]" "$again"; then
      expect_http_frozen "$again" "after picking $harness_title again"
    else
      skip "$harness did not answer the second pick"
    fi
  fi
fi

echo
if [ "$failures" -eq 0 ]; then
  pass "all checks passed - dumps and stills under $out"
  exit 0
fi
fail "$failures check(s) failed - dumps and stills under $out"
exit 1
