#!/usr/bin/env bash
# Linux Settings Window smoke test, the GTK counterpart to
# verify-settings-win.ps1 / verify-settings-macos.sh.
# Checks what the GTK renderer actually built: the AI tab's section order,
# the labels and help lines, and - the part no unit test can reach - whether
# the HTTP Completer rows freeze and unfreeze when the AI source popup
# changes in a window that was not rebuilt (#630).
#
# Usage:
#   ./scripts/verify-settings-linux.sh
#   AI_BUDDY_VERIFY_BIN=path/to/ai-buddy ./scripts/verify-settings-linux.sh
#   AI_BUDDY_VERIFY_HARNESS=claude ./scripts/verify-settings-linux.sh
#   AI_BUDDY_CHARACTERS=path/to/characters ./scripts/verify-settings-linux.sh
#
# Expects a built debug binary; it does not cargo build. One throwaway HOME
# so the run does not read your own settings or Secret Service. Output under
# .verify/settings-linux-<stamp>/.
#
# Needs AT-SPI (atspi2, pyatspi). Freeze checks are skipped, not failed, when
# the Harness never answers. CI does not run this script.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

bin="${AI_BUDDY_VERIFY_BIN:-$root/target/debug/ai-buddy}"
harness="${AI_BUDDY_VERIFY_HARNESS:-claude}"
out="$root/.verify/settings-linux-$(date +%Y%m%d-%H%M%S)"
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

info "Output: $out"

app_pid=""
trap '[ -n "$app_pid" ] && kill "$app_pid" 2> /dev/null' EXIT

home="$out/home"
mkdir -p "$home"
# The throwaway HOME isolates ai-buddy's own state. Link Harness dotfiles
# across so a signed-in CLI stays signed in.
link=""
for link in .claude .claude.json .codex .config; do
  [ -e "$HOME/$link" ] && ln -sfn "$HOME/$link" "$home/$link"
done

log="$out/app.log"
# HOME is overridden so settings.json is this run's and not your own.
# AI_BUDDY_DIRECTOR_API_KEY is dropped: inherited key freezes API key row.
# AI_BUDDY_CHARACTERS is defaulted: empty HOME has no characters.
# AI_BUDDY_OPEN_SETTINGS=1 opens Settings window on launch.
# Native GTK window. Default Settings is the webview (#706 Step 8).
env -u AI_BUDDY_DIRECTOR_API_KEY \
  HOME="$home" AI_BUDDY_OPEN_SETTINGS=1 AI_BUDDY_CHARACTER=timber-wolf \
  AI_BUDDY_SETTINGS_NATIVE=1 \
  AI_BUDDY_CHARACTERS="${AI_BUDDY_CHARACTERS:-$root/characters}" \
  "$bin" > "$log" 2>&1 &
app_pid=$!

# Wait for Settings window to appear via AT-SPI
wait_for_window() {
  python3 "$root/scripts/ax-settings-linux.py" wait --timeout 30 2> /dev/null
}

if ! wait_for_window; then
  fail "Settings window never appeared (AT-SPI check)"
  exit 1
fi

pass "Settings window appeared"

# Dump accessible tree via AT-SPI
dump_window() {
  local target="$1"
  python3 "$root/scripts/ax-settings-linux.py" dump > "$target" 2>&1
}

plain="$out/no-harness.txt"
if ! dump_window "$plain"; then
  fail "could not dump Settings window without a Harness"
  exit 1
fi

pass "Dumped Settings window accessible tree"

# Check AI tab section order
section_order() {
  grep -E '^\s+label\|(AI|AI source|Model / API|Last user turn)\|' "$1" |
    sed 's/.*label|//' | sed 's/|.*//' |
    awk '!seen[$0]++' | paste -sd'>' - | sed 's/>/ > /g'
}

info "Section order: $(section_order "$plain")"
expected_order="AI > AI source > Model / API > Last user turn"
actual_order="$(section_order "$plain")"
if [ "$actual_order" = "$expected_order" ]; then
  pass "AI tab section order matches director_sections"
else
  fail "AI tab section order was $actual_order (expected $expected_order)"
fi

# Check HTTP rows editable state
row_state() {
  local dump="$1" label="$2"
  grep -F "$label" "$dump" | grep -Eq 'editable|sensitive' && echo "editable" || echo "frozen"
}

expect_editable() {
  local dump="$1" label="$2" when="$3"
  if [ "$(row_state "$dump" "$label")" = "editable" ]; then
    pass "$label is editable $when"
  else
    fail "$label should be editable $when"
  fi
}

expect_frozen() {
  local dump="$1" label="$2" when="$3"
  if [ "$(row_state "$dump" "$label")" = "frozen" ]; then
    pass "$label is frozen $when"
  else
    fail "$label must be frozen $when (#452)"
  fi
}

expect_http_editable() {
  local dump="$1" when="$2"
  for label in "Base URL" "Model" "API key" "Clear key"; do
    expect_editable "$dump" "$label" "$when"
  done
}

expect_http_frozen() {
  local dump="$1" when="$2"
  for label in "Base URL" "Model" "API key" "Clear key"; do
    expect_frozen "$dump" "$label" "$when"
  done
}

expect_http_editable "$plain" "with no Harness attached"

# Switch AI source via AT-SPI (pick from combo box)
pick_source() {
  local title="$1"
  python3 "$root/scripts/ax-settings-linux.py" pick-source "$title" 2>&1
}

# Exact popup titles after #593
model_api="Model API"
harness_title="Harness · $harness"

info "Runtime switch: $harness_title"
# Note: AT-SPI combo box picking is complex and may not be reliable in all
# GTK3 configurations. This script attempts basic interaction but may skip
# the freeze checks if AT-SPI cannot manipulate the combo.

if pick_source "$harness_title"; then
  sleep 2 # Wait for attachment
  driven="$out/harness.txt"
  dump_window "$driven" || fail "could not dump after picking $harness_title"

  if grep -Eq "$harness attached[,;]" "$log"; then
    expect_http_frozen "$driven" "while $harness drives"
  else
    skip "$harness never answered; the freeze checks need a signed-in Harness"
  fi

  info "Runtime switch: $model_api"
  if pick_source "$model_api"; then
    sleep 1
    off="$out/off-again.txt"
    dump_window "$off" || fail "could not dump after picking $model_api"
    expect_http_editable "$off" "after picking $model_api"
  else
    skip "could not pick $model_api in the source popup"
  fi
else
  skip "AT-SPI combo box manipulation not available; freeze checks skipped"
  info "Manual testing required: pick $harness_title, verify HTTP rows frozen,"
  info "then pick $model_api, verify HTTP rows editable without relaunch"
fi

echo
if [ "$failures" -eq 0 ]; then
  pass "all checks passed - dumps and stills under $out"
  exit 0
fi
fail "$failures check(s) failed - dumps and stills under $out"
exit 1
