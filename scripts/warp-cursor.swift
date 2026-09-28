// Places the cursor at a top-left-origin point and prints where it landed. Posts
// no event. Warping is the harness's business, not the app's (ADR-0003).
// Usage: swift scripts/warp-cursor.swift x y

import AppKit

let args = CommandLine.arguments
guard args.count == 3, let x = Double(args[1]), let y = Double(args[2]) else {
    FileHandle.standardError.write(Data("usage: warp-cursor.swift x y\n".utf8))
    exit(2)
}

CGWarpMouseCursorPosition(CGPoint(x: x, y: y))

// Read the cursor back rather than trusting the warp: the window server clamps
// to the displays it has, so a point off every screen quietly lands elsewhere
// and the caller has to be able to tell.
let mainHeight = CGDisplayBounds(CGMainDisplayID()).height
let landed = NSEvent.mouseLocation
print(String(format: "%.0f %.0f", landed.x, mainHeight - landed.y))
