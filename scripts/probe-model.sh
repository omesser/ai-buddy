#!/usr/bin/env bash
# Hit the Director Completer with the overlay's env and HTTP client; prints only
# the key's length and last four. Usage: scripts/probe-model.sh, reading
# FIDGET_DIRECTOR_*. With a local base URL the key is optional.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

exec cargo run -q -p fidget -- --probe-model
