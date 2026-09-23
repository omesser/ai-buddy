// A window that covers the whole main display, so `fullscreen_frontmost` in
// crates/core/src/visibility.rs decides a fullscreen app is frontmost and the
// Character fades. The rule reads rectangles, never pixels, so the window is
// nearly transparent and lets every click through: it hides the sprite without
// taking the desktop from whoever is using the machine while a bench runs.

// The window re-asserts its place at the front of ordinary windows while it
// lives: an accessory window is buried by anything that takes focus, and the
// rule takes the first window overlapping a display as the frontmost one.
// Prints one JSON line with the bounds the window server settled on.
// Usage: swift scripts/fullscreen-window.swift [quit-after-secs]

import AppKit

let quitAfter = Double(CommandLine.arguments.dropFirst().first ?? "") ?? 120.0
let reassertInterval = 0.05

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let bounds = CGDisplayBounds(CGMainDisplayID())
let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: bounds.width, height: bounds.height),
    styleMask: [.borderless], backing: .buffered, defer: false)
window.title = "ai-buddy fullscreen prop"
window.backgroundColor = .black
window.alphaValue = 0.02
window.ignoresMouseEvents = true
window.level = .normal
window.orderFrontRegardless()
Timer.scheduledTimer(withTimeInterval: reassertInterval, repeats: true) { _ in
    window.orderFrontRegardless()
}

RunLoop.current.run(until: Date().addingTimeInterval(0.3))
var line: [String: Double] = ["at_ms": Date().timeIntervalSince1970 * 1000]
let entry = (CGWindowListCopyWindowInfo(
    .optionIncludingWindow, CGWindowID(window.windowNumber)) as? [[String: Any]])?.first
if let settled = entry?[kCGWindowBounds as String] as? [String: Any] {
    line["x"] = settled["X"] as? Double ?? 0
    line["y"] = settled["Y"] as? Double ?? 0
    line["w"] = settled["Width"] as? Double ?? 0
    line["h"] = settled["Height"] as? Double ?? 0
}
print(String(data: try! JSONSerialization.data(withJSONObject: line), encoding: .utf8)!)
fflush(stdout)

DispatchQueue.main.asyncAfter(deadline: .now() + quitAfter) { app.terminate(nil) }
app.run()
