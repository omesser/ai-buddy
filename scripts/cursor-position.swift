// The cursor's position in top-left-origin points — the same convention
// CGWindowList uses for window bounds, so the two can be compared directly.
import AppKit

let p = NSEvent.mouseLocation
let mainHeight = CGDisplayBounds(CGMainDisplayID()).height
print(String(format: "%.0f %.0f", p.x, mainHeight - p.y))
