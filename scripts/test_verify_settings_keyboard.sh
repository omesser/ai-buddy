#!/usr/bin/env bash
# The pure checks in verify-settings-keyboard-webview.sh against fixtures (#854):
# the focus-sequence comparison must reject reordered, duplicate-label and
# unlabeled controls, a tab still must exist with a focused control, and a
# <select> must change value. No app, no Accessibility grant.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT
FAILED=0

sed -n \
  -e '/^identity()/,/^}/p' \
  -e '/^expected_sequence()/,/^}/p' \
  -e '/^observed_sequence()/,/^}/p' \
  -e '/^check_tab_sequence()/,/^}/p' \
  -e '/^check_tab_still()/,/^}/p' \
  -e '/^check_select_commit()/,/^}/p' \
  scripts/verify-settings-keyboard-webview.sh > "$TEMP_DIR/functions.sh"
# shellcheck disable=SC1091
. "$TEMP_DIR/functions.sh"
pass() { :; }
fail() { :; }

# expect <PASS|FAIL> <name> <command...>
expect() {
  local want="$1" name="$2"
  shift 2
  local got=FAIL
  "$@" > /dev/null 2>&1 && got=PASS
  if [ "$got" = "$want" ]; then
    echo "  PASS  $name ($want)"
  else
    echo "  FAIL  $name: wanted $want, got $got"
    FAILED=1
  fi
}

# Dump columns: role|title|value|placeholder|enabled|settable.
cat > "$TEMP_DIR/dump.txt" << 'EOF'
AXWindow|Settings||||
AXButton:AXCloseButton||||true|
AXRadioButton|Presence|1||true|
AXRadioButton|Character|0||true|
AXRadioButton|AI|0||true|
AXStaticText||Presence controls|||
AXCheckBox|Hide from captures|0||true|
AXPopUpButton|Sound|On||true|
AXButton|Remove|||true|
AXButton|Remove|||true|
AXButton||||true|
AXButton|What is this?|||true|
AXButton|Frozen|||false|
EOF

# Cycle columns: role|title|value|enabled, first line repeated on the wrap.
cat > "$TEMP_DIR/cycle.txt" << 'EOF'
AXRadioButton|Presence|1|true
AXRadioButton|Character|0|true
AXRadioButton|AI|0|true
AXCheckBox|Hide from captures|0|true
AXPopUpButton|Sound|On|true
AXButton|Remove||true
AXButton|Remove||true
AXButton|||true
AXButton|▸ What is this?|false|true
AXRadioButton|Presence|1|true
EOF

echo "Test: focus sequence"
seq_case() { # name want fixture-edit
  local name="$1" want="$2" edit="$3"
  local dir="$TEMP_DIR/$name"
  mkdir -p "$dir"
  eval "$edit" < "$TEMP_DIR/cycle.txt" > "$dir/cycle.txt"
  expect "$want" "$name" check_tab_sequence Presence "$TEMP_DIR/dump.txt" "$dir/cycle.txt" "$dir"
}
seq_case matching-cycle PASS 'cat'
# Space can leave focus anywhere in the tab bar; the cycle is a rotation.
seq_case cycle-starting-mid-dump PASS \
  "awk 'NR >= 4 && NR <= 9 { print } NR <= 3 { held = held \$0 \"\\n\" } END { printf \"%s\", held; print \"AXCheckBox|Hide from captures|0|true\" }'"
seq_case reordered-controls FAIL "awk 'NR == 4 { held = \$0; next } NR == 5 { print; print held; next } { print }'"
seq_case duplicate-label-visited-once FAIL "awk '!(\$0 == \"AXButton|Remove||true\" && seen++)'"
seq_case unlabeled-control-skipped FAIL "grep -vx 'AXButton|||true'"
seq_case focus-left-the-form-without-wrapping FAIL "sed '\$d'; echo 'AXWebArea|||true'"
seq_case start-not-in-dump FAIL "sed '1s/.*/AXStaticText|Presence controls||true/'"

echo
echo "Test: one still per tab with a focused control"
printf 'PNG' > "$TEMP_DIR/full.png"
: > "$TEMP_DIR/empty.png"
expect PASS still-with-focus check_tab_still Presence "$TEMP_DIR/full.png" 'AXCheckBox|Hide from captures|0|true'
expect FAIL still-missing check_tab_still Presence "$TEMP_DIR/absent.png" 'AXCheckBox|Hide from captures|0|true'
expect FAIL still-empty check_tab_still Presence "$TEMP_DIR/empty.png" 'AXCheckBox|Hide from captures|0|true'
expect FAIL still-without-focus check_tab_still Presence "$TEMP_DIR/full.png" 'none|||false'
expect FAIL still-focus-unrecorded check_tab_still Presence "$TEMP_DIR/full.png" ''

echo
echo "Test: <select> commits a new value"
expect PASS select-changed check_select_commit 'AXPopUpButton|Character|Timber Wolf|true' 'AXPopUpButton|Character|BMO|true'
expect FAIL select-unchanged check_select_commit 'AXPopUpButton|Character|Timber Wolf|true' 'AXPopUpButton|Character|Timber Wolf|true'
expect FAIL select-lost-focus check_select_commit 'AXPopUpButton|Character|Timber Wolf|true' 'AXButton|Apply||true'

echo
if [ "$FAILED" = "0" ]; then
  echo "All settings-keyboard fixture tests passed."
else
  echo "Some settings-keyboard fixture tests FAILED."
  exit 1
fi
