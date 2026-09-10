#!/usr/bin/env bash
#
# Sample the resident set of a running ai-buddy, macOS only.
#
# RSS lives in more than one process. The app is the Rust binary plus the
# WebKit XPC helpers WKWebView spawns for it — one WebContent per overlay,
# one GPU process, one Networking process — and those are children of launchd,
# not of the app, so no process-tree walk finds them. This takes the set of
# WebKit helpers before launch and after, and calls the difference ours.
#
# Usage: scripts/bench-rss.sh [--settle N] [--seconds N] [--interval N] [--out FILE]
#   Launches target/debug/ai-buddy, waits `settle` seconds, then samples every
#   interval for `seconds`, writes one TSV row per sample, prints min/median/max
#   over the sampled window and each process's peak physical footprint, then
#   stops the app.
#
#   Settling is not politeness. A launch peaks near twice its steady state and
#   takes about five minutes to come down: 396 MB at launch, 176 MB at 150 s,
#   back near 225 MB by 300 s and only drifting after that. Sample the first
#   minute and the number you publish is the loader's, not the app's.
#
#   Environment reaches the app unchanged, which is how a scenario is chosen:
#   AI_BUDDY_INSTANCES picks the roster, AI_BUDDY_CHARACTERS the packages.
#   Set HOME to a scratch directory to keep the real install's settings and
#   Action Log out of it.
#
# RSS alone does not compare two runs. It is what macOS has let the process
# keep, so a busy machine reclaims pages from an idle buddy and the same app
# reads 150 MB lighter for reasons that have nothing to do with the app. Peak
# physical footprint — Activity Monitor's "Memory" column, and what `vmmap`
# reports — only ever rises, so it is the figure that survives a noisy machine.
# Both are printed. Compare scenarios on the footprint and read the RSS series
# for shape.
#
# Nothing here is a benchmark on its own: an RSS figure means nothing without
# the roster, the display count and what the sprite was doing. Record those
# beside the number — docs/research/memory-rss-and-multi-monitor.md is where
# this repository's runs live.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

settle=300
seconds=300
interval=5
out=""
bin="target/debug/ai-buddy"

while [ $# -gt 0 ]; do
  case "$1" in
    --settle) settle="$2" && shift 2 ;;
    --seconds) seconds="$2" && shift 2 ;;
    --interval) interval="$2" && shift 2 ;;
    --out) out="$2" && shift 2 ;;
    *) echo "unknown argument: $1" >&2 && exit 2 ;;
  esac
done

[ -x "$bin" ] || {
  echo "no $bin — run: (cd src-tauri && cargo build --bin ai-buddy)" >&2
  exit 2
}
out="${out:-$(mktemp -t ai-buddy-rss).tsv}"
log="$out.app.log"

# Everything WebKit is already running belongs to some other application.
before=$(pgrep -f com.apple.WebKit || true)

"./$bin" > "$log" 2>&1 &
app=$!
trap 'kill "$app" 2>/dev/null' EXIT INT TERM

# The overlays are what allocate; sampling before they exist measures a
# half-started app. The line is the one place the app says how many it made.
for _ in $(seq 30); do
  grep -q 'overlay: [0-9]* display' "$log" && break
  sleep 1
done
displays=$(sed -n 's/^overlay: \([0-9]*\) display.*/\1/p' "$log" | head -1)
[ -n "$displays" ] || {
  echo "the app never reported its overlays; see $log" >&2
  exit 1
}

helpers=$(comm -13 <(echo "$before" | sort) <(pgrep -f com.apple.WebKit | sort))
pids=$(echo "$app $helpers" | tr '\n' ' ' | xargs)

# One WebContent per display, plus the GPU and Networking processes. Any other
# count means another WebKit application started a helper inside the same few
# seconds and the set difference caught it: the run is contaminated, not fixable
# after the fact, and worth rerunning on a quieter machine.
expected=$((displays + 2))
found=$(echo "$helpers" | grep -c .)
[ "$found" -eq "$expected" ] ||
  echo "warning: $found WebKit helpers, expected $expected — another application's are in this set" >&2

echo "displays: $displays   pids: $pids"
echo "settling ${settle}s, then sampling ${seconds}s every ${interval}s -> $out"
sleep "$settle"
printf 'epoch\ttotal_kb\t%s\n' "$(echo "$pids" | tr ' ' '\t')" > "$out"

end=$(($(date +%s) + seconds))
while [ "$(date +%s)" -lt "$end" ]; do
  # A helper that died reports nothing, so the row is short rather than wrong.
  rss=$(ps -o rss= -p "$(echo "$pids" | tr ' ' ',')" 2> /dev/null | tr -d ' ')
  total=$(echo "$rss" | awk '{s += $1} END {print s}')
  printf '%s\t%s\t%s\n' "$(date +%s)" "$total" "$(echo "$rss" | tr '\n' '\t')" >> "$out"
  sleep "$interval"
done

# BSD awk has no asort, so the ordering is sort's and the arithmetic is awk's.
median() { sort -n | awk '{t[n++] = $1} END {printf "%.0f", t[int(n / 2)] / 1024}'; }

awk -F'\t' 'NR > 1 {print $2}' "$out" | sort -n |
  awk '{t[n++] = $1}
    END {
      printf "total   samples: %d   min: %.0f MB   median: %.0f MB   max: %.0f MB\n",
        n, t[0] / 1024, t[int(n / 2)] / 1024, t[n - 1] / 1024
    }'

# Per process, because the per-display cost is one WebContent and nothing else:
# the overlays are one webview each, and the Rust side does not fork per screen.
# `vmmap` reads the peak while the process is still alive; after the kill below
# there is nothing left to ask.
column=3
for pid in $pids; do
  printf '  %-6s %-28s rss median: %4s MB   footprint peak: %s\n' "$pid" \
    "$(ps -o comm= -p "$pid" 2> /dev/null | xargs -n1 basename)" \
    "$(cut -f "$column" "$out" | tail -n +2 | median)" \
    "$(vmmap --summary "$pid" 2> /dev/null |
      sed -n 's/^Physical footprint (peak): *//p')"
  column=$((column + 1))
done
