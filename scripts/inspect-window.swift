// Reports the overlay's on-screen geometry as JSON.
//
// Why Swift and not shell: no stock command-line tool reports a window's bounds
// or level. lsappinfo knows only about applications, and osascript needs an
// Accessibility grant — broader than anything this project asks for — then still
// cannot see a non-activating panel that is excluded from the switcher, nor
// report a window level at all.
//
// Why Swift and not Rust, in a Rust repository: CGWindowList lives in
// CoreGraphics, which would mean adding a dependency purely for a dev tool.
// Xcode is already required to build a Tauri app on macOS, so `swift` is
// already installed and costs nothing. It also keeps the observer independent
// of the app it observes, which is the point — the display-union bug was caught
// precisely because this asks the window server, not the app.
//
// Uses CGWindowListCopyWindowInfo, which returns window bounds, owner and layer
// with no permission prompt — the same call docs/SPEC.md specifies for
// WindowSource. Screen Recording is needed only for the screenshots that
// verify-overlay.sh takes, never for this.

import CoreGraphics
import Foundation

var displays: [[String: Any]] = []
var activeCount: UInt32 = 0
CGGetActiveDisplayList(0, nil, &activeCount)
var ids = [CGDirectDisplayID](repeating: 0, count: Int(activeCount))
CGGetActiveDisplayList(activeCount, &ids, &activeCount)
for id in ids {
    let b = CGDisplayBounds(id)
    displays.append([
        "id": Int(id), "x": b.origin.x, "y": b.origin.y,
        "w": b.width, "h": b.height,
    ])
}

var windows: [[String: Any]] = []
let opts = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
if let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] {
    for w in list {
        let owner = w[kCGWindowOwnerName as String] as? String ?? ""
        guard owner.lowercased().contains("ai-buddy") else { continue }
        let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        windows.append([
            "owner": owner,
            "layer": w[kCGWindowLayer as String] as? Int ?? -999,
            "alpha": w[kCGWindowAlpha as String] as? Double ?? -1,
            "onscreen": w[kCGWindowIsOnscreen as String] as? Bool ?? false,
            "x": b["X"] as? Double ?? 0, "y": b["Y"] as? Double ?? 0,
            "w": b["Width"] as? Double ?? 0, "h": b["Height"] as? Double ?? 0,
        ])
    }
}

let out = ["displays": displays, "windows": windows]
let data = try JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted])
print(String(data: data, encoding: .utf8)!)
