// Prints the CGWindowID of <pid>'s on-screen layer-0 window titled <title>, for
// `screencapture -l`. Reads the window server only, so it needs no grant.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 3, let pid = Int(args[1]) else {
    FileHandle.standardError.write("usage: window-id <pid> <title>\n".data(using: .utf8)!)
    exit(2)
}
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows
where (window[kCGWindowOwnerPID as String] as? Int) == pid
    && (window[kCGWindowLayer as String] as? Int) == 0
    && (window[kCGWindowName as String] as? String) == args[2]
{
    print(window[kCGWindowNumber as String] as! Int)
    exit(0)
}
exit(1)
