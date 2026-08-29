// Reports the overlay's on-screen geometry as JSON.
//
// Why Swift and not shell: no stock command-line tool reports a window's bounds
// or level. lsappinfo knows only about applications, and osascript needs an
// Accessibility grant — broader than anything this project asks for — then still
// cannot see a non-activating panel that is excluded from the switcher, nor
// report a window level at all.
//
// Why Swift and not Rust, in a Rust repository: the crate's own WindowSource
// now reads these same CoreGraphics calls, so the bindings are no longer the
// obstacle. It cannot stand in for this, though — WindowSource is deliberately
// blind to our own process, because the overlay spans every display and a
// Character able to see it would find a Perch across the whole desktop and
// never fall again. This script exists to observe precisely that window, so the
// two want opposite things.
//
// Xcode is already required to build a Tauri app on macOS, so `swift` costs
// nothing to run. Keeping the observer outside the app's own toolchain is worth
// something on its own: the display-union bug was caught because this asks the
// window server rather than asking the app what it believes.
//
// Uses CGWindowListCopyWindowInfo, which returns window bounds, owner and layer
// with no permission prompt — the same call docs/SPEC.md specifies for
// WindowSource. Screen Recording is needed only for the screenshots that
// verify-overlay.sh takes, never for this.

import AppKit
import CoreGraphics
import Foundation

var displays: [[String: Any]] = []
var activeCount: UInt32 = 0
CGGetActiveDisplayList(0, nil, &activeCount)
var ids = [CGDirectDisplayID](repeating: 0, count: Int(activeCount))
CGGetActiveDisplayList(activeCount, &ids, &activeCount)
// The usable part of each display as well as its frame. A screen reserves
// strips of itself for the Dock and the menu bar, and the sprite comes to rest
// on the near edge of those rather than behind them (#39). NSScreen is the only
// thing that will say where they are: CoreGraphics reports the Dock as a window
// covering the whole display.
//
// NSScreen measures from the bottom of the main display and CoreGraphics from
// the top, so the insets are read from NSScreen and applied to the CoreGraphics
// frame, which is the space every other number in this file is in.
let screens = NSScreen.screens
for id in ids {
    let b = CGDisplayBounds(id)
    var usable: [String: Any] = ["x": b.origin.x, "y": b.origin.y, "w": b.width, "h": b.height]

    if let screen = screens.first(where: {
        ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?
            .uint32Value == id
    }) {
        let f = screen.frame
        let v = screen.visibleFrame
        let left = v.origin.x - f.origin.x
        let right = (f.origin.x + f.width) - (v.origin.x + v.width)
        let dock = v.origin.y - f.origin.y
        let menuBar = (f.origin.y + f.height) - (v.origin.y + v.height)
        usable = [
            "x": b.origin.x + left, "y": b.origin.y + menuBar,
            "w": b.width - left - right, "h": b.height - dock - menuBar,
        ]
    }

    displays.append([
        "id": Int(id), "x": b.origin.x, "y": b.origin.y,
        "w": b.width, "h": b.height,
        "usable": usable,
    ])
}

var windows: [[String: Any]] = []
// Everything the window server stacks above or below the ordinary application
// level: the menu bar, the Dock, the status items, Notification Centre. None of
// them is a Perch, and the frame loop is checked against these rectangles to
// prove the sprite never stands on one.
var elevated: [[String: Any]] = []
let opts = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
if let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] {
    for w in list {
        let owner = w[kCGWindowOwnerName as String] as? String ?? ""
        let layer = w[kCGWindowLayer as String] as? Int ?? -999
        let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        let entry: [String: Any] = [
            "owner": owner,
            "layer": layer,
            "alpha": w[kCGWindowAlpha as String] as? Double ?? -1,
            "onscreen": w[kCGWindowIsOnscreen as String] as? Bool ?? false,
            "x": b["X"] as? Double ?? 0, "y": b["Y"] as? Double ?? 0,
            "w": b["Width"] as? Double ?? 0, "h": b["Height"] as? Double ?? 0,
        ]
        if owner.lowercased().contains("ai-buddy") {
            windows.append(entry)
        } else if layer != 0 {
            elevated.append(entry)
        }
    }
}

let out = ["displays": displays, "windows": windows, "elevated": elevated]
let data = try JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted])
print(String(data: data, encoding: .utf8)!)
