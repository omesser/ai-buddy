// Reads whether the Chat composer paints a blinking text caret (#891).
//
// The accessibility tree cannot answer this. A text field reports
// AXSelectedTextRange whether or not anything is drawn, so the composer
// looked healthy to AX for the whole life of the bug. The only witness is the
// screen: a caret is a narrow column that toggles on and off about twice a
// second, so a burst of stills of the field either changes or it does not.
//
// Needs an Accessibility grant and a Screen Recording grant for whatever runs
// it (System Settings > Privacy & Security). Caller: verify-chat-caret-macos.sh.
//
// This takes the pointer's place in the window order: it brings ai-buddy to
// the front for the length of the burst. It posts no click and no keystroke.

import AppKit
import ApplicationServices
import Foundation

let args = Array(CommandLine.arguments.dropFirst())

func die(_ message: String) -> Never {
    FileHandle.standardError.write("ax-chat-caret: \(message)\n".data(using: .utf8)!)
    exit(2)
}

guard args.count >= 3, let pid = pid_t(args[0]) else {
    die("usage: ax-chat-caret <pid> <window title> <out dir> [frames]")
}
let wantedTitle = args[1]
let outDir = args[2]
let frames = args.count >= 4 ? (Int(args[3]) ?? 16) : 16

guard AXIsProcessTrusted() else {
    die("""
        no Accessibility grant. Add the terminal running this to
        System Settings > Privacy & Security > Accessibility, then run again.
        """)
}

let app = AXUIElementCreateApplication(pid)

func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
        return nil
    }
    return value
}

func string(_ element: AXUIElement, _ name: String) -> String? {
    attribute(element, name) as? String
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
}

func waitFor<T>(_ seconds: Double, _ produce: () -> T?) -> T? {
    let deadline = Date().addingTimeInterval(seconds)
    while Date() < deadline {
        if let value = produce() { return value }
        usleep(120_000)
    }
    return produce()
}

func frame(_ element: AXUIElement) -> CGRect? {
    guard let position = attribute(element, kAXPositionAttribute),
        let size = attribute(element, kAXSizeAttribute)
    else { return nil }
    var origin = CGPoint.zero
    var extent = CGSize.zero
    AXValueGetValue(position as! AXValue, .cgPoint, &origin)
    AXValueGetValue(size as! AXValue, .cgSize, &extent)
    return CGRect(origin: origin, size: extent)
}

func window(titled title: String) -> AXUIElement? {
    let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
    return windows.first { string($0, kAXTitleAttribute) == title }
}

/// Every element under `root` in tree order, which is render order.
func flatten(_ root: AXUIElement, _ depth: Int = 0) -> [AXUIElement] {
    guard depth < 30 else { return [] }
    return [root] + children(root).flatMap { flatten($0, depth + 1) }
}

/// The first editable text area in tree order. Whether the value can be
/// written is the only signal a frozen field gives: the Prompt tab draws two
/// read-only layers beside the one you can type into, and a read-only field
/// paints no caret however healthy the window is.
func editableTextArea(_ root: AXUIElement) -> AXUIElement? {
    flatten(root).first { element in
        guard string(element, kAXRoleAttribute) == "AXTextArea" else { return false }
        var settable: DarwinBoolean = false
        let read = AXUIElementIsAttributeSettable(
            element, kAXValueAttribute as CFString, &settable)
        return read == .success && settable.boolValue
    }
}

/// A WKWebView publishes its tree only on request, and the real subtree lands
/// a moment after the first ask. `ax-settings.swift` carries the long version.
func settled(_ window: AXUIElement) -> Bool {
    guard let area = flatten(window).first(where: { string($0, kAXRoleAttribute) == "AXWebArea" })
    else { return false }
    return !children(area).isEmpty
}

/// The window the caret question is about, front-most so WebKit paints it as
/// active. A window that is not key draws no caret however the page is styled,
/// which is the whole reason this script exists.
func activate() {
    NSRunningApplication(processIdentifier: pid)?.activate()
    usleep(400_000)
}

func focusedRole() -> String {
    var reference: CFTypeRef?
    guard
        AXUIElementCopyAttributeValue(app, kAXFocusedUIElementAttribute as CFString, &reference)
            == .success, let focused = reference,
        CFGetTypeID(focused) == AXUIElementGetTypeID()
    else { return "none" }
    return string(focused as! AXUIElement, kAXRoleAttribute) ?? "?"
}

func keyState(_ window: AXUIElement) -> String {
    let main = (attribute(window, kAXMainAttribute) as? Bool).map(String.init) ?? "-"
    let focused = (attribute(window, kAXFocusedAttribute) as? Bool).map(String.init) ?? "-"
    return "main=\(main) key=\(focused)"
}

/// RGBA of one still, or nil when the file never landed.
func pixels(_ path: String) -> (width: Int, height: Int, bytes: [UInt8])? {
    guard let image = NSImage(contentsOfFile: path),
        let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
    else { return nil }
    let width = cgImage.width
    let height = cgImage.height
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    guard
        let context = CGContext(
            data: &bytes, width: width, height: height, bitsPerComponent: 8,
            bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))
    return (width, height, bytes)
}

struct Burst {
    let name: String
    let rect: String
    let differingFrames: Int
    let columns: Int
    let rows: Int
    let columnSpan: String

    /// A caret, told apart from a repaint: something toggled, and what toggled
    /// is a narrow column taller than it is wide. Retina doubles the width, so
    /// the bound is in device pixels rather than points.
    var isCaret: Bool { differingFrames > 0 && columns <= 8 && rows >= 8 }
}

/// Take `frames` stills of `box` and say what changed between them.
func burst(_ name: String, _ box: CGRect) -> Burst {
    let rect = "\(Int(box.origin.x)),\(Int(box.origin.y)),\(Int(box.width)),\(Int(box.height))"
    var paths: [String] = []
    for index in 0..<frames {
        let path = "\(outDir)/\(name)-\(String(format: "%02d", index)).png"
        let capture = Process()
        capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        capture.arguments = ["-x", "-o", "-R", rect, path]
        try? capture.run()
        capture.waitUntilExit()
        paths.append(path)
        usleep(110_000)
    }

    guard let first = pixels(paths[0]) else {
        die("no still at \(paths[0]) - is Screen Recording granted?")
    }
    var changedColumns = Set<Int>()
    var changedRows = Set<Int>()
    var differing = 0
    for path in paths.dropFirst() {
        guard let still = pixels(path), still.width == first.width, still.height == first.height
        else { continue }
        var differs = false
        for y in 0..<first.height {
            for x in 0..<first.width {
                let offset = (y * first.width + x) * 4
                // 24 clears JPEG-ish noise and leaves a caret, which is drawn
                // at the ink colour against the field fill.
                let moved = (0..<3).contains {
                    abs(Int(still.bytes[offset + $0]) - Int(first.bytes[offset + $0])) > 24
                }
                if moved {
                    changedColumns.insert(x)
                    changedRows.insert(y)
                    differs = true
                }
            }
        }
        if differs { differing += 1 }
    }
    let span =
        changedColumns.isEmpty
        ? "-" : "\(changedColumns.min()!)..\(changedColumns.max()!)"
    return Burst(
        name: name, rect: rect, differingFrames: differing, columns: changedColumns.count,
        rows: changedRows.count, columnSpan: span)
}

try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

guard let chat = waitFor(15, { window(titled: wantedTitle) }) else {
    die("no window titled \(wantedTitle)")
}
activate()
guard waitFor(8, { settled(chat) ? chat : nil }) != nil else {
    die("the Chat webview never published its tree")
}

// The composer is the text area inside the form landmark; the Prompt tab's
// field is the control. Both live in this one window, so they share its key
// state and its webview. If only the composer stays dark the cause is in the
// page; if both do, it is the window.
let tree = flatten(chat)
guard let composer = editableTextArea(chat), let composerBox = frame(composer)
else { die("no composer text area in \(wantedTitle)") }

print("window \(keyState(chat)) focused=\(focusedRole())")
let composerBurst = burst("composer", composerBox)
print("window \(keyState(chat)) focused=\(focusedRole())")

// The Prompt tab, opened through its accessibility action rather than a click.
var controlBurst: Burst?
if let tab = tree.first(where: {
    string($0, kAXRoleAttribute) == "AXRadioButton" && string($0, kAXTitleAttribute) == "Prompt"
}) {
    if AXUIElementPerformAction(tab, kAXPressAction as CFString) == .success {
        usleep(600_000)
        if let field = editableTextArea(chat), let box = frame(field) {
            controlBurst = burst("prompt", box)
        }
    }
}

func report(_ burst: Burst) {
    print(
        "\(burst.name): rect=\(burst.rect) differing=\(burst.differingFrames)/\(frames - 1) "
            + "columns=\(burst.columns) rows=\(burst.rows) span=\(burst.columnSpan) "
            + "caret=\(burst.isCaret)")
}

report(composerBurst)
if let control = controlBurst { report(control) } else { print("prompt: not reached") }

if composerBurst.isCaret {
    print("PASS composer paints a blinking caret")
    exit(0)
}
if let control = controlBurst, control.isCaret {
    print("FAIL composer draws no caret while the Prompt field does - the cause is in the page")
} else {
    print("FAIL neither field draws a caret - the cause is the window, not the page")
}
exit(1)
