#!/usr/bin/env bash
# Test row_live() in verify-settings-macos.sh against recorded dumps. It is the
# one piece of that script with logic in it, and a live run needs a window, an
# Accessibility grant and a display. Columns: role|title|value|placeholder|enabled|settable.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

sed -n '/^row_live()/,/^}/p' scripts/verify-settings-macos.sh > "$TEMP_DIR/function.sh"
# shellcheck disable=SC1091
. "$TEMP_DIR/function.sh"

# The AI tab with Model API picked: every HTTP row is editable. "Apply" is
# disabled because nothing was edited, a negative control matched by the same
# rule as "Clear key". The toolbar is here so a "Model" tab can shadow the "Model" row.
cat > "$TEMP_DIR/live.txt" << 'EOF'
AXWindow:AXStandardWindow|Settings||||
AXTabGroup|||||false
AXRadioButton:AXTabButton|Presence|0||true|false
AXRadioButton:AXTabButton|Model|0||true|false
AXStaticText||AI source||true|false
AXPopUpButton|Model API|||true|false
AXStaticText||Base URL||true|false
AXTextField||https://api.anthropic.com/v1||true|true
AXStaticText||Model||true|false
AXTextField||claude-sonnet-4||true|true
AXStaticText||API key||true|false
AXTextField:AXSecureTextField|||sk-ant-...|true|true
AXButton|Clear key|||true|false
AXButton|Apply|||false|false
EOF

# The same rows out of the webview window, recorded under
# AI_BUDDY_SETTINGS_WEBVIEW=1. WebKit sent every name twice until #706 wrapped
# the control in its label - the label element and its own text run, both
# AXStaticText - and the second block is what it sends now.
cat > "$TEMP_DIR/webview-doubled.txt" << 'EOF'
AXStaticText||Base URL||true|false
AXStaticText||Base URL||true|false
AXTextField|Base URL||https://api.openai.com|true|true
AXStaticText||Model||true|false
AXStaticText||Model||true|false
AXTextField|Model||gpt-4o-mini|true|true
AXButton|Clear key|||true|false
EOF

cat > "$TEMP_DIR/webview.txt" << 'EOF'
AXStaticText||Base URL||true|false
AXTextField|Base URL||https://api.openai.com|true|true
AXStaticText||Model||true|false
AXTextField|Model||gpt-4o-mini|true|true
AXButton|Clear key|||true|false
EOF

# The same tab with a Harness attached. The text fields are gone: AppKit
# demoted each one to AXStaticText when the renderer called setEditable(false),
# which is why the frozen rows here look like their own labels.
cat > "$TEMP_DIR/frozen.txt" << 'EOF'
AXStaticText||Base URL||true|false
AXStaticText||https://api.anthropic.com/v1||true|false
AXStaticText||API key||true|false
AXStaticText||........||true|false
AXButton|Clear key|||false|false
EOF

failures=0
expect() {
  local dump="$1" label="$2" want="$3" what="$4"
  local got
  got=$(row_live "$dump" "$label")
  if [ "$got" = "$want" ]; then
    echo "  PASS: $what"
  else
    echo "  FAIL: $what - wanted '$want', got '$got'"
    failures=$((failures + 1))
  fi
}

echo "Running row_live tests..."

# A label names the row and the control is the line after it.
expect "$TEMP_DIR/live.txt" "API key" true "a labelled row is live when its field is settable"
expect "$TEMP_DIR/frozen.txt" "API key" false "a labelled row is frozen when its field was demoted"

# A button carries its own name in the title column, so there is no label line
# above it to match and the answer has to come from the button's own line.
expect "$TEMP_DIR/live.txt" "Clear key" true "a self-naming control is live when enabled"
expect "$TEMP_DIR/frozen.txt" "Clear key" false "a self-naming control is frozen when disabled"
expect "$TEMP_DIR/live.txt" "Apply" false "a self-naming control that is disabled still reads frozen"

# A tab button titled "Model" sits above the "Model" row in the same tree. The
# row is the answer; a title match that fired on any role would hand back the
# tab instead, because render order puts the toolbar first.
expect "$TEMP_DIR/live.txt" "Model" true "a toolbar tab does not shadow the row it shares a name with"

# A name that reaches the tree twice is the window's business, not the row's:
# the control is the first line after the label that says something else.
expect "$TEMP_DIR/webview-doubled.txt" "Base URL" true "a repeated label does not answer for its own row"
expect "$TEMP_DIR/webview-doubled.txt" "Model" true "a repeated label does not shadow the field under it"
expect "$TEMP_DIR/webview.txt" "Base URL" true "a webview row is live when its field is settable"
expect "$TEMP_DIR/webview.txt" "Clear key" true "a webview button answers off its own line"

# A label that is not in the dump has to read as neither live nor frozen.
# Both expect_live and expect_frozen compare against a word, so an empty answer
# fails whichever one asked - a renamed row is a red line, not a silent pass.
expect "$TEMP_DIR/live.txt" "Nonexistent row" "" "a label that appears nowhere answers nothing"

echo
if [ "$failures" -eq 0 ]; then
  echo "All row_live tests passed."
  exit 0
fi
echo "$failures row_live test(s) failed."
exit 1
