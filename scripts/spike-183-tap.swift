// The #183 Stage 1 probe: one mouse-only, listen-only session tap, created
// once, and the four facts around it printed as key=value lines. Built into
// a throwaway .app by spike-183-input-monitoring.sh so TCC attributes the
// tap to that bundle and not to the terminal.
//
// Usage: spike-183-tap <full|no-motion>
//   full       the six types input_events.rs MOUSE_EVENTS holds
//   no-motion  the same without mouseMoved

import CoreGraphics
import Foundation

let variant = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "full"
var types: [CGEventType] = [
    .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .leftMouseDragged,
]
if variant == "full" { types.append(.mouseMoved) }
let mask = types.reduce(CGEventMask(0)) { $0 | (CGEventMask(1) << CGEventMask($1.rawValue)) }

print("pid=\(getpid())")
print("variant=\(variant)")
print("mask=0x\(String(mask, radix: 16))")
print("preflight_before=\(CGPreflightListenEventAccess())")

let port = CGEvent.tapCreate(
    tap: .cgSessionEventTap,
    place: .headInsertEventTap,
    options: .listenOnly,
    eventsOfInterest: mask,
    callback: { _, _, event, _ in Unmanaged.passUnretained(event) },
    userInfo: nil
)
print("tap_created=\(port != nil)")
if let port { print("tap_enabled=\(CGEvent.tapIsEnabled(tap: port))") }
print("preflight_after=\(CGPreflightListenEventAccess())")
fflush(stdout)

// Long enough for tccd to draw a prompt and the harness to read it.
Thread.sleep(forTimeInterval: 4)
