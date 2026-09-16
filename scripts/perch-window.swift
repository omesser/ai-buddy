// A real window at a chosen rectangle, which then steps or flings itself down
// the screen. The frame loop's Perches can only be verified against a desktop,
// and moving another app's window needs an Accessibility grant; ours is free.

// --fast covers the travel in one jump: a Perch that outruns the sprite leaves
// it behind to fall. Each event prints one JSON line: Unix ms, the bounds the
// window server settled on (what was asked for is not evidence), and depth.

// The window re-asserts its place at the front of its level while it lives: an
// accessory window is buried by anything that takes focus, and a buried prop is
// one the sprite falls through, so a landing check would assert nothing.

// An optional window level makes the prop desktop furniture (Dock 20, menu bar
// 24), the only way to check from a script that neither is a Perch. It quits on
// its own, so an interrupted run leaves no stray window.
// Usage: swift scripts/perch-window.swift [--fast] x y width height [level]

import AppKit

/// How far each step moves the window, and how long between steps. Slow enough
/// that the sprite is settled before the next, and repeated so the check can
/// use a step after the sprite was perched. The fling waits the same interval.
let stepPoints = 80.0
let stepInterval = 5.0
let steps = 3

/// The whole of that travel in one move: the app reads the window list about
/// ten times a second, and a burst of small moves can fall either side of a
/// read with neither half fast enough to be a yank.
let flingPoints = stepPoints * Double(steps)

/// How often the prop re-asserts its place at the front of its level. Faster
/// than the app's ~10Hz window poll, so a burial cannot survive a whole tick and
/// be read as one.
let reassertInterval = 0.05

/// A backstop, not a schedule: the script kills this window when it is done
/// with it, and this is what happens if the script never gets the chance.
let quitAfter = 45.0

var args = Array(CommandLine.arguments.dropFirst())
let fast = args.first == "--fast"
if fast { args.removeFirst() }
guard args.count == 4 || args.count == 5, let x = Double(args[0]), let y = Double(args[1]),
    let width = Double(args[2]), let height = Double(args[3])
else {
    FileHandle.standardError.write(
        Data("usage: perch-window.swift [--fast] x y width height [level]\n".utf8))
    exit(2)
}
let level = args.count == 5 ? Int(args[4]) ?? 0 : 0

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
window.level = NSWindow.Level(rawValue: level)
window.orderFrontRegardless()
Timer.scheduledTimer(withTimeInterval: reassertInterval, repeats: true) { _ in
    window.orderFrontRegardless()
}

/// One line per event. `at` is passed in rather than read here, because the
/// server takes a moment to catch up with a move and the interesting timestamp
/// is the move, not the reading.
func report(at: Date) {
    var line: [String: Double] = ["at_ms": at.timeIntervalSince1970 * 1000]
    let entry = (CGWindowListCopyWindowInfo(
        .optionIncludingWindow, CGWindowID(window.windowNumber)) as? [[String: Any]])?.first
    if let bounds = entry?[kCGWindowBounds as String] as? [String: Any] {
        line["x"] = bounds["X"] as? Double ?? 0
        line["y"] = bounds["Y"] as? Double ?? 0
        line["w"] = bounds["Width"] as? Double ?? 0
        line["h"] = bounds["Height"] as? Double ?? 0
    }
    // The level the window server settled on, not the one that was asked for:
    // what makes a prop furniture is where the server put it.
    if let layer = entry?[kCGWindowLayer as String] as? Int {
        line["layer"] = Double(layer)
    }
    // How many ordinary windows are in front of this one, 0 when frontmost.
    // Bounds alone cannot tell a Perch from an edge buried behind another
    // window, so a check that reads only the bounds asserts nothing.
    if let onScreen = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID)
        as? [[String: Any]]
    {
        let depth = onScreen
            .filter { $0[kCGWindowLayer as String] as? Int == 0 }
            .firstIndex { $0[kCGWindowNumber as String] as? Int == window.windowNumber }
        // Absent for a prop the list does not carry: an elevated one is at no
        // depth among ordinary windows, and the check that reads this is only
        // ever asked about the Perch.
        if let depth { line["depth"] = Double(depth) }
    }
    let data = try! JSONSerialization.data(withJSONObject: line)
    print(String(data: data, encoding: .utf8)!)
    fflush(stdout)
}

/// How long to let the window server catch up before believing what it reports.
let settle = 0.3

RunLoop.current.run(until: Date().addingTimeInterval(settle))
report(at: Date())

/// When each move happens, and where it puts the top edge. The two variants
/// differ in nothing else: same window, same travel, same reports.
let moves: [(after: Double, top: Double)] =
    fast
    ? [(stepInterval, y + flingPoints)]
    : (1...steps).map { (stepInterval * Double($0), y + stepPoints * Double($0)) }

for move in moves {
    DispatchQueue.main.asyncAfter(deadline: .now() + move.after) {
        let moved = Date()
        window.setFrame(appKitRect(top: move.top), display: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + settle) { report(at: moved) }
    }
}

DispatchQueue.main.asyncAfter(deadline: .now() + quitAfter) { app.terminate(nil) }
app.run()
