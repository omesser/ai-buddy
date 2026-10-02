#!/usr/bin/env bash

set -euo pipefail

export GIT_TERMINAL_PROMPT=0

tag=""
tap=""
branch="main"
tap_branch=""
only="all"
pr_branch="homebrew-cask-bump"
cask="packaging/homebrew/Casks/fidget.rb"
semver='^v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'

usage() {
  echo "usage: scripts/publish-homebrew-cask.sh --tag <tag> [--tap <dir>] [--branch <name>] [--tap-branch <name>] [--only canonical|tap|all]" >&2
  exit 2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tag)
      [ "$#" -ge 2 ] || usage
      tag="$2"
      shift 2
      ;;
    --tap)
      [ "$#" -ge 2 ] || usage
      tap="$2"
      shift 2
      ;;
    --branch)
      [ "$#" -ge 2 ] || usage
      branch="$2"
      shift 2
      ;;
    --tap-branch)
      [ "$#" -ge 2 ] || usage
      tap_branch="$2"
      shift 2
      ;;
    --only)
      [ "$#" -ge 2 ] || usage
      only="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[ -n "$tag" ] || usage
[[ $tag =~ $semver ]] || {
  echo "tag '$tag' is not a release tag" >&2
  exit 2
}
[[ $tag == v* ]] || tag="v${tag}"
case "$only" in
  all | canonical | tap) ;;
  *)
    echo "only must be canonical, tap, or all" >&2
    exit 2
    ;;
esac
[ -n "$tap_branch" ] || tap_branch="$branch"
[[ $branch =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || {
  echo "branch '$branch' is not a branch name" >&2
  exit 2
}
[[ $tap_branch =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || {
  echo "branch '$tap_branch' is not a branch name" >&2
  exit 2
}

as_bot() {
  git -C "$1" config user.name 'github-actions[bot]'
  git -C "$1" config user.email '41898282+github-actions[bot]@users.noreply.github.com'
  git -C "$1" config commit.gpgsign false
}

require_worktree() {
  git -C "$1" rev-parse --is-inside-work-tree > /dev/null
  git -C "$1" remote get-url origin > /dev/null
}

publish_canonical() {
  require_worktree .
  [ -f "$cask" ] || {
    echo "missing $cask" >&2
    exit 1
  }
  as_bot .
  git add -- "$cask"
  if git diff --staged --quiet -- "$cask"; then
    echo "canonical_status=unchanged"
    return
  fi
  git commit -q -m "build: Bump the Homebrew cask to ${tag}" -- "$cask"
  if git push --quiet origin "HEAD:${branch}"; then
    echo "canonical_status=pushed"
    return
  fi
  local lease
  lease="$(git ls-remote origin "refs/heads/${pr_branch}" | cut -f1)"
  git push --quiet --force-with-lease="refs/heads/${pr_branch}:${lease}" \
    origin "HEAD:${pr_branch}"
  echo "canonical_status=pull-request"
  echo "canonical_branch=${pr_branch}"
}

publish_tap() {
  [ -n "$tap" ] || usage
  require_worktree "$tap"
  [ -f "$cask" ] || {
    echo "missing $cask" >&2
    exit 1
  }
  mkdir -p "${tap}/Casks"
  cp -- "$cask" "${tap}/Casks/fidget.rb"
  as_bot "$tap"
  git -C "$tap" add -- Casks/fidget.rb
  if git -C "$tap" diff --staged --quiet -- Casks/fidget.rb; then
    echo "tap_status=unchanged"
    return
  fi
  git -C "$tap" commit -q -m "fidget: ${tag}" -- Casks/fidget.rb
  git -C "$tap" push --quiet origin "HEAD:${tap_branch}"
  echo "tap_status=pushed"
}

case "$only" in
  canonical) publish_canonical ;;
  tap) publish_tap ;;
  all)
    publish_canonical
    publish_tap
    ;;
esac
