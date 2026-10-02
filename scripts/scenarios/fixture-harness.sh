#!/usr/bin/env bash
# The fixture ACP agent as a Harness launcher. Preflight runs `<launcher> --version`,
# which the Rust test binary rejects, and a refused launcher never gets a wake.
# Usage: fixture-harness.sh <fidget test binary> script=<name> [count=<file>]
set -euo pipefail
if [ "${1:-}" = --version ]; then
  echo "fixture-harness"
  exit 0
fi
bin=$1
shift
exec "$bin" harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 "$@"
