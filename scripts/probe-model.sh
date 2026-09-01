#!/usr/bin/env bash
#
# Hit the Director Completer with the same env and the same HTTP client
# the overlay uses. Later this is also the seam for harness reachability
# (#16): one command, same credentials, no sprite.
#
# Never prints the key — only its length and last four.
#
# Usage: scripts/probe-model.sh
#   Reads AI_BUDDY_DIRECTOR_* from the environment. A local base URL needs no
#   key, so this is also the fastest check that a local server is up and
#   serving the model you configured.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

exec cargo run -q -p ai-buddy -- --probe-model
