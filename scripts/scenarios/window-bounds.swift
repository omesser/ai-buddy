// Prints the bounds of <pid>'s on-screen window titled <title> as "x y w h".
// Reads the window server only, so it needs no grant.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 3, let pid = Int(args[1]) else {
    FileHandle.standardError.write("usage: window-bounds <pid> <title>\n".data(using: .utf8)!)
    exit(2)
}
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows
where (window[kCGWindowOwnerPID as String] as? Int) == pid
    && (window[kCGWindowName as String] as? String) == args[2]
{
    if let bounds = window[kCGWindowBounds as String] as? [String: AnyObject] {
        let x = (bounds["X"] as? NSNumber)?.intValue ?? 0
        let y = (bounds["Y"] as? NSNumber)?.intValue ?? 0
        let w = (bounds["Width"] as? NSNumber)?.intValue ?? 0
        let h = (bounds["Height"] as? NSNumber)?.intValue ?? 0
        print("\(x) \(y) \(w) \(h)")
        exit(0)
    }
}
exit(1)
