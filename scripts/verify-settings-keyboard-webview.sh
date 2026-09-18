#!/usr/bin/env bash
# Keyboard-only sitting for the Settings webview (#848, #706 artefact 2).
# Usage:
#   ./scripts/verify-settings-keyboard-webview.sh
#   AI_BUDDY_VERIFY_BIN=path/to/ai-buddy ./scripts/verify-settings-keyboard-webview.sh
#
# Forces AI_BUDDY_SETTINGS_WEBVIEW=1. Overlay may be up. After the tray
# open, every control is reached with Tab / Space / Enter / Escape, not AXPress.
# Needs Accessibility (scripts/ax-settings.swift). CI does not run this.
#
# Stills land under .verify/; downscale before attaching. Do not commit PNGs.
# Shares /tmp/ai-buddy-settings-overlay.lock with the #849 sitting.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

bin="${AI_BUDDY_VERIFY_BIN:-$root/target/debug/ai-buddy}"
out="$root/.verify/settings-keyboard-webview-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"

failures=0
lock_held=0
app_pid=""

info() { printf '\033[36m[INFO]\033[0m %s\n' "$*"; }
pass() { printf '\033[32m[PASS]\033[0m %s\n' "$*"; }
fail() {
  printf '\033[31m[FAIL]\033[0m %s\n' "$*"
  failures=$((failures + 1))
}

# Inline: shellcheck cannot see that trap calls a function (same as
# verify-settings-macos.sh).
trap '[ -n "$app_pid" ] && kill "$app_pid" 2> /dev/null; [ "${lock_held:-0}" -eq 1 ] && rmdir /tmp/ai-buddy-settings-overlay.lock 2> /dev/null' EXIT

[ -x "$bin" ] || {
  fail "no binary at $bin - cargo build -p ai-buddy first, or set AI_BUDDY_VERIFY_BIN"
  exit 1
}

if [ "${AI_BUDDY_OVERLAY_LOCK:-1}" = 1 ]; then
  deadline=$((SECONDS + 1200))
  while ! mkdir /tmp/ai-buddy-settings-overlay.lock 2> /dev/null; do
    info "waiting for /tmp/ai-buddy-settings-overlay.lock"
    if [ "$SECONDS" -ge "$deadline" ]; then
      fail "overlay lock not free after 20 minutes"
      exit 1
    fi
    sleep 30
  done
  lock_held=1
fi

ax="$out/ax-settings"
info "Output: $out"
swiftc -O "$root/scripts/ax-settings.swift" -o "$ax" || {
  fail "could not compile scripts/ax-settings.swift"
  exit 1
}

home="$out/home"
mkdir -p "$home"
link=""
for link in .claude .claude.json .codex .config; do
  [ -e "$HOME/$link" ] && ln -sfn "$HOME/$link" "$home/$link"
done

log="$out/app.log"
env -u AI_BUDDY_DIRECTOR_API_KEY \
  HOME="$home" \
  AI_BUDDY_CAPTURABLE=1 \
  AI_BUDDY_SETTINGS_WEBVIEW=1 \
  AI_BUDDY_CHARACTER=timber-wolf \
  AI_BUDDY_CHARACTERS="${AI_BUDDY_CHARACTERS:-$root/characters}" \
  "$bin" > "$log" 2>&1 &
app_pid=$!

ready=0
waited=0
while [ "$waited" -lt 40 ]; do
  if grep -qE '^overlay:' "$log" 2> /dev/null; then
    ready=1
    break
  fi
  if ! kill -0 "$app_pid" 2> /dev/null; then
    fail "ai-buddy exited before overlay:"
    tail -40 "$log" || true
    exit 1
  fi
  sleep 0.5
  waited=$((waited + 1))
done
[ "$ready" -eq 1 ] || info "no overlay: line yet; continuing because Settings does not need the sprite"

"$ax" open "$app_pid" || {
  fail "could not open Settings from the tray"
  exit 1
}
"$ax" focus-window "$app_pid" || fail "could not focus the Settings title bar"

still() {
  local name="$1"
  local rect
  rect=$("$ax" frame "$app_pid" 2> /dev/null) || {
    info "no Settings frame for $name"
    return 0
  }
  [ -n "$rect" ] || return 0
  printf '%s\n' "$rect" > "$out/last-rect"
  screencapture -x -o -R "$rect" "$out/${name}.png" 2> /dev/null
}

downscale() {
  local src="$1"
  local dst="$2"
  [ -f "$src" ] || return 0
  sips -Z 420 "$src" --out "$dst" > /dev/null
}

focused() {
  "$ax" focused "$app_pid" 2> /dev/null || echo "none|||false"
}

key() { "$ax" key "$app_pid" "$1"; }

# Tab until the focused title equals $1. WebKit prefixes a disclosure with ▸.
tab_until() {
  local want="$1"
  local i=0
  local line="" title=""
  while [ "$i" -lt 48 ]; do
    line=$(focused)
    printf '%s\n' "$line" >> "$out/focus.log"
    title=$(printf '%s\n' "$line" | cut -d'|' -f2)
    title="${title#▸ }"
    [ "$title" = "$want" ] && return 0
    key tab
    i=$((i + 1))
  done
  return 1
}

TABS="Presence Character AI Privacy Development"

info "Tab order on all five tabs"
for tab in $TABS; do
  if tab_until "$tab"; then
    key space
    sleep 0.4
    "$ax" dump "$app_pid" > "$out/dump-${tab}.txt" || fail "dump $tab"
    still "tab-${tab}"
    count=0
    first=$(focused)
    printf '%s\n' "$first" > "$out/cycle-${tab}.txt"
    key tab
    j=0
    while [ "$j" -lt 40 ]; do
      line=$(focused)
      printf '%s\n' "$line" >> "$out/cycle-${tab}.txt"
      count=$((count + 1))
      [ "$line" = "$first" ] && [ "$count" -gt 1 ] && break
      key tab
      j=$((j + 1))
    done
    info "$tab cycle length $count"
  else
    fail "Tab never reached the $tab tab"
  fi
done

tab_order=PASS
for tab in $TABS; do
  cycle="$out/cycle-${tab}.txt"
  dump="$out/dump-${tab}.txt"
  if [ ! -s "$cycle" ]; then
    tab_order=FAIL
    continue
  fi
  # Enabled tabbable roles in dump order, titles only. Window chrome is
  # skipped because WKWebView's Tab cycle stays inside the page.
  awk -F'|' '
    $1 ~ /AXCloseButton|AXMinimizeButton|AXZoomButton|AXFullScreenButton/ { next }
    $1 ~ /^(AXButton|AXCheckBox|AXPopUpButton|AXComboBox|AXDisclosureTriangle|AXRadioButton)/ && $5 != "false" { print $2 }
    $1 ~ /^AXTextField/ && $5 != "false" { print $2 }
    $1 ~ /^AXTextArea/ && $5 != "false" { print $2 }
  ' "$dump" | awk 'NF && !seen[$0]++' > "$out/expect-${tab}.txt"
  missing=0
  while IFS= read -r title; do
    [ -z "$title" ] && continue
    grep -F "|${title}|" "$cycle" > /dev/null || {
      echo "$title" >> "$out/missing-${tab}.txt"
      missing=$((missing + 1))
    }
  done < "$out/expect-${tab}.txt"
  if [ "$missing" -gt 0 ]; then
    fail "$tab: $missing dump controls never took focus"
    tab_order=FAIL
  else
    pass "$tab: Tab reached every dump-tabbable control"
  fi
done

# Presence still is the tab-order artefact: a mid-form focus ring after a walk.
if [ -f "$out/tab-Presence.png" ]; then
  downscale "$out/tab-Presence.png" "$out/01-tab-order.png"
else
  tab_order=FAIL
fi
if [ "$tab_order" = PASS ]; then
  pass "Tab reaches every control in DOM order on all five tabs"
else
  fail "Tab order sitting failed"
fi

# Focus ring: Character tab, one Tab into the panel, still.
if tab_until "Character"; then
  key space
  sleep 0.3
  key tab
  key tab
  key tab
  key tab
  key tab
  still "focus-ring"
  downscale "$out/focus-ring.png" "$out/02-focus-ring.png"
  line=$(focused)
  printf '%s\n' "$line" > "$out/focus-ring.txt"
  if [ -f "$out/02-focus-ring.png" ] && [ "$line" != "none|||false" ]; then
    pass "focus ring still taken on $(echo "$line" | cut -d'|' -f1-2)"
    focus_ring=PASS
  else
    fail "focus ring still missing or nothing focused"
    focus_ring=FAIL
  fi
else
  fail "could not reach Character for the focus-ring still"
  focus_ring=FAIL
fi

# <select> Space then arrows. Character popup is the first <select>.
select_ok=FAIL
if tab_until "Character"; then
  key space
  sleep 0.3
  s=0
  while [ "$s" -lt 24 ]; do
    line=$(focused)
    echo "$line" | cut -d'|' -f1 | grep -Eq 'AXPopUpButton|AXComboBox' && break
    key tab
    s=$((s + 1))
  done
  before=$("$ax" menus "$app_pid")
  still "select-closed"
  key space
  sleep 0.35
  after=$("$ax" menus "$app_pid")
  still "select-open"
  downscale "$out/select-open.png" "$out/05-select-space.png"
  info "select menus before=$before after=$after focused=$(focused)"
  key down
  sleep 0.2
  still "select-arrow"
  key return
  sleep 0.4
  if [ "${after:-0}" -gt "${before:-0}" ]; then
    pass "<select> opened on Space (menu windows $before -> $after) and arrows moved"
    select_ok=PASS
  else
    fail "<select> Space did not add a menu-layer window ($before -> $after)"
  fi
else
  fail "could not reach Character for the <select> sitting"
fi

# <details> Enter. Presence "What is this?" is the first disclosure.
details_ok=FAIL
if tab_until "Presence"; then
  key space
  sleep 0.3
  if tab_until "What is this?"; then
    before=$(focused)
    still "details-closed"
    printf '%s\n' "$before" > "$out/details-before-focus.txt"
    key return
    sleep 0.3
    after=$(focused)
    still "details-open"
    downscale "$out/details-open.png" "$out/06-details-enter.png"
    printf '%s\n' "$after" > "$out/details-after-focus.txt"
    before_val=$(printf '%s\n' "$before" | cut -d'|' -f3)
    after_val=$(printf '%s\n' "$after" | cut -d'|' -f3)
    if [ "$before_val" != "$after_val" ]; then
      pass "<details> toggled on Enter ($before_val -> $after_val)"
      details_ok=PASS
    else
      fail "<details> AX value stayed $after_val after Enter"
    fi
  else
    fail "Tab never reached a What is this? disclosure"
  fi
else
  fail "could not reach Presence for the <details> sitting"
fi

# Enter applies: AI tab, First wake field. Replace 120 with 90 and Return.
# A junk string looks like it typed (the field shows it) then reverts on blur
# because the row only stores a number.
enter_ok=FAIL
if tab_until "AI"; then
  key space
  sleep 0.4
  if tab_until "First wake, in seconds"; then
    # Caret lands at the start; End then backspace clears 120 without Cmd-A,
    # which WKWebView did not take as select-all.
    key end
    key delete
    key delete
    key delete
    "$ax" type "$app_pid" "90"
    sleep 0.2
    still "enter-before"
    downscale "$out/enter-before.png" "$out/04-enter-applies.png"
    key return
    sleep 0.5
    still "enter-after"
    "$ax" dump "$app_pid" > "$out/enter-dump.txt"
    settings_json="$home/Library/Application Support/ai-buddy/settings.json"
    # AX keeps the placeholder 120 on this row. The file is what Apply wrote.
    if grep -Fq '"director_wake_secs": "90"' "$settings_json"; then
      pass "Enter applied First wake = 90 in settings.json"
      enter_ok=PASS
    else
      fail "Enter did not write director_wake_secs 90"
      grep director_wake_secs "$settings_json" || true
    fi
  else
    fail "Tab never reached First wake, in seconds"
  fi
else
  fail "could not reach AI for the Enter-applies sitting"
fi

# Escape last: it closes the window.
still "escape-before"
downscale "$out/escape-before.png" "$out/03-escape-before.png"
key escape
sleep 0.5
escape_ok=FAIL
if "$ax" frame "$app_pid" > /dev/null 2>&1; then
  fail "Escape left the Settings window open"
  still "escape-still-open"
else
  pass "Escape closed the Settings window"
  escape_ok=PASS
  if [ -f "$out/last-rect" ]; then
    screencapture -x -o -R "$(cat "$out/last-rect")" "$out/escape-after.png" 2> /dev/null || true
    downscale "$out/escape-after.png" "$out/03-escape.png"
  elif [ -f "$out/03-escape-before.png" ]; then
    cp "$out/03-escape-before.png" "$out/03-escape.png"
  fi
fi

table="$out/TABLE.md"
{
  echo '| Behaviour | Result | Still |'
  echo '| --- | --- | --- |'
  echo "| Tab reaches every control in DOM order on all five tabs | $tab_order | ![tab-order](./01-tab-order.png) |"
  echo "| Focus ring visible on each | $focus_ring | ![focus-ring](./02-focus-ring.png) |"
  echo "| Escape closes | $escape_ok | ![escape](./03-escape.png) |"
  echo "| Enter applies | $enter_ok | ![enter-applies](./04-enter-applies.png) |"
  echo "| \`<select>\` opens on Space, picks with arrows | $select_ok | ![select-space](./05-select-space.png) |"
  echo "| \`<details>\` toggles on Enter | $details_ok | ![details-enter](./06-details-enter.png) |"
} > "$table"
cat "$table"

echo
if [ "$failures" -eq 0 ]; then
  pass "all checks passed - stills under $out"
  echo "$out" > "$out/../settings-keyboard-webview-latest"
  exit 0
fi
fail "$failures check(s) failed - dumps and stills under $out"
echo "$out" > "$out/../settings-keyboard-webview-latest"
exit 1
