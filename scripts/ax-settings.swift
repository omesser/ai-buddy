// Drives and reads the macOS Settings window through the Accessibility API.
//
// Why this exists: the Settings window is native AppKit, so the only way to
// check it from outside was a screenshot, and a screenshot has to be looked at
// by a person (or a model) to mean anything. Every question the verification
// actually asks - is this row frozen, do the sections come in this order, does
// this label say "AI brain" - is a string or a boolean the Accessibility tree
// already holds. Reading it turns a ten-minute click-and-look session into a
// second of text, and the assertions become greppable instead of visual.
//
// Why not osascript: System Events can click and type, but it cannot report a
// control's AXEnabled, which is the whole question for a frozen row. It also
// addresses controls by index, so a reordered section silently checks the
// wrong row.
//
// Coordinates appear nowhere here on purpose. `open` presses the status item
// and its Settings row by name, and `tab` presses the tab by title, so a moved
// window or a Retina display cannot make the script click the wrong thing -
// the failure mode that made the manual runs unreliable.
//
// Needs an Accessibility grant for whatever runs it: the terminal or the IDE.
// System Settings > Privacy & Security > Accessibility. Nothing else in this
// repository asks for that grant; verify-settings-macos.sh is its only caller.

import ApplicationServices
import Foundation

let args = Array(CommandLine.arguments.dropFirst())

func die(_ message: String) -> Never {
    FileHandle.standardError.write("ax-settings: \(message)\n".data(using: .utf8)!)
    exit(2)
}

guard args.count >= 2, let pid = pid_t(args[1]) else {
    die("usage: ax-settings <open|tab|dump|frame> <pid> [tab-title]")
}

guard AXIsProcessTrusted() else {
    die("""
        no Accessibility grant. Add the terminal running this to
        System Settings > Privacy & Security > Accessibility, then run again.
        """)
}

let app = AXUIElementCreateApplication(pid)

/// One attribute, or nil when the element does not carry it.
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

/// Depth-first search for the first element a predicate accepts.
///
/// Depth-first rather than breadth-first because the tab group sits deeper
/// than the window's own buttons, and the first match by either order is the
/// same element in every tree this drives.
func find(
    _ element: AXUIElement,
    depth: Int = 0,
    where accept: (AXUIElement) -> Bool
) -> AXUIElement? {
    if accept(element) { return element }
    // The tree under a Settings window is shallow; the bound only stops a
    // runaway on a cycle, which AX does produce through AXParent links.
    guard depth < 30 else { return nil }
    for child in children(element) {
        if let hit = find(child, depth: depth + 1, where: accept) { return hit }
    }
    return nil
}

/// Waits for a predicate to find something, polling rather than sleeping a
/// fixed span: the window server answers as soon as the window exists, and a
/// fixed sleep is either a stall or a flake.
func waitFor(
    _ seconds: Double,
    _ probe: () -> AXUIElement?
) -> AXUIElement? {
    let deadline = Date().addingTimeInterval(seconds)
    repeat {
        if let hit = probe() { return hit }
        usleep(100_000)
    } while Date() < deadline
    return nil
}

var lastError = "none attempted"

/// The actions an element answers to, for the failure message.
func actions(_ element: AXUIElement) -> [String] {
    var names: CFArray?
    guard AXUIElementCopyActionNames(element, &names) == .success else { return [] }
    return names as? [String] ?? []
}

/// The element's own on-screen rectangle, in the global coordinates a
/// synthesized click wants.
func frame(_ element: AXUIElement) -> CGRect? {
    guard let posValue = attribute(element, kAXPositionAttribute),
        let sizeValue = attribute(element, kAXSizeAttribute)
    else { return nil }
    var origin = CGPoint.zero
    var size = CGSize.zero
    guard AXValueGetValue(posValue as! AXValue, .cgPoint, &origin),
        AXValueGetValue(sizeValue as! AXValue, .cgSize, &size)
    else { return nil }
    return CGRect(origin: origin, size: size)
}

/// A real click at the element's centre, for the elements AX will describe but
/// will not act on.
///
/// The status item is one: it advertises AXPress and answers it with
/// kAXErrorCannotComplete, because the menu it opens is the window server's
/// and not the app's. Taking the point from AXPosition rather than writing one
/// down keeps the click honest - a menu bar that rearranges, or a display with
/// a different scale, moves the point with it.
func click(_ element: AXUIElement) -> Bool {
    guard let rect = frame(element) else { return false }
    let point = CGPoint(x: rect.midX, y: rect.midY)
    guard let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown,
                             mouseCursorPosition: point, mouseButton: .left),
        let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp,
                         mouseCursorPosition: point, mouseButton: .left)
    else { return false }
    down.post(tap: .cghidEventTap)
    usleep(60_000)
    up.post(tap: .cghidEventTap)
    return true
}

/// AXPress, falling back to AXShowMenu and then to a synthesized click.
///
/// A status item answers to AXPress on some macOS versions and only to
/// AXShowMenu on others, and which one it is has never been documented.
func press(_ element: AXUIElement) -> Bool {
    for action in [kAXPressAction as String, "AXShowMenu", kAXPickAction as String] {
        guard actions(element).contains(action) else { continue }
        let err = AXUIElementPerformAction(element, action as CFString)
        if err == .success { return true }
        lastError = "\(action) -> \(err.rawValue)"
    }
    return click(element)
}

func settingsWindow() -> AXUIElement? {
    let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
    return windows.first { string($0, kAXTitleAttribute) == "Settings" }
}

switch args[0] {
case "open":
    // The status item, not the app's own menu bar: Settings has no keyboard
    // shortcut and no menu of its own, so the tray row is the only way in.
    guard let extras = waitFor(20, { attribute(app, "AXExtrasMenuBar").map { $0 as! AXUIElement } })
    else {
        die("no status item after 20s - did the app finish launching?")
    }
    guard let item = children(extras).first else { die("the app has no status item") }
    guard press(item) else {
        die("status item refused every action \(actions(item)); last: \(lastError)")
    }
    guard
        let row = waitFor(
            5,
            {
                find(extras) { string($0, kAXTitleAttribute)?.hasPrefix("Settings") == true }
            })
    else {
        die("the tray menu has no Settings row")
    }
    guard press(row) else { die("could not press Settings…") }
    guard waitFor(10, { settingsWindow() }) != nil else {
        die("Settings did not open within 10s")
    }

case "tab":
    guard args.count >= 3 else { die("usage: ax-settings tab <pid> <title>") }
    guard let window = settingsWindow() else { die("Settings is not open") }
    guard let tab = find(window, where: { string($0, kAXTitleAttribute) == args[2] }),
        press(tab)
    else {
        die("no tab titled \(args[2])")
    }

case "frame":
    // For `screencapture -R`, so the still is the window and not the desktop.
    guard let window = settingsWindow(), let rect = frame(window) else {
        die("Settings is not open")
    }
    print("\(Int(rect.origin.x)),\(Int(rect.origin.y)),\(Int(rect.width)),\(Int(rect.height))")

case "dump":
    // One line per element as role|title|value|placeholder|enabled|settable,
    // so the
    // shell can grep for a label and read the enabled flag beside it. Order is
    // the tree's own order, which is the render order - that is what makes
    // section order assertable.
    guard let window = settingsWindow() else { die("Settings is not open") }
    func walk(_ element: AXUIElement, depth: Int) {
        guard depth < 30 else { return }
        let role = string(element, kAXRoleAttribute) ?? "?"
        // The subrole is what separates the API key row from the rows beside
        // it: a secure field reports AXSecureTextField and an empty value.
        let subrole = string(element, kAXSubroleAttribute) ?? ""
        let title = string(element, kAXTitleAttribute) ?? ""
        let value = attribute(element, kAXValueAttribute).map { "\($0)" } ?? ""
        // A frozen field is often empty and showing its placeholder, so the
        // placeholder is the only text that says which row this is.
        let placeholder = string(element, kAXPlaceholderValueAttribute) ?? ""
        let enabled = (attribute(element, kAXEnabledAttribute) as? Bool).map(String.init) ?? ""
        // Whether the value can be written is the only signal a frozen text
        // field gives: the renderer freezes with setEditable(false), which
        // leaves AXEnabled true and the field looking exactly like a live one.
        var settableFlag: DarwinBoolean = false
        let settable =
            AXUIElementIsAttributeSettable(element, kAXValueAttribute as CFString, &settableFlag)
            == .success ? String(settableFlag.boolValue) : ""
        // A trailing newline inside a value would break the one-line-per-row
        // contract the shell greps against.
        let flat = { (s: String) in s.replacingOccurrences(of: "\n", with: "\\n") }
        print(
            "\(role)\(subrole.isEmpty ? "" : ":" + subrole)|\(flat(title))|\(flat(value))"
                + "|\(flat(placeholder))|\(enabled)|\(settable)")
        for child in children(element) { walk(child, depth: depth + 1) }
    }
    walk(window, depth: 0)

default:
    die("unknown command \(args[0])")
}
