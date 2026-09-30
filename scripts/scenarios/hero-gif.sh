#!/usr/bin/env bash
# Scenario: hero-gif (macOS)
# On screen: launches Fidget as Buddy Bot with a fixture Harness, then records
#   the main display for 20 s. The terminal prompts you through: pick the
#   sprite up and throw it, let it land and perch on a window's top edge,
#   double-click it. Chat opens and takes focus, and the Harness replies
#   "Hello". Fidget quits when the scenario ends.
# Input: yours, at the mouse, on the terminal's cue. The script sends none.
# Duration: about 45 s, 2 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: the recording exists and runs 19 to 21 s. The look is yours to judge
#   from the contact sheet and the full-frame GIF in the evidence directory.
#
# Usage: hero-gif.sh --go <fidget binary> <fidget test binary>
#        hero-gif.sh --crop x:y:w:h <recording.mp4> [<from s> [<length s>]]
# Without --go it prints this header, which is the takeover prompt, and exits 2.
# --crop re-encodes a saved recording to docs/readme/hero.gif and launches
# nothing. x:y:w:h is in recording pixels, so 2x on a Retina display.
set -euo pipefail

ffmpeg=/opt/homebrew/bin/ffmpeg
root=$(cd "$(dirname "$0")/../.." && pwd)

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

bin=${2:?usage: hero-gif.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: hero-gif.sh --go <fidget binary> <fidget test binary>}
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
out="${TMPDIR:-/tmp}/fidget-scenario-hero-gif-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out/home"
log="$out/app.log" marks="$out/harness.log" rec="$out/recording.mp4"
: > "$marks"

# avfoundation numbers the displays after the cameras, so look the index up.
screen=$("$ffmpeg" -hide_banner -f avfoundation -list_devices true -i "" 2>&1 |
  sed -n "s/.*\[\([0-9]*\)\] Capture screen ${FIDGET_HERO_DISPLAY:-0}\$/\1/p")
[ -n "$screen" ] || fail "no avfoundation screen device; grant Screen Recording to the terminal"

# FIDGET_HARNESS splits on whitespace, so no path in it may hold a space.
harness="$test_bin harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 script=hero count=$marks"
[ "$(wc -w <<< "$harness")" -eq 7 ] || fail "a path in the Harness line holds a space: $harness"

env HOME="$out/home" \
  FIDGET_HARNESS="$harness" \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 \
  FIDGET_CHARACTER=buddy-bot FIDGET_CHARACTERS="$root/characters" \
  "$bin" > "$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2> /dev/null || true; pkill -f "count=$marks" || true' EXIT

sleep 4
kill -0 "$pid" 2> /dev/null || fail "Fidget exited; see $log"
"$ffmpeg" -y -v error -f avfoundation -capture_cursor 1 -framerate 30 -i "$screen:none" \
  -t 20 -vf 'crop=trunc(iw/2)*2:trunc(ih/2)*2' -c:v libx264 -preset ultrafast -crf 18 -pix_fmt yuv420p \
  "$rec" > "$out/ffmpeg.log" 2>&1 &
ffpid=$!

cue() { # <at s> <text>
  echo ">>> ${1}s  $2"
}
cue 0 "recording. Pick Buddy Bot up and throw it at a window's top edge."
sleep 6
cue 6 "let it land and perch. Hands off."
sleep 4
cue 10 "double-click it. Chat opens and the reply arrives."
sleep 10
cue 20 "done. Hands off while the recording closes."
wait "$ffpid" || fail "ffmpeg failed; see $out/ffmpeg.log"

[ -s "$rec" ] || fail "no recording at $rec"
secs=$(/opt/homebrew/bin/ffprobe -v error -show_entries format=duration -of csv=p=0 "$rec")
awk -v s="$secs" 'BEGIN { exit !(s >= 19 && s <= 21) }' || fail "recording runs ${secs}s, want 19 to 21"

"$ffmpeg" -y -v error -i "$rec" -vf 'fps=1,scale=480:-1,tile=5x4' "$out/contact-sheet.png"
to_gif "$rec" "$out/full-frame.gif" "" 0 20
echo "PASS: evidence in $out"
echo "next: $0 --crop x:y:w:h $rec [<from s> [<length s>]]"
