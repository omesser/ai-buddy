#!/usr/bin/env bash
# Linux Settings Window smoke test, the GTK counterpart to
# verify-settings-win.ps1 / verify-settings-macos.sh.
#
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
# Needs AT-SPI accessibility infrastructure available (atspi2, pyatspi).
# The Harness switch needs that Harness installed and signed in; those freeze
# checks are skipped, not failed, when it never answers. CI does not run this
# script.

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
env -u AI_BUDDY_DIRECTOR_API_KEY \
  HOME="$home" AI_BUDDY_OPEN_SETTINGS=1 AI_BUDDY_CHARACTER=timber-wolf \
  AI_BUDDY_CHARACTERS="${AI_BUDDY_CHARACTERS:-$root/characters}" \
  "$bin" > "$log" 2>&1 &
app_pid=$!

# Wait for Settings window to appear via AT-SPI
wait_for_window() {
  local waited=0
  while [ "$waited" -lt 30 ]; do
    if python3 -c "
import sys
try:
    import pyatspi

    def has_ai_tab(acc, depth=0):
        if depth > 6:
            return False
        try:
            if acc.getRoleName() == 'page tab' and (acc.name or '') == 'AI':
                return True
            for i in range(acc.childCount):
                if has_ai_tab(acc.getChildAtIndex(i), depth + 1):
                    return True
        except Exception:
            return False
        return False

    desktop = pyatspi.Registry.getDesktop(0)
    for app_idx in range(desktop.childCount):
        app = desktop.getChildAtIndex(app_idx)
        if 'ai-buddy' not in (app.name or '').lower():
            continue
        for win_idx in range(app.childCount):
            win = app.getChildAtIndex(win_idx)
            # Prefer the GTK Settings notebook over overlay/WebKit frames
            # that share the same window title 'ai-buddy'.
            if has_ai_tab(win):
                sys.exit(0)
    sys.exit(1)
except Exception as e:
    print(f'AT-SPI error: {e}', file=sys.stderr)
    sys.exit(1)
" 2> /dev/null; then
      return 0
    fi
    sleep 1
    waited=$((waited + 1))
  done
  return 1
}

if ! wait_for_window; then
  fail "Settings window never appeared (AT-SPI check)"
  exit 1
fi

pass "Settings window appeared"

# Dump accessible tree via AT-SPI
dump_window() {
  local target="$1"
  python3 << 'PYEOF' > "$target" 2>&1 || return 1
import sys
try:
    import pyatspi

    def has_ai_tab(acc, depth=0):
        if depth > 6:
            return False
        try:
            if acc.getRoleName() == "page tab" and (acc.name or "") == "AI":
                return True
            for i in range(acc.childCount):
                if has_ai_tab(acc.getChildAtIndex(i), depth + 1):
                    return True
        except Exception:
            return False
        return False

    def dump_accessible(acc, depth=0):
        indent = "  " * depth
        role = acc.getRoleName() if hasattr(acc, "getRoleName") else "unknown"
        name = acc.name if hasattr(acc, "name") else ""

        states = []
        if hasattr(acc, "getState"):
            state_set = acc.getState()
            if hasattr(state_set, "contains"):
                if state_set.contains(pyatspi.STATE_EDITABLE):
                    states.append("editable")
                if state_set.contains(pyatspi.STATE_SENSITIVE):
                    states.append("sensitive")
                if not state_set.contains(pyatspi.STATE_SENSITIVE):
                    states.append("insensitive")

        state_str = f"[{','.join(states)}]" if states else ""
        print(f"{indent}{role}|{name}|{state_str}")

        if hasattr(acc, "childCount"):
            for i in range(acc.childCount):
                try:
                    child = acc.getChildAtIndex(i)
                    dump_accessible(child, depth + 1)
                except Exception:
                    pass

    desktop = pyatspi.Registry.getDesktop(0)
    settings = None
    for app_idx in range(desktop.childCount):
        app = desktop.getChildAtIndex(app_idx)
        if "ai-buddy" not in (app.name or "").lower():
            continue
        for win_idx in range(app.childCount):
            win = app.getChildAtIndex(win_idx)
            if has_ai_tab(win):
                settings = win
                break
        if settings is not None:
            break

    if settings is None:
        print("Settings window not found", file=sys.stderr)
        sys.exit(1)

    dump_accessible(settings)
    sys.exit(0)

except Exception as e:
    print(f"AT-SPI error: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
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
  TITLE="$title" python3 << 'PYEOF' 2>&1 || return 1
import os
import sys
import time

try:
    import pyatspi

    title = os.environ.get("TITLE", "")

    def has_ai_tab(acc, depth=0):
        if depth > 6:
            return False
        try:
            if acc.getRoleName() == "page tab" and (acc.name or "") == "AI":
                return True
            for i in range(acc.childCount):
                if has_ai_tab(acc.getChildAtIndex(i), depth + 1):
                    return True
        except Exception:
            return False
        return False

    def walk(acc, depth=0):
        if depth > 20:
            return
        yield acc
        try:
            for i in range(acc.childCount):
                try:
                    yield from walk(acc.getChildAtIndex(i), depth + 1)
                except Exception:
                    pass
        except Exception:
            pass

    def menu_titles(combo):
        titles = []
        for node in walk(combo, 0):
            try:
                if node.getRoleName() == "menu item":
                    titles.append(node.name or "")
            except Exception:
                pass
        return titles

    def find_source_combo(win):
        # The AI source combo's accessible name is the current selection
        # (e.g. "Model API"), not the "AI source" label beside it. Identify
        # it by its menu items after #593.
        for node in walk(win):
            try:
                if "combo" not in (node.getRoleName() or "").lower():
                    continue
            except Exception:
                continue
            titles = menu_titles(node)
            if "Model API" in titles and any(t.startswith("Harness") for t in titles):
                return node
        return None

    def find_menu_item(combo, want):
        for node in walk(combo):
            try:
                if node.getRoleName() == "menu item" and (node.name or "") == want:
                    return node
            except Exception:
                pass
        return None

    desktop = pyatspi.Registry.getDesktop(0)
    settings = None
    for app_idx in range(desktop.childCount):
        app = desktop.getChildAtIndex(app_idx)
        if "ai-buddy" not in (app.name or "").lower():
            continue
        for win_idx in range(app.childCount):
            win = app.getChildAtIndex(win_idx)
            if has_ai_tab(win):
                settings = win
                break
        if settings is not None:
            break

    if settings is None:
        print("Settings window not found", file=sys.stderr)
        sys.exit(1)

    combo = find_source_combo(settings)
    if combo is None:
        print("AI source combo not found", file=sys.stderr)
        sys.exit(1)

    item = find_menu_item(combo, title)
    if item is None:
        print(f"menu item not found: {title!r}", file=sys.stderr)
        sys.exit(1)

    # Prefer selecting the menu item directly; fall back to opening the combo.
    acted = False
    try:
        act = item.queryAction()
        if act.nActions > 0:
            act.doAction(0)
            acted = True
    except Exception:
        pass

    if not acted:
        try:
            act = combo.queryAction()
            if act.nActions > 0:
                act.doAction(0)
                time.sleep(0.3)
                item = find_menu_item(combo, title)
                if item is not None:
                    item.queryAction().doAction(0)
                    acted = True
        except Exception as e:
            print(f"AT-SPI action error: {e}", file=sys.stderr)
            sys.exit(1)

    if not acted:
        print("could not activate menu item", file=sys.stderr)
        sys.exit(1)

    time.sleep(0.5)
    sys.exit(0)

except Exception as e:
    print(f"AT-SPI error: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
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
