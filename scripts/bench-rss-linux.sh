#!/usr/bin/env bash
#
# Sample the resident set of a running ai-buddy, Linux only.
#
# RSS on Linux lives in the main process and potentially in WebKitGTK helper
# processes if WebKitGTK runs content out-of-process. This script discovers
# the process tree at launch and attributes all children to the app.
#
# Usage: scripts/bench-rss-linux.sh [--settle N] [--seconds N] [--interval N] [--out FILE] [--research]
#   Launches target/debug/ai-buddy, waits `settle` seconds, then samples every
#   interval for `seconds`, writes one TSV row per sample, prints min/median/max
#   over the sampled window and each process's peak RSS (VmHWM), then stops the
#   app.
#
#   DEFAULT: Brief smoke test (settle ~3s, sample ~10s) — enough for fast
#   verification in a test matrix. Not a research soak.
#
#   --research: Long research mode (settle 300s, sample 300s) for bathtub
#   curve analysis. The macOS script found a launch peak near twice steady
#   state, settling by ~5 minutes. Use this for measurement studies, not for
#   everyday verification.
#
#   Environment reaches the app unchanged, which is how a scenario is chosen:
#   AI_BUDDY_INSTANCES picks the roster, AI_BUDDY_CHARACTERS the packages.
#   Set HOME to a scratch directory to keep the real install's settings and
#   Action Log out of it.
#
# RSS alone does not compare two runs on a busy machine. VmHWM (peak resident
# set size from /proc/[pid]/status) only ever rises, so it survives noise.
# Both are reported. Compare scenarios on VmHWM and read the RSS series for
# shape.
#
# Nothing here is a benchmark on its own: an RSS figure means nothing without
# the roster, the display count, and what the sprite was doing. Record those
# beside the number.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

settle=3
seconds=10
interval=2
out=""
bin="target/debug/ai-buddy"

while [ $# -gt 0 ]; do
  case "$1" in
    --settle) settle="$2" && shift 2 ;;
    --seconds) seconds="$2" && shift 2 ;;
    --interval) interval="$2" && shift 2 ;;
    --out) out="$2" && shift 2 ;;
    --research) settle=300 && seconds=300 && interval=5 && shift ;;
    *) echo "unknown argument: $1" >&2 && exit 2 ;;
  esac
done

[ -x "$bin" ] || {
  echo "no $bin — run: (cd src-tauri && cargo build --bin ai-buddy)" >&2
  exit 2
}
out="${out:-$(mktemp -t ai-buddy-rss-XXXXXX).tsv}"
log="$out.app.log"

# WebKitGTK helpers on Linux (if any) are children of the main process.
# We'll discover them after launch.
"./$bin" > "$log" 2>&1 &
app=$!
trap 'kill -TERM "$app" 2>/dev/null; sleep 1; kill -KILL "$app" 2>/dev/null' EXIT INT TERM

# The overlays are what allocate; sampling before they exist measures a
# half-started app. The line is the one place the app says how many it made.
for _ in $(seq 30); do
  grep -q 'overlay: [0-9]* display' "$log" && break
  sleep 1
done
displays=$(sed -n 's/^overlay: \([0-9]*\) display.*/\1/p' "$log" | head -1)
if [ -z "$displays" ]; then
  # Check if the app failed to start (e.g., no DISPLAY)
  if grep -qi "error\|failed\|cannot" "$log" 2> /dev/null; then
    echo "app failed to start; see $log" >&2
    cat "$log" >&2
    exit 1
  fi
  echo "the app never reported its overlays; see $log" >&2
  exit 1
fi

# Find all descendant processes. WebKitGTK may spawn helpers as direct children.
# We use pgrep with parent filtering to find them.
sleep 2 # Give helpers time to spawn
children=$(pgrep -P "$app" 2> /dev/null || true)
pids=$(echo "$app" | cat - <(echo "$children") | tr '\n' ' ' | xargs)

# Count distinct pids for diagnostics
pid_count=$(echo "$pids" | wc -w)

echo "displays: $displays   main pid: $app   total processes: $pid_count"
echo "pids: $pids"
echo "settling ${settle}s, then sampling ${seconds}s every ${interval}s -> $out"

# Log process tree for forensics
ps -p "$app" -o pid,ppid,comm,args 2> /dev/null || true
for child in $children; do
  ps -p "$child" -o pid,ppid,comm,args 2> /dev/null || true
done

sleep "$settle"
printf 'epoch\ttotal_kb\t%s\n' "$(echo "$pids" | tr ' ' '\t')" > "$out"

end=$(($(date +%s) + seconds))
while [ "$(date +%s)" -lt "$end" ]; do
  # Read RSS from /proc/[pid]/status. Dead processes produce no line.
  rss=""
  for pid in $pids; do
    if [ -f "/proc/$pid/status" ]; then
      pid_rss=$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status" 2> /dev/null || echo "0")
      rss="$rss$pid_rss "
    else
      rss="${rss}0 "
    fi
  done
  total=$(echo "$rss" | awk '{s = 0; for (i = 1; i <= NF; i++) s += $i; print s}')
  printf '%s\t%s\t%s\n' "$(date +%s)" "$total" "$(echo "$rss" | tr ' ' '\t')" >> "$out"
  sleep "$interval"
done

# BSD awk compatibility: no asort, so ordering is sort's.
median() { sort -n | awk '{t[n++] = $1} END {printf "%.0f", t[int(n / 2)] / 1024}'; }

awk -F'\t' 'NR > 1 {print $2}' "$out" | sort -n |
  awk '{t[n++] = $1}
    END {
      printf "total   samples: %d   min: %.0f MB   median: %.0f MB   max: %.0f MB\n",
        n, t[0] / 1024, t[int(n / 2)] / 1024, t[n - 1] / 1024
    }'

# Per process, using VmHWM (peak RSS) from /proc/[pid]/status
column=3
for pid in $pids; do
  comm=$(ps -p "$pid" -o comm= 2> /dev/null | xargs || echo "gone")
  rss_med=$(cut -f "$column" "$out" 2> /dev/null | tail -n +2 | median)
  if [ -f "/proc/$pid/status" ]; then
    vmhwm=$(awk '/^VmHWM:/ {print $2 / 1024 " MB"}' "/proc/$pid/status" 2> /dev/null || echo "N/A")
  else
    vmhwm="N/A (process exited)"
  fi
  printf '  %-6s %-28s rss median: %4s MB   peak (VmHWM): %s\n' "$pid" "$comm" "$rss_med" "$vmhwm"
  column=$((column + 1))
done

kill -TERM "$app" 2> /dev/null
sleep 1
kill -KILL "$app" 2> /dev/null
trap - EXIT INT TERM

echo ""
echo "Log written to: $log"
echo "TSV written to: $out"
