#!/usr/bin/env bash
# Run the automated double-click-interval unit tests for the current OS.
#
# Prefer these unit tests in CI over a live desktop. Each platform pointer
# module asserts its reader against the native OS API independently; the
# public OnceLock wrapper is also covered in platform.rs.
#
# Usage: scripts/verify-double-click-interval.sh
# Exit non-zero on any failure.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$WORKSPACE_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

log_info() {
  echo -e "${GREEN}[INFO]${NC} $*"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $*"
}

fail() {
  log_error "$@"
  exit 1
}

log_info "core: Pointer injection"
cargo test -p ai-buddy-core --lib input::tests::double_click_interval_is_injected ||
  fail "core double_click_interval_is_injected failed"

log_info "platform: resolve/clamp/fallback + OnceLock cache"
cargo test -p ai-buddy --bin ai-buddy platform::tests::double_click_interval_ ||
  fail "platform resolve/cache tests failed"

case "$(uname -s)" in
  Linux)
    log_info "Linux/X11: gtk-double-click-time reader"
    cargo test -p ai-buddy --bin ai-buddy \
      platform::x11::pointer::tests::double_click_interval_ms_matches_gtk_settings ||
      fail "x11 pointer OS-reader test failed"
    ;;
  Darwin)
    log_info "macOS: NSEvent::doubleClickInterval reader"
    cargo test -p ai-buddy --bin ai-buddy \
      platform::macos::pointer::tests::double_click_interval_ms_matches_nsevent ||
      fail "macos pointer OS-reader test failed"
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT)
    log_info "Windows: GetDoubleClickTime reader"
    cargo test -p ai-buddy --bin ai-buddy \
      platform::windows::pointer::tests::double_click_interval_ms_matches_get_double_click_time ||
      fail "windows pointer OS-reader test failed"
    ;;
  *)
    log_info "unknown OS $(uname -s); skipped platform-native reader filter"
    ;;
esac

log_info "all double-click-interval unit tests passed"
