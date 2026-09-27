// LOAD GENERATOR. Floods the main display with `count` real, titled windows,
// cascaded over whatever is on screen, so a bench can measure what a large
// window list costs. It covers the screen for as long as it runs.
//
// Refuses to start unless FIDGET_BENCH_GREEN_LIGHT=1, the switch the bench
// scripts already use, because the person at the machine has to agree to lose
// the screen first. Capped at 300 windows so a typo cannot bury the display.
//
// The windows are 30% opaque, ignore the mouse, never take focus, and quit
// after quit-after-secs (default 180, 1-3600), so an interrupted run leaves
// none behind. Prints one JSON line: how many it opened, how many of those the
// window server lists on screen (after a brief settle wait), and the
// on-screen total.
// Usage: FIDGET_BENCH_GREEN_LIGHT=1 swift scripts/window-flood-macos.swift count [quit-after-secs]

import AppKit

guard ProcessInfo.processInfo.environment["FIDGET_BENCH_GREEN_LIGHT"] == "1" else {
    FileHandle.standardError.write(
        Data(
            "window-flood-macos.swift covers the screen with windows. Set FIDGET_BENCH_GREEN_LIGHT=1 once the person at the machine has agreed.\n"
                .utf8))
    exit(2)
}

let maxWindows = 300
let maxQuitAfterSecs = 3600.0
let usage =
    "usage: window-flood-macos.swift count(1-\(maxWindows)) [quit-after-secs(1-\(Int(maxQuitAfterSecs)))]\n"
let args = CommandLine.arguments.dropFirst()
guard let count = Int(args.first ?? ""), (1...maxWindows).contains(count) else {
    FileHandle.standardError.write(Data(usage.utf8))
    exit(2)
}
let quitAfter: Double
if let raw = args.dropFirst().first {
    guard let value = Double(raw), value.isFinite, (1...maxQuitAfterSecs).contains(value) else {
        FileHandle.standardError.write(Data(usage.utf8))
        exit(2)
    }
    quitAfter = value
} else {
    quitAfter = 180.0
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let bounds = CGDisplayBounds(CGMainDisplayID())
let size = NSSize(width: 160, height: 100)
// A cascade that wraps, so every window is on screen whatever the count.
// Past one screenful (count > columns*rows) windows stack on top of each
// other; that overlap is intentional, since a stacked window still appears
// in the on-screen list this bench measures.
let columns = max(1, Int((bounds.width - size.width) / 24))
let rows = max(1, Int((bounds.height - size.height) / 24))
var windows: [NSWindow] = []
for i in 0..<count {
    let x = 24.0 * Double(i % columns)
    let y = 24.0 * Double((i / columns) % rows)
    let window = NSWindow(
        contentRect: NSRect(x: x, y: y, width: size.width, height: size.height),
        styleMask: [.titled], backing: .buffered, defer: false)
    window.title = "fidget flood \(i)"
    window.alphaValue = 0.3
    window.ignoresMouseEvents = true
    window.level = .normal
    window.orderFrontRegardless()
    windows.append(window)
}

let ours = Set(windows.map { $0.windowNumber })
// Bounded settle wait: at most settleStep * settleSteps (2s), well inside the
// bench's 10s wait for a stdout line starting with `{`. Near 300 windows the
// window server can lag creation, so poll instead of a single fixed sleep;
// `listedDeadlineHit` says honestly whether the count still fell short.
let settleStep = 0.05
let settleSteps = 40
var onScreen: [[String: Any]] = []
var listed = 0
var listedDeadlineHit = true
for _ in 0..<settleSteps {
    RunLoop.current.run(until: Date().addingTimeInterval(settleStep))
    onScreen = (CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]]) ?? []
    listed = onScreen.filter { ours.contains($0[kCGWindowNumber as String] as? Int ?? -1) }.count
    if listed >= count {
        listedDeadlineHit = false
        break
    }
}
let line: [String: Any] = [
    "at_ms": Date().timeIntervalSince1970 * 1000, "opened": count,
    "listed": listed, "on_screen": onScreen.count, "listed_deadline_hit": listedDeadlineHit,
]
guard let data = try? JSONSerialization.data(withJSONObject: line),
    let text = String(data: data, encoding: .utf8)
else {
    FileHandle.standardError.write(Data("window-flood-macos.swift: failed to encode the result JSON\n".utf8))
    exit(1)
}
print(text)
fflush(stdout)

DispatchQueue.main.asyncAfter(deadline: .now() + quitAfter) { app.terminate(nil) }
app.run()
