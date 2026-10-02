#!/usr/bin/env bash
# Scenario: control-click-menu (macOS)
# On screen: launches Fidget as BMO with a fixture Harness. The scenario finds
#   the sprite, sends a Control-click to its centre, waits for the menu to
#   appear, and verifies the menu contains expected items (Chat, Settings,
#   Quit). Fidget quits when the scenario ends.
# Input: one Control-click on the sprite (synthetic, via CGEvent).
# Duration: about 20 s, 1 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: a menu window appears after the Control-click, and the Accessibility
#   tree for that window contains menu items with expected titles.
#
# Usage: control-click-menu.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
  exit 2
fi
bin=${2:?usage: control-click-menu.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: control-click-menu.sh --go <fidget binary> <fidget test binary>}
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-control-click-menu-$(date +%Y%m%d-%H%M%S)"
tools="${TMPDIR:-/tmp}/fidget-scenario-tools"
mkdir -p "$out/home" "$tools"
log="$out/app.log"

fail() {
  echo "FAIL: $*" >&2
  echo "evidence in $out" >&2
  exit 1
}

[ -x "$tools/window-id" ] || swiftc -O "$root/scripts/scenarios/window-id.swift" -o "$tools/window-id"
[ -x "$tools/ax" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

harness="$test_bin harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 script=nop"
[ "$(wc -w <<< "$harness")" -eq 7 ] || fail "a path in the Harness line holds a space: $harness"

env HOME="$out/home" \
  FIDGET_HARNESS="$harness" \
  FIDGET_DIRECTOR_WAKE_SECS=3600 \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 \
  FIDGET_CHARACTER=bmo FIDGET_CHARACTERS="$root/characters" \
  "$bin" > "$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2> /dev/null || true; pkill -f "script=nop" || true' EXIT

wait_for() { # <seconds> <command...>
  local n=$(($1 * 4))
  shift
  until "$@" > /dev/null 2>&1; do
    kill -0 "$pid" 2> /dev/null || fail "Fidget exited; see $log"
    n=$((n - 1))
    [ "$n" -gt 0 ] || return 1
    sleep 0.25
  done
}

# Wait for Fidget to launch and the sprite to appear
wait_for 15 pgrep -f "FIDGET_CHARACTER=bmo" || fail "Fidget did not start; see $log"
sleep 2

# Find the sprite window bounds from the window server
sprite_bounds=$(swift - "$pid" <<'SWIFT'
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 2, let pid = Int(args[1]) else {
    FileHandle.standardError.write("usage: sprite-bounds.swift pid\n".data(using: .utf8)!)
    exit(2)
}
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows
where (window[kCGWindowOwnerPID as String] as? Int) == pid
    && (window[kCGWindowLayer as String] as? Int) == 0
{
    if let bounds = window[kCGWindowBounds as String] as? [String: Any],
       let x = bounds["X"] as? Double,
       let y = bounds["Y"] as? Double,
       let w = bounds["Width"] as? Double,
       let h = bounds["Height"] as? Double {
        print("\(Int(x)),\(Int(y)),\(Int(w)),\(Int(h))")
        exit(0)
    }
}
exit(1)
SWIFT
) || fail "no sprite window found"

IFS=',' read -r x y w h <<< "$sprite_bounds"
cx=$((x + w / 2))
cy=$((y + h / 2))
echo "ok: sprite at ($x,$y) size ${w}x${h}, centre ($cx,$cy)"

# Send Control-click to the sprite centre
swift - "$cx" "$cy" <<'SWIFT' || fail "Control-click failed"
import AppKit

let args = CommandLine.arguments
guard args.count >= 3, let x = Double(args[1]), let y = Double(args[2]) else {
    FileHandle.standardError.write(Data("usage: control-click.swift x y\n".utf8))
    exit(2)
}

let point = CGPoint(x: x, y: y)

guard let source = CGEventSource(stateID: .combinedSessionState),
      let down = CGEvent(mouseEventSource: source, mouseType: .leftMouseDown,
                         mouseCursorPosition: point, mouseButton: .left),
      let up = CGEvent(mouseEventSource: source, mouseType: .leftMouseUp,
                       mouseCursorPosition: point, mouseButton: .left)
else {
    FileHandle.standardError.write(Data("could not create CGEvent\n".utf8))
    exit(1)
}

down.flags = .maskControl
up.flags = .maskControl
down.post(tap: .cghidEventTap)
usleep(60_000)
up.post(tap: .cghidEventTap)
SWIFT

echo "ok: sent Control-click to ($cx,$cy)"

# Wait for a menu-layer window to appear
menu_count_before=$(swift - "$pid" <<'SWIFT'
import CoreGraphics
let args = CommandLine.arguments
guard args.count >= 2, let pid = Int(args[1]) else { exit(2) }
let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
let windows = (info as? [[String: AnyObject]]) ?? []
let menus = windows.filter {
    ($0[kCGWindowOwnerPID as String] as? Int) == pid
        && (($0[kCGWindowLayer as String] as? Int) ?? 0) >= 100
}
print(menus.count)
SWIFT
)

wait_for 5 swift - "$pid" "$menu_count_before" <<'SWIFT' || fail "no menu window appeared after Control-click"
import CoreGraphics
let args = CommandLine.arguments
guard args.count >= 3, let pid = Int(args[1]), let before = Int(args[2]) else { exit(2) }
let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
let windows = (info as? [[String: AnyObject]]) ?? []
let menus = windows.filter {
    ($0[kCGWindowOwnerPID as String] as? Int) == pid
        && (($0[kCGWindowLayer as String] as? Int) ?? 0) >= 100
}
exit(menus.count > before ? 0 : 1)
SWIFT

echo "ok: menu window appeared"

# Dump the menu via Accessibility
"$tools/ax" menus "$pid" > "$out/menu-count.txt" || fail "ax menus failed"
menu_count=$(cat "$out/menu-count.txt")
[ "$menu_count" -gt 0 ] || fail "no menu windows found via AX"
echo "ok: $menu_count menu window(s) via AX"

# Dump the first menu's items
menu_dump="$out/menu.ax.txt"
swift - "$pid" > "$menu_dump" <<'SWIFT' || fail "could not dump menu"
import AppKit
import ApplicationServices
import Foundation

let args = CommandLine.arguments
guard args.count >= 2, let pid = pid_t(args[1]) else { exit(2) }

guard AXIsProcessTrusted() else {
    FileHandle.standardError.write("no Accessibility grant\n".data(using: .utf8)!)
    exit(2)
}

let app = AXUIElementCreateApplication(pid)
func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
        return nil
    }
    return value
}

func string(_ element: AXUIElement, _ name: String) -> String? {
    attribute(element, name) as? String
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
}

func walk(_ element: AXUIElement, depth: Int) {
    guard depth < 20 else { return }
    let role = string(element, kAXRoleAttribute) ?? "?"
    let title = string(element, kAXTitleAttribute) ?? ""
    if !title.isEmpty {
        print("\(role)|\(title)")
    }
    for child in children(element) {
        walk(child, depth: depth + 1)
    }
}

let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
for window in windows {
    let role = string(window, kAXRoleAttribute) ?? ""
    if role == "AXMenu" || role == "AXWindow" {
        walk(window, depth: 0)
        break
    }
}
SWIFT

cat "$menu_dump"

# Assert expected menu items are present
for item in "Chat" "Settings" "Quit"; do
  grep -q "$item" "$menu_dump" || fail "menu item '$item' not found in AX dump; see $menu_dump"
  echo "ok: menu contains '$item'"
done

echo "PASS: Control-click opened the menu with expected items; evidence in $out"
