#!/usr/bin/env bash
# macOS Settings Window smoke test, the AppKit counterpart to
# verify-settings-win.ps1.
#
# Checks what the AppKit renderer actually built: the AI tab's section order,
# the labels and help lines, and - the part no unit test can reach - whether
# the HTTP Completer rows are frozen when a Harness is the Completer and live
# when one is not.
#
# Usage:
#   ./scripts/verify-settings-macos.sh
#   AI_BUDDY_VERIFY_BIN=path/to/ai-buddy ./scripts/verify-settings-macos.sh
#   AI_BUDDY_VERIFY_HARNESS=grok ./scripts/verify-settings-macos.sh
#
# Expects a built debug binary; it does not cargo build. Two passes, one
# without a Harness and one with, each in a throwaway HOME so neither reads
# your own settings or Keychain. Output under .verify/settings-macos-<stamp>/.
#
# Needs an Accessibility grant for the terminal running it - see
# scripts/ax-settings.swift. The Harness pass needs that Harness installed and
# signed in; it is skipped, not failed, when the Harness never answers.

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

# Dumps the AI tab of a fresh instance into $1. A non-empty $2 means wait for
# the Harness to attach first. Extra environment comes in as $3 onwards, so the
# two passes differ only in AI_BUDDY_HARNESS.
dump_ai_tab() {
  local target="$1"
  local expect="$2"
  shift 2
  local home
  home="$out/home-$(basename "$target" .txt)"
  mkdir -p "$home"
  # The throwaway HOME isolates ai-buddy's own state, which lives under
  # Library/Application Support - but a Harness CLI keeps its onboarding in
  # a dotfile, and one that cannot find it never finishes starting and the
  # freeze pass silently degrades to "set but not running". Link the
  # dotfiles across; add a line here for a Harness that keeps its own
  # somewhere else.
  local link
  for link in .claude .claude.json .codex .config; do
    [ -e "$HOME/$link" ] && ln -sfn "$HOME/$link" "$home/$link"
  done
  # HOME is the only thing overridden: it isolates settings.json and the
  # Action Log from your own, while leaving the Harness its Keychain
  # credentials, which is where a signed-in CLI actually keeps them.
  # env, not a bare assignment prefix: the per-pass variables arrive in "$@"
  # and the shell only honours assignments it can see literally.
  local log
  log="$out/$(basename "$target" .txt).log"
  env HOME="$home" AI_BUDDY_CAPTURABLE=1 AI_BUDDY_CHARACTER=timber-wolf "$@" \
    "$bin" > "$log" 2>&1 &
  app_pid=$!
  # Wait for the attachment on the app's own stream before opening Settings.
  # The window builds its rows once and never re-freezes them (#625), so a
  # window opened first would report the Completer that was in force a
  # second ago rather than the one in force now.
  if [ -n "$expect" ]; then
    local waited=0
    while ! grep -Fq "harness: $harness attached" "$log" && [ "$waited" -lt 90 ]; do
      sleep 1
      waited=$((waited + 1))
    done
  fi
  "$ax" open "$app_pid" || return 1
  "$ax" tab "$app_pid" AI || return 1
  "$ax" dump "$app_pid" > "$target" || return 1
  # A still for the record, and the only way to catch a label that renders
  # but does not fit: AX reports the whole string whatever the width.
  local rect
  rect=$("$ax" frame "$app_pid" 2> /dev/null)
  [ -n "$rect" ] && screencapture -x -o -R "$rect" \
    "$out/$(basename "$target" .txt).png" 2> /dev/null
  kill "$app_pid" 2> /dev/null
  wait "$app_pid" 2> /dev/null
  app_pid=""
}

# Whether the control right after a label is live, which is how a labelled row
# is addressed here: the tree is in render order, so the control for "Base URL"
# is the line straight after the static text that says it.
#
# The role is not fixed, which is the trap. AppKit demotes a non-editable
# NSTextField from AXTextField to plain AXStaticText, so a frozen row and its
# label look alike and a matcher that waits for AXTextField walks straight past
# it into the next row. Take the next line whatever its role, and read the
# column that means "live" for that role: settability for a field, enabled for
# a popup or a button, and nothing at all for the demoted static text, which is
# frozen by definition.
row_live() {
  awk -F'|' -v want="$2" '
    found {
      if ($1 ~ /AXTextField/) { print $6 }
      else if ($1 ~ /AXPopUpButton|AXButton/) { print $5 }
      else { print "false" }
      exit
    }
    $3 == want { found = 1 }
  ' "$1"
}

has_line() { grep -Fq "$2" "$1"; }

section_order() {
  # Section headings are the static texts whose value matches a heading the
  # form declares. Reading them in tree order is what makes the order
  # assertable without a screenshot.
  # Deduplicated because "AI source" is both a heading and the label of the
  # row under it, and only the first is a section.
  grep -E '^AXStaticText\|\|(AI|AI source|Model / API|Last user turn)\|' "$1" |
    cut -d'|' -f3 | awk '!seen[$0]++' | paste -sd'>' - | sed 's/>/ > /g'
}

info "Pass 1: no Harness"
if ! dump_ai_tab "$out/no-harness.txt" ""; then
  fail "could not read the Settings window without a Harness"
  exit 1
fi
plain="$out/no-harness.txt"

info "Section order: $(section_order "$plain")"

for label in "Base URL" "Model"; do
  if [ "$(row_live "$plain" "$label")" = "true" ]; then
    pass "$label is live with no Harness attached"
  else
    fail "$label should be editable with no Harness attached"
  fi
done

if has_line "$plain" 'the Director'"'"'s "AI brain"'; then
  pass 'the state line says "AI brain"'
else
  fail 'the state line no longer says "AI brain"'
fi

if has_line "$plain" "is the Director's mind"; then
  fail '"mind" is back in the state line'
else
  pass '"mind" is gone from the state line'
fi

info "Pass 2: Harness · $harness"
if ! dump_ai_tab "$out/harness.txt" wait AI_BUDDY_HARNESS="$harness"; then
  fail "could not read the Settings window with $harness attached"
  exit 1
fi
driven="$out/harness.txt"

# "attached, session X" and "attached; no session opened yet" are both the
# driving state - the session opens a moment after the child answers. The
# punctuation is what separates them from "attached but not authenticated",
# which is not driving and must not freeze anything.
if grep -Eq "$harness attached[,;]" "$driven"; then
  for label in "Base URL" "Model"; do
    if [ "$(row_live "$driven" "$label")" = "false" ]; then
      pass "$label is frozen while $harness drives"
    else
      fail "$label must be frozen while $harness drives (#452)"
    fi
  done
  if [ "$(row_live "$driven" "Model / API")" = "false" ]; then
    pass "the endpoint picker is frozen while $harness drives"
  else
    fail "the endpoint picker must be frozen while $harness drives"
  fi
else
  # Set but never answering is a real state, and freezing on it would leave
  # no reachable Completer at all - so it is not a failure here, just not
  # the state these three checks are about (#452).
  skip "$harness never answered; the freeze checks need a signed-in Harness"
  grep -F "$harness" "$driven" | head -3
fi

echo
if [ "$failures" -eq 0 ]; then
  pass "all checks passed - dumps and stills under $out"
  exit 0
fi
fail "$failures check(s) failed - dumps and stills under $out"
exit 1
