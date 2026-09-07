#!/usr/bin/env bash
#
# Attach the configured Harness and run one ACP turn, with no sprite. The
# sibling of probe-model.sh, one hop further out: where that one posts to a
# chat-completions host, this one spawns the Harness, prints what `initialize`
# advertised, and reports whether the reply parsed as a Behavior proposal.
#
# Never prints a credential. A Harness that is not signed in comes back as the
# command to run in your own terminal — run it there, not here.
#
# Usage: AI_BUDDY_HARNESS=hermes scripts/probe-harness.sh
#   Reads AI_BUDDY_HARNESS and AI_BUDDY_MCP_BIN from the environment, the same
#   two `cargo run` reads. The exit code splits on the two phases it prints:
#   2 is never having asked (nothing configured, no binary, not signed in),
#   1 is asked and not answered, 0 is end_turn.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

exec cargo run -q -p ai-buddy -- --probe-harness
