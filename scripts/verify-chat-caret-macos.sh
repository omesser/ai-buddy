#!/usr/bin/env bash
# The Chat composer draws a blinking text caret (#891).
#
# TAKES THE SCREEN for about half a minute. Ask whoever is at the machine
# first. It posts no click and no keystroke: AI_BUDDY_OPEN_CHAT opens the Chat
# surface and an accessibility action opens the Prompt tab.
#
# Covers pixels and nothing else. AX has no caret — a focused field reports
# AXSelectedTextRange whether or not one is painted, which is why this was
# filed against a window every AX assertion called healthy. So it proves that
# something narrow blinks where the caret belongs, and not its colour, its
# place, or that it follows an arrow key.
#
# Usage:
#   ./scripts/verify-chat-caret-macos.sh
#   AI_BUDDY_VERIFY_BIN=path/to/ai-buddy ./scripts/verify-chat-caret-macos.sh
#
# Needs an Accessibility grant and a Screen Recording grant. CI does not run it.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

bin="${AI_BUDDY_VERIFY_BIN:-$root/target/debug/ai-buddy}"
out="$root/.verify/chat-caret-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"

app_pid=""

info() { printf '\033[36m[INFO]\033[0m %s\n' "$*"; }
pass() { printf '\033[32m[PASS]\033[0m %s\n' "$*"; }
fail() { printf '\033[31m[FAIL]\033[0m %s\n' "$*"; }

# Inline: shellcheck cannot see that trap calls a function.
trap '[ -n "$app_pid" ] && kill "$app_pid" 2> /dev/null' EXIT

[ -x "$bin" ] || {
  fail "no binary at $bin - cargo build -p ai-buddy first, or set AI_BUDDY_VERIFY_BIN"
  exit 1
}

info "Output: $out"
probe="$out/ax-chat-caret"
swiftc -O "$root/scripts/ax-chat-caret.swift" -o "$probe" || {
  fail "could not compile scripts/ax-chat-caret.swift"
  exit 1
}

# A scratch HOME, so a sitting never edits the settings of whoever ran it.
home="$out/home"
mkdir -p "$home"
for link in .claude .claude.json .codex .config; do
  [ -e "$HOME/$link" ] && ln -sfn "$HOME/$link" "$home/$link"
done

log="$out/app.log"
# The key skips the Keychain modal that otherwise holds startup; without it the
# app traces no frames and reads as a broken build. Capturable, or the stills
# come back without the window.
HOME="$home" \
  AI_BUDDY_DIRECTOR_API_KEY="${AI_BUDDY_DIRECTOR_API_KEY:-verify}" \
  AI_BUDDY_CAPTURABLE=1 \
  AI_BUDDY_OPEN_CHAT=1 \
  AI_BUDDY_CHARACTER="${AI_BUDDY_CHARACTER:-timber-wolf}" \
  AI_BUDDY_CHARACTERS="${AI_BUDDY_CHARACTERS:-$root/characters}" \
  "$bin" > "$log" 2>&1 &
app_pid=$!

waited=0
while [ "$waited" -lt 40 ]; do
  grep -qE '^overlay:' "$log" 2> /dev/null && break
  if ! kill -0 "$app_pid" 2> /dev/null; then
    fail "ai-buddy exited before overlay:"
    tail -40 "$log" || true
    exit 1
  fi
  sleep 0.5
  waited=$((waited + 1))
done

# The Chat window carries the Instance name, which is the Character's unless
# the roster renamed it. The log line says which Instances came up.
title="${AI_BUDDY_VERIFY_CHAT_TITLE:-$(sed -n 's/.*sprite [0-9]*x[0-9]*; \([^,]*\) as \(.*\)$/\2/p' "$log" | head -1)}"
[ -n "$title" ] || title="Timber Wolf"
info "Chat window title: $title"

"$probe" "$app_pid" "$title" "$out" 16 > "$out/caret.txt" 2>&1
rc=$?
cat "$out/caret.txt"

if [ "$rc" -eq 0 ]; then
  pass "composer caret proven - stills under $out"
  exit 0
fi
fail "composer caret not proven - stills and reasoning under $out"
exit 1
