#!/usr/bin/env bash
# Minimal xterm → xfce4-terminal shim for verify-overlay-x11.sh perch props.
# Only the flags that script uses: -geometry WxH+X+Y -title NAME -e CMD...
set -euo pipefail
geom="" title="" cmd=()
while [ $# -gt 0 ]; do
  case "$1" in
    -geometry)
      geom="$2"
      shift 2
      ;;
    -title)
      title="$2"
      shift 2
      ;;
    -e)
      shift
      cmd=("$@")
      break
      ;;
    *) shift ;;
  esac
done
args=()
[ -n "$geom" ] && args+=(--geometry="$geom")
[ -n "$title" ] && args+=(--title="$title")
if [ ${#cmd[@]} -gt 0 ]; then
  # xfce4-terminal wants one command string
  joined="${cmd[*]}"
  args+=(-e "$joined")
fi
exec xfce4-terminal "${args[@]}"
