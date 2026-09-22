#!/usr/bin/env bash
# Keep the vendored Matt Pocock engineering skills in `.agents/skills/` current.
#
#   scripts/sync-mattpocock.sh            regenerate the index
#   scripts/sync-mattpocock.sh --fetch    re-vendor from upstream first
#
# The sibling of `scripts/sync-pstack.sh`, and deliberately the same shape.
# `.agents/skills/` is one flat directory shared by two vendored sets and this
# repository's own skills, so each sync has to know exactly which names are its
# own. `UPSTREAM.json` records them, and gates deletion, so a skill this
# repository owns survives an upstream that has never heard of it.
#
# This script does not create the `.claude` and `.cursor` symlinks.
# `scripts/sync-pstack.sh` does, they are committed, and
# `tests/skills-layout.test.js` fails when one breaks. One owner is enough.

set -euo pipefail

REPO=https://github.com/mattpocock/skills
BRANCH=main
SUBPATH=skills/engineering

root=$(git rev-parse --show-toplevel)
cd "$root"

SKILLS=.agents/skills
META=.agents/mattpocock/UPSTREAM.json
PSTACK_META=.agents/pstack/UPSTREAM.json

# Names upstream ships that this sync deliberately does not vendor, one
# `name|reason` per line. The same list drives the skip and the `excluded`
# block written into the lock file, so the behaviour and the record of it
# cannot drift apart. Keep reasons free of double quotes; they are written
# into JSON verbatim.
EXCLUDED='ask-matt|Policy. A router over the whole upstream set, including the four groups this repository does not vendor, so most of what it offers is not here. docs/agents/picking-work.md decides what to work on.
setup-matt-pocock-skills|Already run here, and a re-run only does damage. It scaffolds docs/agents/issue-tracker.md, docs/agents/triage-labels.md and docs/agents/domain.md plus the Agent skills block in AGENTS.md; all four exist and have been hand-edited since. Its step 4 also prefers CLAUDE.md, which here only points at AGENTS.md, so a re-run would write a second Agent skills block into the file that does nothing else. See docs/agents/vendored-skills.md.'

excluded_names() {
  printf '%s\n' "$EXCLUDED" | cut -d'|' -f1 | sort
}

# The `excluded` object body for the lock file, one JSON member per line.
excluded_json() {
  local first=1 name reason
  while IFS='|' read -r name reason; do
    [ -n "$name" ] || continue
    [ "$first" -eq 1 ] || echo ','
    printf '    "%s": "%s"' "$name" "$reason"
    first=0
  done <<< "$EXCLUDED"
  echo
}

# The owned names of either lock file, one per line. A missing file owns
# nothing, which is the bootstrap case: the very first `--fetch` has no lock
# file yet, and owning nothing is exactly right — it may delete nothing.
owned_names() {
  [ -f "$1" ] || return 0
  awk '
    /"skills": \[/ { inlist = 1; next }
    inlist && /^[[:space:]]*\]/ { inlist = 0 }
    inlist { gsub(/[ \t",]/, ""); if ($0 != "") print }
  ' "$1"
}

mine() { owned_names "$META"; }

# Names this sync must never write and never delete: everything pstack owns,
# plus every directory in `.agents/skills` that neither lock file claims, which
# is by definition one this repository wrote itself.
#
# Computed rather than hand-listed. A hand-listed exclusion only covers the
# collision someone already noticed; this covers the one upstream adds next
# year. The collision that exists today is `tdd`, and it is settled the other
# way: `scripts/sync-pstack.sh` excludes it, so pstack no longer claims the
# name and nothing here reserves it.
reserved_names() {
  local pstack_owned
  pstack_owned=$(owned_names "$PSTACK_META" | sort)
  {
    echo "$pstack_owned"
    comm -23 \
      <(find "$SKILLS" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort) \
      <(sort -u <(echo "$pstack_owned") <(mine | sort))
  } | sort -u
}

if [ "${1:-}" = "--fetch" ]; then
  tmp=$(mktemp -d)
  # shellcheck disable=SC2064  # expand tmp now, not at trap time
  trap "rm -rf '$tmp'" EXIT

  git clone --quiet --filter=blob:none --sparse --depth 1 -b "$BRANCH" \
    "$REPO" "$tmp/skills"
  git -C "$tmp/skills" sparse-checkout set --skip-checks "$SUBPATH" LICENSE
  up=$tmp/skills/$SUBPATH

  before=$(mine | sort)
  # What upstream ships is what gets vendored: this listing drives the rsync
  # below, and the lock file's list is a record of the last sync rather than a
  # whitelist gating this one. That is why `EXCLUDED` has to exist — dropping a
  # name from the lock file would not keep it out, it would just be rsynced back
  # over ours on the next fetch and re-listed.
  upstream=$(find "$up" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort)
  reserved=$(reserved_names)

  # Everything upstream ships, less the policy exclusion, less any name that is
  # already somebody else's. Announce a skipped collision: silence here would
  # mean a skill quietly missing from the index with no reason recorded.
  candidates=$(comm -23 <(echo "$upstream") <(excluded_names))
  after=$(comm -23 <(echo "$candidates") <(echo "$reserved"))
  comm -12 <(echo "$candidates") <(echo "$reserved") | while read -r clash; do
    [ -n "$clash" ] || continue
    echo "skipping $clash (name already owned by pstack or by this repository)"
  done

  # Dropped upstream. Only names this script put there are eligible, so neither
  # a pstack skill nor a repo-owned one can be removed by an upstream deletion.
  comm -23 <(echo "$before") <(echo "$after") | while read -r gone; do
    [ -n "$gone" ] || continue
    echo "removing $gone (gone from upstream, excluded, or now reserved)"
    rm -rf "${SKILLS:?}/$gone"
  done

  while read -r name; do
    [ -n "$name" ] || continue
    rsync -a --delete "$up/$name/" "$SKILLS/$name/"
  done <<< "$after"

  cp "$tmp/skills/LICENSE" .agents/mattpocock/LICENSE

  sha=$(git -C "$tmp/skills" rev-parse HEAD)

  # Written without jq, like the pstack sync: the fields wanted are one
  # `git rev-parse` and a directory listing, and this has to run on a bare
  # macOS checkout too.
  {
    echo '{'
    echo '  "_warning": "Generated by scripts/sync-mattpocock.sh and .github/workflows/mattpocock-skills-sync.yml. Do not hand-edit.",'
    echo "  \"repository\": \"$REPO\","
    echo "  \"subpath\": \"$SUBPATH\","
    echo "  \"branch\": \"$BRANCH\","
    echo "  \"commit\": \"$sha\","
    echo '  "license": "MIT",'
    echo "  \"synced\": \"$(date -u +%Y-%m-%d)\","
    echo '  "vendored_into": ".agents/skills",'
    echo '  "not_vendored": ["skills/deprecated", "skills/in-progress", "skills/misc", "skills/productivity"],'
    echo '  "_excluded": "Names upstream ships under the subpath that this sync deliberately does not vendor. Generated from EXCLUDED in scripts/sync-mattpocock.sh.",'
    echo '  "excluded": {'
    excluded_json
    echo '  },'
    echo '  "_skills": "The skill directories under .agents/skills this file owns. Anything else there belongs to pstack or to this repository and is never touched by this sync.",'
    echo '  "skills": ['
    echo "$after" | sed '$ !s/.*/    "&",/; $ s/.*/    "&"/'
    echo '  ]'
    echo '}'
  } > "$META"
fi

# The index. An agent cannot invoke a skill it cannot see, and reading every
# SKILL.md on the chance one fits is what this file exists to avoid. One line
# each, from the frontmatter `description`.
{
  echo "# Matt Pocock engineering skills"
  echo
  echo "Generated by \`scripts/sync-mattpocock.sh\`. Do not hand-edit."
  echo
  echo "One line per vendored skill, taken from its frontmatter. They live in"
  echo "\`.agents/skills/\`, alongside the pstack skills and the ones this"
  echo "repository owns, neither of which is listed here. None of them is on by"
  echo "default. Apply one when the user names it, by reading"
  echo "\`.agents/skills/<name>/SKILL.md\` and following it."
  echo
  while read -r name; do
    [ -n "$name" ] || continue
    # Some descriptions are quoted on one line, others are bare, and a folded
    # block scalar has to be joined back up.
    desc=$(awk '
      /^description:/ {
        sub(/^description:[ \t]*/, "")
        if ($0 ~ /^[>|]/) {
          out = ""
          while ((getline line) > 0) {
            if (line !~ /^[ \t]+/) break
            sub(/^[ \t]+/, "", line)
            out = (out == "" ? line : out " " line)
          }
          print out
        } else {
          print
        }
        exit
      }
    ' "$SKILLS/$name/SKILL.md" | sed 's/^"//; s/"$//; s/\\"/"/g')
    echo "- \`$name\` — $desc"
  done < <(mine)
} > .agents/mattpocock/SKILLS.md

echo "$(mine | grep -c .) vendored Matt Pocock skills in $SKILLS ($(find "$SKILLS" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ') total)"
