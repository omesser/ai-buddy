// Prints the CGWindowID of <pid>'s on-screen window titled <title>, for
// `screencapture -l`. Optionally prints bounds as "id x y w h" with `-b`.
// Reads the window server only, so it needs no grant.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
let withBounds = args.contains("-b")
let filtered = args.filter { $0 != "-b" }
guard filtered.count == 3, let pid = Int(filtered[1]) else {
    FileHandle.standardError.write("usage: window-id [-b] <pid> <title>\n".data(using: .utf8)!)
    exit(2)
}
let title = filtered[2]
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows
where (window[kCGWindowOwnerPID as String] as? Int) == pid
    && (window[kCGWindowName as String] as? String) == title
{
    let id = window[kCGWindowNumber as String] as! Int
    if withBounds {
        if let bounds = window[kCGWindowBounds as String] as? [String: AnyObject] {
            let x = (bounds["X"] as? NSNumber)?.intValue ?? 0
            let y = (bounds["Y"] as? NSNumber)?.intValue ?? 0
            let w = (bounds["Width"] as? NSNumber)?.intValue ?? 0
            let h = (bounds["Height"] as? NSNumber)?.intValue ?? 0
            print("\(id) \(x) \(y) \(w) \(h)")
        } else {
            print(id)
        }
    } else {
        print(id)
    }
    exit(0)
}
exit(1)
