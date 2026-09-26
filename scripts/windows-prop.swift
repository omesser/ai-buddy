// N real windows on the main display, so a bench can grow the window list
// without opening anything of the operator's. They ignore the mouse, never
// take focus, and quit on their own, so an interrupted run leaves none behind.
// Prints one JSON line with how many of them the window server lists on screen.
// Usage: swift scripts/windows-prop.swift count [quit-after-secs]

import AppKit

let args = CommandLine.arguments.dropFirst()
guard let count = Int(args.first ?? ""), count > 0 else {
    FileHandle.standardError.write(Data("usage: windows-prop.swift count [quit-after-secs]\n".utf8))
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
    window.title = "ai-buddy prop \(i)"
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
