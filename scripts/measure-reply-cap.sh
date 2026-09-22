#!/usr/bin/env bash
# Measure what a wake spends, so #606 §D's default cap is settled with numbers.
# Sends a representative set of wakes at the configured Completer and reports
# thinking against answer tokens per call, and how often a turn was truncated.
#
# Usage: scripts/measure-reply-cap.sh [--cap N] [--runs N] [--effort L] [--self-check]
#   --cap N       max_tokens per call. Default 512, the number §D asks about.
#   --runs N      how many times each wake is sent. Default 3.
#   --effort L    reasoning_effort, as the app sends it. Default low; '' omits.
#   --self-check  prove the URL join and the statistics offline. No network.
#   Reads AI_BUDDY_DIRECTOR_BASE_URL, _MODEL and _API_KEY, as probe-model.sh
#   does, and never prints the key. Needs curl and jq. Exit 2 is never having
#   asked, 1 is a call that did not answer, 0 is every call answered.
#
# Two limits before quoting the output. The thinking/answer split is whatever
# the server reports in `usage`, and one that reports none gets a dash; nothing
# is inferred, because this codebase does not tokenize (#606). And the prompt is
# a wake's shape, not a copy of crates/core/src/director/prompt.rs — it carries
# the reply contract, the part that sets an answer's length. The Responses path
# is uncovered: xAI alone goes there, for no extra evidence.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

info() { printf '\033[36m[INFO]\033[0m %s\n' "$*"; }
fail() { printf '\033[31m[FAIL]\033[0m %s\n' "$*"; }

# The same join completions_url does: never double /v1, never re-append a path
# that is already there.
join_url() {
  local url="${1%/}"
  case "$url" in
    */chat/completions) printf '%s\n' "$url" ;;
    */v1) printf '%s\n' "$url/chat/completions" ;;
    *) printf '%s\n' "$url/v1/chat/completions" ;;
  esac
}

# min / median / max of one tab-separated column, skipping the -1 a server that
# reports no such number writes. #598 §6.1 quoted a median, so this has to be
# comparable to it.
spread() {
  local label="$1" column="$2" rows="$3" values
  values="$(cut -f"$column" "$rows" | grep -v '^-1$' | sort -n)"
  if [ -z "$values" ]; then
    printf '  %-16s not reported by this server\n' "$label"
    return
  fi
  printf '%s\n' "$values" | awk -v label="$label" '
    {v[NR] = $1}
    END {
      mid = (NR % 2) ? v[int(NR / 2) + 1] : int((v[NR / 2] + v[NR / 2 + 1]) / 2)
      printf "  %-16s min %d  median %d  max %d  (n=%d)\n", label, v[1], mid, v[NR], NR
    }'
}

self_check() {
  local bad=0 got want rows
  while read -r base want; do
    got="$(join_url "$base")"
    if [ "$got" != "$want" ]; then
      fail "join_url $base gave $got, wanted $want"
      bad=1
    fi
  done << 'EOF'
http://localhost:11434 http://localhost:11434/v1/chat/completions
http://localhost:11434/ http://localhost:11434/v1/chat/completions
https://api.openai.com/v1 https://api.openai.com/v1/chat/completions
https://h/v1/chat/completions https://h/v1/chat/completions
EOF

  rows="$(mktemp)"
  printf '%s\n' $'10\t100\tstop' $'-1\t300\tlength' $'30\t200\tstop' > "$rows"
  got="$(spread total 2 "$rows")"
  want="  total            min 100  median 200  max 300  (n=3)"
  [ "$got" = "$want" ] || {
    fail "spread over totals gave [$got], wanted [$want]"
    bad=1
  }
  got="$(spread thinking 1 "$rows")"
  want="  thinking         min 10  median 20  max 30  (n=2)"
  [ "$got" = "$want" ] || {
    fail "spread skipping unreported gave [$got], wanted [$want]"
    bad=1
  }
  rm -f "$rows"

  [ "$bad" -eq 0 ] && info "self-check passed"
  return "$bad"
}

cap=512
runs=3
effort="low"

while [ $# -gt 0 ]; do
  case "$1" in
    --cap)
      cap="${2:-}"
      shift 2
      ;;
    --runs)
      runs="${2:-}"
      shift 2
      ;;
    --effort)
      effort="${2:-}"
      shift 2
      ;;
    --self-check)
      self_check
      exit $?
      ;;
    -h | --help)
      awk 'NR > 1 && /^#/ {sub(/^# ?/, ""); print; next} NR > 1 {exit}' \
        "${BASH_SOURCE[0]}"
      exit 0
      ;;
    *)
      fail "unknown argument $1; --help lists them"
      exit 2
      ;;
  esac
done

command -v jq > /dev/null || {
  fail "jq is not on PATH; brew install jq"
  exit 2
}

base="${AI_BUDDY_DIRECTOR_BASE_URL:-https://api.openai.com}"
model="${AI_BUDDY_DIRECTOR_MODEL:-gpt-4o-mini}"
key="${AI_BUDDY_DIRECTOR_API_KEY:-}"

case "$base" in
  *api.x.ai* | */responses)
    fail "the Responses path is out of scope; point this at a chat-completions host"
    exit 2
    ;;
esac

url="$(join_url "$base")"
auth=()
[ -n "$key" ] && auth=(-H "Authorization: Bearer $key")

# The opening turn's shape: app instructions, then the labelled moment. A later
# wake sends only the moment and asks for the same two lines.
wake_prompt() {
  local happened="$1" said="$2"
  printf '%s\n' \
    "You may propose one of these behaviors: idle, walk, sit, sleep, wave, jump" \
    "" \
    "Reply with the behavior name on the first line." \
    "An optional spoken line may follow on the next line." \
    "Propose nothing else." \
    "" \
    "Speak in this character's voice, always in character: never mention being" \
    "a model or an assistant. A spoken line fits a small speech bubble: five" \
    "short sentences at the most." \
    "" \
    "what just happened: $happened" \
    "recent: walk, idle" \
    "time: 3:40 pm" \
    "state: idle" \
    "standing on: the Dock" \
    "open: Terminal is the frontmost window"
  [ -n "$said" ] && printf 'they said: %s\n' "$said"
  return 0
}

rows="$(mktemp)"
trap 'rm -f "$rows"' EXIT
failures=0

call() {
  local wake="$1" run="$2" prompt="$3"
  local body answer parsed think total finish reply

  body="$(jq -n --arg m "$model" --arg p "$prompt" --argjson cap "$cap" \
    --arg effort "$effort" \
    '{model: $m, max_tokens: $cap, stream: false,
      messages: [{role: "user", content: $p}]}
     + (if $effort == "" then {} else {reasoning_effort: $effort} end)')"

  answer="$(curl -sS --max-time 120 "$url" \
    -H 'Content-Type: application/json' "${auth[@]}" \
    --data-binary "$body" 2> /dev/null)"

  parsed="$(printf '%s' "$answer" | jq -r '
    if .choices then
      [(.usage.completion_tokens_details.reasoning_tokens // -1),
       (.usage.completion_tokens // -1),
       (.choices[0].finish_reason // "none")] | @tsv
    else
      "err\terr\t" + ((.error.message // "no choices in the body")
                      | gsub("\\s+"; " ") | .[0:60])
    end' 2> /dev/null)"

  if [ -z "$parsed" ]; then
    printf '%-8s %-4s %8s %8s %8s  %s\n' "$wake" "$run" - - - "no JSON answer"
    failures=$((failures + 1))
    return
  fi

  IFS=$'\t' read -r think total finish <<< "$parsed"
  if [ "$think" = "err" ]; then
    printf '%-8s %-4s %8s %8s %8s  %s\n' "$wake" "$run" - - - "$finish"
    failures=$((failures + 1))
    return
  fi

  reply=-1
  [ "$total" -ge 0 ] && [ "$think" -ge 0 ] && reply=$((total - think))
  printf '%-8s %-4s %8s %8s %8s  %s\n' "$wake" "$run" \
    "$([ "$think" -ge 0 ] && echo "$think" || echo -)" \
    "$([ "$reply" -ge 0 ] && echo "$reply" || echo -)" \
    "$([ "$total" -ge 0 ] && echo "$total" || echo -)" \
    "$finish"
  printf '%s\t%s\t%s\n' "$think" "$total" "$finish" >> "$rows"
}

info "$model at $url"
info "cap $cap, $runs run(s) per wake, reasoning_effort ${effort:-(omitted)}"
[ -n "$key" ] || info "no API key set; this only works against a local server"
printf '\n%-8s %-4s %8s %8s %8s  %s\n' wake run think answer total finish

for run in $(seq 1 "$runs"); do
  call ambient "$run" "$(wake_prompt "time passed" "")"
  call poke "$run" "$(wake_prompt "poked" "")"
  call chat "$run" "$(wake_prompt "spoken to" "what does this function do?")"
done

answered="$(wc -l < "$rows" | tr -d ' ')"
cut_off="$(cut -f3 "$rows" | grep -cE '^(length|max_tokens)$')"
printf '\n'
info "$answered call(s) answered, $cut_off truncated, $failures failed"
spread "thinking tokens" 1 "$rows"
spread "total tokens" 2 "$rows"
printf '\n'
info "A median total at or near $cap means the cap is what stopped the turn."
info "One run does not move LOCAL_MAX_TOKENS or HOSTED_MAX_TOKENS. #606 §D."

[ "$failures" -eq 0 ] || exit 1
