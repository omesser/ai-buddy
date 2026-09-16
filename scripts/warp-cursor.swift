// Places the cursor at a point in top-left-origin points, and prints where it
// ended up. The hit-test check needs the cursor and the sprite in one place,
// and the sprite's position belongs to the Engine, so the cursor is what moves.

// Needs no permission and posts no event: CGWarpMouseCursorPosition moves the
// pointer directly. Warping is this harness's business, not the app's (ADR-0003).
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
