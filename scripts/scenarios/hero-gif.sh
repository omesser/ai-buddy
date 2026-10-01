#!/usr/bin/env bash
# Scenario: hero-gif (macOS)
# On screen: launches Fidget as one Character (default Buddy Bot) with a Harness
#   and records the main display for 45 s. The terminal cues five beats: throw
#   the sprite at a window's top edge, poke it, double-click it (Chat opens; keep
#   it open, or the first-run tour bubble lands 25 s after launch), type a
#   question, Enter, and throw it again once the reply lands as a speech bubble
#   and in Chat: Claude Code's words under --harness claude, "Hello" from the fixture.
# Input: yours, at the mouse and keyboard. Each cue waits for the sprite to speak. None sent.
# Duration: about 70 s, 2 min at most. Grants: Screen Recording, Accessibility.
# Asserts: the recording exists and runs 44 to 46 s. The look is yours to judge
#   from the contact sheet and the full-frame GIF in the evidence directory.
#
# Usage: hero-gif.sh --go [--harness fixture|claude|grok] [--character <id>] <fidget binary> <fidget test binary>
#        hero-gif.sh --crop x:y:w:h <recording.mp4> [<from s> [<length s>]]
# Without --go it prints this header, which is the takeover prompt, and exits 2.
# --harness claude links ~/.claude, ~/.claude.json, ~/.npm and ~/Library/Keychains
#   into the private HOME: sign-in and warm npx carry over, Fidget's data stays isolated.
# --crop re-encodes a saved recording to docs/readme/hero.gif, launching
#   nothing; x:y:w:h is in recording pixels, so 2x on a Retina display.
set -euo pipefail

ffmpeg=/opt/homebrew/bin/ffmpeg
root=$(cd "$(dirname "$0")/../.." && pwd)
record=45

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Two passes: 128 colours drawn from what moves, ordered dither. Under 3 MB
# for a 15 s loop needs 15 fps and a crop, not the whole display.
to_gif() { # <in> <out> <crop filter or empty> <from s> <length s>
  local vf="fps=15,${3}scale=800:-1:flags=lanczos"
  "$ffmpeg" -y -v error -ss "$4" -t "$5" -i "$1" \
    -vf "$vf,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" \
    -loop 0 "$2"
  echo "$2: $(du -h "$2" | cut -f1)"
}

case "${1:-}" in
  --crop)
    crop=${2:?usage: hero-gif.sh --crop x:y:w:h <recording.mp4> [<from s> [<length s>]]}
    rec=${3:?usage: hero-gif.sh --crop x:y:w:h <recording.mp4> [<from s> [<length s>]]}
    IFS=: read -r x y w h <<< "$crop"
    to_gif "$rec" "$root/docs/readme/hero.gif" "crop=$w:$h:$x:$y," "${4:-0}" "${5:-15}"
    exit 0
    ;;
  --go) ;;
  *)
    sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
    exit 2
    ;;
esac
shift

usage="usage: hero-gif.sh --go [--harness fixture|claude|grok] [--character <id>] <fidget binary> <fidget test binary>"
harness_kind=fixture character=buddy-bot
while [ $# -gt 2 ]; do
  case "$1" in
    --harness) harness_kind=$2 ;;
    --character) character=$2 ;;
    *) fail "$usage" ;;
  esac
  shift 2
done
bin=${1:?$usage}
test_bin=${2:?$usage}
# Buddy Bot draws 90 px square and Trump 108 px at 1x, so a crop tuned for
# one is loose or tight on the other.
[ -d "$root/characters/$character" ] || fail "no such character: $root/characters/$character"
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
out="${TMPDIR:-/tmp}/fidget-scenario-hero-gif-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out/home"
home="$out/home"
log="$out/app.log" marks="$out/harness.log" rec="$out/recording.mp4"
: > "$marks"

# avfoundation numbers the displays after the cameras, so look the index up.
# ffmpeg exits non-zero after listing, so the pipeline must not trip pipefail.
screen=$({ "$ffmpeg" -hide_banner -f avfoundation -list_devices true -i "" 2>&1 || true; } |
  sed -n "s/.*\[\([0-9]*\)\] Capture screen ${FIDGET_HERO_DISPLAY:-0}\$/\1/p")
[ -n "$screen" ] || fail "no avfoundation screen device; grant Screen Recording to the terminal"

case "$harness_kind" in
  fixture)
    # FIDGET_HARNESS splits on whitespace, so no path in it may hold a space.
    harness="$test_bin harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 script=hero count=$marks"
    [ "$(wc -w <<< "$harness")" -eq 7 ] || fail "a path in the Harness line holds a space: $harness"
    ;;
  claude)
    harness=claude
    command -v npx > /dev/null || fail "npx is not on PATH; the claude Harness runs on Node"
    # claude-agent-acp needs Node 22 and, on an older one, hangs instead of attaching.
    node_major=$(node -p 'process.versions.node.split(".")[0]')
    [ "$node_major" -ge 22 ] || fail "node $(node -v) is on PATH; the claude Harness needs 22 or newer"
    # The login sits in the login keychain, not under ~/.claude, and `security`
    # falls back to `$HOME/Library/Keychains/login.keychain-db` for its search
    # list, so CLAUDE_CONFIG_DIR alone would still read no keychain.
    [ -e "$HOME/.claude.json" ] || fail "no $HOME/.claude.json; sign in with \`claude /login\` first"
    mkdir -p "$out/home/Library"
    for entry in .claude .claude.json .npm Library/Keychains; do
      [ -e "$HOME/$entry" ] || fail "no $HOME/$entry"
      ln -s "$HOME/$entry" "$out/home/$entry"
    done
    ;;
  grok)
    # grok signs in from its own files under the real HOME and needs no Node.
    harness=grok home=$HOME
    command -v grok > /dev/null || fail "grok is not on PATH"
    ;;
  *) fail "$usage" ;;
esac

env HOME="$home" \
  FIDGET_HARNESS="$harness" FIDGET_TRACE_DIRECTOR=1 \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 \
  FIDGET_CHARACTER="$character" FIDGET_CHARACTERS="$root/characters" \
  "$bin" > "$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2> /dev/null || true; pkill -f "count=$marks" || true' EXIT

sleep 4
kill -0 "$pid" 2> /dev/null || fail "Fidget exited; see $log"
# A real Harness opens with its own turn. Record once that reply lands, or a
# question typed on cue queues behind it and misses the take.
if [ "$harness_kind" != fixture ]; then
  echo ">>> warming up: waiting for $harness_kind's first reply before recording (60 s at most)"
  for _ in $(seq 60); do
    grep -q '^director: .* playing ' "$log" && break
    kill -0 "$pid" 2> /dev/null || fail "Fidget exited; see $log"
    sleep 1
  done
fi
"$ffmpeg" -y -v error -f avfoundation -capture_cursor 1 -framerate 30 -i "$screen:none" \
  -t "$record" -vf 'crop=trunc(iw/2)*2:trunc(ih/2)*2' -c:v libx264 -preset ultrafast -crf 18 -pix_fmt yuv420p \
  "$rec" > "$out/ffmpeg.log" 2>&1 &
ffpid=$!
t0=$SECONDS

now() { echo $((SECONDS - t0)); }
at() { # <s>: sleep until s seconds into the recording
  while [ "$(now)" -lt "$1" ]; do sleep 1; done
}
cue() { # <text>
  echo ">>> $(now)s  $1"
}
# FIDGET_TRACE_DIRECTOR logs a `playing` line for every bubble the sprite speaks.
spoken() { grep -c '^director: .* playing ' "$log" || true; }
# Each beat starts once the sprite answers the last one. A beat that gets no
# answer is cued anyway while there is time left to act on it.
after_speech() { # <text>
  local before
  before=$(spoken)
  while [ "$(spoken)" -le "$before" ] && [ "$(now)" -lt $((record - 5)) ]; do sleep 0.5; done
  cue "$1"
}

cue "recording. Pick the sprite up and throw it hard at the window's top edge."
after_speech "it spoke. Click it once, a poke, then move the mouse off him."
after_speech "it spoke. Double-click it without moving the mouse. Chat opens; leave it open."
after_speech "it spoke. Type in Chat: What's in the news today?  Then press Enter."
after_speech "it answered. Pick it up and throw it once more, anywhere."
at "$record"
cue "done. Hands off while the recording closes."
wait "$ffpid" || fail "ffmpeg failed; see $out/ffmpeg.log"

[ -s "$rec" ] || fail "no recording at $rec"
secs=$(/opt/homebrew/bin/ffprobe -v error -show_entries format=duration -of csv=p=0 "$rec")
awk -v s="$secs" -v r="$record" 'BEGIN { exit !(s >= r - 1 && s <= r + 1) }' || fail "recording runs ${secs}s, want $((record - 1)) to $((record + 1))"

"$ffmpeg" -y -v error -i "$rec" -vf 'fps=1,scale=480:-1,tile=5x9' "$out/contact-sheet.png"
to_gif "$rec" "$out/full-frame.gif" "" 0 "$record"
echo "PASS: evidence in $out"
echo "next: $0 --crop x:y:w:h $rec [<from s> [<length s>]]"
