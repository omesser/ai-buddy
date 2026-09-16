// Posts one or two real left-button clicks at a point in top-left-origin
// points, warping the cursor there first. CGEventPost feeds the HID stream the
// window server tracks, so the app's pointer reader sees them as physical clicks.
// Usage: swift scripts/click-cursor.swift x y [clicks]
//   clicks: 1 (default, Poke) or 2 (Summon).

import AppKit

let args = CommandLine.arguments
guard args.count >= 3, let x = Double(args[1]), let y = Double(args[2]) else {
    FileHandle.standardError.write(Data("usage: click-cursor.swift x y [clicks]\n".utf8))
    exit(2)
}
let clicks = args.count >= 4 ? max(1, Int(args[3]) ?? 1) : 1

let point = CGPoint(x: x, y: y)
CGWarpMouseCursorPosition(point)

let interval = NSEvent.doubleClickInterval > 0 ? NSEvent.doubleClickInterval : 0.5
let gap = interval * 0.4

guard let source = CGEventSource(stateID: .combinedSessionState) else {
    FileHandle.standardError.write(Data("could not create CGEventSource\n".utf8))
    exit(1)
}

func post(_ type: CGEventType, clickState: Int64) {
    let event = CGEvent(
        mouseEventSource: source,
        mouseType: type,
        mouseCursorPosition: point,
        mouseButton: .left
    )
    // The gap alone lands two clicks inside the OS double-click interval; the
    // explicit click count is how the window server and any AX observer are
    // told which click in the run this is, rather than inferring it from timing.
    event?.setIntegerValueField(.mouseEventClickState, value: clickState)
    event?.post(tap: .cghidEventTap)
}

for i in 0 ..< clicks {
    let clickState = Int64(i + 1)
    post(.leftMouseDown, clickState: clickState)
    Thread.sleep(forTimeInterval: 0.06)
    post(.leftMouseUp, clickState: clickState)
    if i < clicks - 1 {
        Thread.sleep(forTimeInterval: gap)
    }
}

print("clicked (\(Int(x)),\(Int(y))) x\(clicks)")
