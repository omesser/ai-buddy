// Prints the window-server layer of <pid>'s on-screen window titled <title>:
// 0 is the normal level, 25 is NSStatusWindowLevel. Needs no grant.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 3, let pid = Int(args[1]) else {
    FileHandle.standardError.write("usage: window-layer <pid> <title>\n".data(using: .utf8)!)
    exit(2)
}
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows
where (window[kCGWindowOwnerPID as String] as? Int) == pid
    && (window[kCGWindowName as String] as? String) == args[2]
{
    print(window[kCGWindowLayer as String] as! Int)
    exit(0)
}
exit(1)
