// A real window at a chosen rectangle, which then steps down the screen.
//
// The frame loop can only be verified against a desktop, and the interesting
// half of it is Perches: the sprite has to land on a window's top edge, follow
// that edge when the window moves, and fall when the window goes away. None of
// the user's own windows can be used for that — moving another application's
// window needs an Accessibility grant, which this project asks for nowhere —
// but opening one of our own is free, and the window server reports it to
// WindowSource exactly like any other window.
//
// Each event prints one JSON line: when it happened, in Unix milliseconds, and
// the bounds the window server actually settled on, in the top-left-origin
// points WindowSource reads. Both matter. A titled window's frame is taller
// than the content rectangle it was asked for and AppKit may constrain where it
// lands, so what was requested is not evidence; what the server reports is. The
// timestamp is what lets verify-overlay.sh measure how long the app took to
// notice, against the app's own frame trace in the same clock.
//
// It terminates on its own, so an interrupted run cannot leave a stray window
// on the user's screen.
//
// Usage: swift scripts/perch-window.swift x y width height

import AppKit

/// How far down the screen each step moves the window, and how long between
/// steps. Slow enough that the sprite is settled before the next one, and
/// repeated so the check can use whichever step happened once the sprite was
/// already perched, rather than racing the app's startup.
let stepPoints = 80.0
let stepInterval = 5.0
let steps = 3

/// A backstop, not a schedule: the script kills this window when it is done
/// with it, and this is what happens if the script never gets the chance.
let quitAfter = 45.0

let args = CommandLine.arguments
guard args.count == 5, let x = Double(args[1]), let y = Double(args[2]),
    let width = Double(args[3]), let height = Double(args[4])
else {
    FileHandle.standardError.write(
        Data("usage: perch-window.swift x y width height\n".utf8))
    exit(2)
}

let app = NSApplication.shared
// No Dock tile, no switcher entry, no stolen focus: this is a prop, not an app.
app.setActivationPolicy(.accessory)

let mainHeight = CGDisplayBounds(CGMainDisplayID()).height

/// AppKit places windows in bottom-left-origin points on the main display;
/// WindowSource and this script's arguments are in top-left-origin points.
func appKitRect(top: Double) -> NSRect {
    NSRect(x: x, y: mainHeight - (top + height), width: width, height: height)
}

let window = NSWindow(
    contentRect: appKitRect(top: y), styleMask: [.titled], backing: .buffered, defer: false)
window.title = "ai-buddy perch"
// Again as a frame, because a titled window's frame is its content rectangle
// plus a title bar and every step below sets the frame. Setting it both ways
// would make the first rectangle the odd one out.
window.setFrame(appKitRect(top: y), display: true)
window.orderFrontRegardless()

/// One line per event: when it happened, and what the window server says the
/// window's bounds are. `at` is passed in rather than read here, because the
/// server takes a moment to catch up with a move and the interesting timestamp
/// is the move, not the reading.
func report(at: Date) {
    var line: [String: Double] = ["at_ms": at.timeIntervalSince1970 * 1000]
    if let list = CGWindowListCopyWindowInfo(
        .optionIncludingWindow, CGWindowID(window.windowNumber)) as? [[String: Any]],
        let bounds = list.first?[kCGWindowBounds as String] as? [String: Any]
    {
        line["x"] = bounds["X"] as? Double ?? 0
        line["y"] = bounds["Y"] as? Double ?? 0
        line["w"] = bounds["Width"] as? Double ?? 0
        line["h"] = bounds["Height"] as? Double ?? 0
    }
    let data = try! JSONSerialization.data(withJSONObject: line)
    print(String(data: data, encoding: .utf8)!)
    fflush(stdout)
}

/// How long to let the window server catch up before believing what it reports.
let settle = 0.3

RunLoop.current.run(until: Date().addingTimeInterval(settle))
report(at: Date())

for step in 1...steps {
    DispatchQueue.main.asyncAfter(deadline: .now() + stepInterval * Double(step)) {
        let moved = Date()
        window.setFrame(appKitRect(top: y + stepPoints * Double(step)), display: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + settle) { report(at: moved) }
    }
}

DispatchQueue.main.asyncAfter(deadline: .now() + quitAfter) { app.terminate(nil) }
app.run()
