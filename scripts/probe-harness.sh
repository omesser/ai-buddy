#!/usr/bin/env bash
# Attach the configured Harness and run one ACP turn, with no sprite: spawn the
# Harness, print what `initialize` advertised, and report whether the reply
# parsed as a Behavior proposal. Never prints a credential.
# Usage: AI_BUDDY_HARNESS=hermes scripts/probe-harness.sh
#   Reads AI_BUDDY_HARNESS and AI_BUDDY_MCP_BIN, the same two `cargo run` reads.
#   Exit 2 is never having asked (nothing configured, no binary, not signed
#   in), 1 is asked and not answered, 0 is end_turn.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

exec cargo run -q -p ai-buddy -- --probe-harness
