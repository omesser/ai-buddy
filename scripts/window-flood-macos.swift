// LOAD GENERATOR. Floods the main display with `count` real, titled windows,
// cascaded over whatever is on screen, so a bench can measure what a large
// window list costs. It covers the screen for as long as it runs.
//
// Refuses to start unless AI_BUDDY_BENCH_GREEN_LIGHT=1, the switch the bench
// scripts already use, because the person at the machine has to agree to lose
// the screen first. Capped at 300 windows so a typo cannot bury the display.
//
// The windows are 30% opaque, ignore the mouse, never take focus, and quit
// after quit-after-secs (default 180), so an interrupted run leaves none behind.
// Prints one JSON line: how many it opened, how many of those the window server
// lists on screen, and the on-screen total.
// Usage: AI_BUDDY_BENCH_GREEN_LIGHT=1 swift scripts/window-flood-macos.swift count [quit-after-secs]

import AppKit

guard ProcessInfo.processInfo.environment["AI_BUDDY_BENCH_GREEN_LIGHT"] == "1" else {
    FileHandle.standardError.write(
        Data(
            "window-flood-macos.swift covers the screen with windows. Set AI_BUDDY_BENCH_GREEN_LIGHT=1 once the person at the machine has agreed.\n"
                .utf8))
    exit(2)
}

let maxWindows = 300
let args = CommandLine.arguments.dropFirst()
guard let count = Int(args.first ?? ""), (1...maxWindows).contains(count) else {
    FileHandle.standardError.write(
        Data("usage: window-flood-macos.swift count(1-\(maxWindows)) [quit-after-secs]\n".utf8))
    exit(2)
}
let quitAfter = Double(args.dropFirst().first ?? "") ?? 180.0

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let bounds = CGDisplayBounds(CGMainDisplayID())
let size = NSSize(width: 160, height: 100)
// A cascade that wraps, so every window is on screen whatever the count.
let columns = max(1, Int((bounds.width - size.width) / 24))
let rows = max(1, Int((bounds.height - size.height) / 24))
var windows: [NSWindow] = []
for i in 0..<count {
    let x = 24.0 * Double(i % columns)
    let y = 24.0 * Double((i / columns) % rows)
    let window = NSWindow(
        contentRect: NSRect(x: x, y: y, width: size.width, height: size.height),
        styleMask: [.titled], backing: .buffered, defer: false)
    window.title = "ai-buddy flood \(i)"
    window.alphaValue = 0.3
    window.ignoresMouseEvents = true
    window.level = .normal
    window.orderFrontRegardless()
    windows.append(window)
}

RunLoop.current.run(until: Date().addingTimeInterval(0.5))
let ours = Set(windows.map { $0.windowNumber })
let onScreen =
    (CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]]) ?? []
let listed = onScreen.filter { ours.contains($0[kCGWindowNumber as String] as? Int ?? -1) }.count
let line: [String: Double] = [
    "at_ms": Date().timeIntervalSince1970 * 1000, "opened": Double(count),
    "listed": Double(listed), "on_screen": Double(onScreen.count),
]
print(String(data: try! JSONSerialization.data(withJSONObject: line), encoding: .utf8)!)
fflush(stdout)

DispatchQueue.main.asyncAfter(deadline: .now() + quitAfter) { app.terminate(nil) }
app.run()
