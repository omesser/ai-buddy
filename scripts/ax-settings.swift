// Drives and reads the macOS Settings window through the Accessibility API.
// Every question the verification asks (is this row frozen, do the sections
// come in this order) is a string or boolean the AX tree already holds.

// Not osascript: System Events cannot report a control's AXEnabled, and it
// addresses controls by index, so a reordered section silently checks the
// wrong row. Coordinates appear nowhere; controls are pressed by name.

// Needs an Accessibility grant for whatever runs it (System Settings > Privacy
// & Security > Accessibility). verify-settings-macos.sh is its only caller.

import ApplicationServices
import Foundation

let args = Array(CommandLine.arguments.dropFirst())

func die(_ message: String) -> Never {
    FileHandle.standardError.write("ax-settings: \(message)\n".data(using: .utf8)!)
    exit(2)
}

guard args.count >= 2, let pid = pid_t(args[1]) else {
    die("usage: ax-settings <open|tab|pick|dump|frame> <pid> [args]")
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

/// Depth-first search for the first element a predicate accepts. Depth-first
/// because the tab group sits deeper than the window's own buttons, and the
/// first match by either order is the same element in every tree this drives.
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

/// A real click at the element's centre, for elements AX describes but will
/// not act on: the status item answers AXPress with kAXErrorCannotComplete,
/// its menu being the window server's. AXPosition keeps the point honest.
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

/// AXPress, falling back to AXShowMenu and then to a synthesized click. A
/// status item answers to AXPress on some macOS versions and only to
/// AXShowMenu on others, and which one has never been documented.
func press(_ element: AXUIElement) -> Bool {
    for action in [kAXPressAction as String, "AXShowMenu", kAXPickAction as String] {
        guard actions(element).contains(action) else { continue }
        let err = AXUIElementPerformAction(element, action as CFString)
        if err == .success { return true }
        lastError = "\(action) -> \(err.rawValue)"
    }
    return click(element)
}

func window(titled title: String) -> AXUIElement? {
    let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
    return windows.first { string($0, kAXTitleAttribute) == title }
}

func settingsWindow() -> AXUIElement? { window(titled: "Settings") }

/// A WKWebView publishes its tree only on request. The first pass into the
/// window gets the content view as a childless AXGroup with no AXScrollArea
/// or AXWebArea under it, and the real subtree lands 107 ms after that ask.
/// Sleeping instead of asking never gets it, because the ask is what primes
/// it. The native Settings window has no childless AXGroup among its own
/// direct children, so that shape is an unprimed webview and nothing else. #706.
func webContentSettled(_ window: AXUIElement) -> Bool {
    if let area = find(window, where: { string($0, kAXRoleAttribute) == "AXWebArea" }) {
        return !children(area).isEmpty
    }
    return !children(window).contains {
        string($0, kAXRoleAttribute) == "AXGroup" && children($0).isEmpty
    }
}

/// `tab`, `pick` and `dump` each address a control the webview owns, so each
/// waits for that priming. `open` and `frame` do not: the status item and the
/// window rectangle are AppKit's, and neither descends into the content. #779.
func settledWindow(titled title: String) -> AXUIElement? {
    guard let window = window(titled: title) else { return nil }
    _ = waitFor(5, { webContentSettled(window) ? window : nil })
    return window
}

/// Windows this pid owns on the menu layer (100+). WebKit's <select> menu
/// is not in the AX tree; a new layer-101 window is the evidence it opened. #797
func menuWindowCount() -> Int {
    let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    return ((info as? [[String: AnyObject]]) ?? []).filter {
        ($0[kCGWindowOwnerPID as String] as? pid_t) == pid
            && (($0[kCGWindowLayer as String] as? Int) ?? 0) >= 100
    }.count
}

/// Type the option title into an already-open menu, then Return. macOS menus
/// select by typed prefix. Only call while a menu tracks, so keys cannot land
/// in the page. Return is consumed by the menu, not the #774 Enter handler.
func typeSelect(_ title: String) {
    for character in title {
        var utf16 = Array(String(character).utf16)
        for down in [true, false] {
            guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
            else { continue }
            event.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: &utf16)
            event.post(tap: .cghidEventTap)
        }
        usleep(50_000)
    }
    // 36 is Return. The tracking menu consumes it, so Settings stays open.
    for down in [true, false] {
        CGEvent(keyboardEventSource: nil, virtualKey: 36, keyDown: down)?.post(tap: .cghidEventTap)
        usleep(30_000)
    }
}

/// The popup whose preceding static text is `label`. Render order, not index.
func labelledPopup(_ label: String) -> AXUIElement? {
    guard let window = settingsWindow(), webContentSettled(window) else { return nil }
    var lastLabel = ""
    var popup: AXUIElement?
    func scan(_ element: AXUIElement, depth: Int) {
        guard depth < 30, popup == nil else { return }
        let role = string(element, kAXRoleAttribute) ?? ""
        if role == "AXStaticText" { lastLabel = string(element, kAXValueAttribute) ?? "" }
        if role == "AXPopUpButton", lastLabel == label {
            popup = element
            return
        }
        for child in children(element) { scan(child, depth: depth + 1) }
    }
    scan(window, depth: 0)
    return popup
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
    guard let window = settledWindow(titled: "Settings") else { die("Settings is not open") }
    guard let tab = find(window, where: { string($0, kAXTitleAttribute) == args[2] }),
        press(tab)
    else {
        die("no tab titled \(args[2])")
    }

case "pick":
    // Changing a popup in place is the only way to reach the states between
    // two launches. The popup is addressed by the label above it, because the
    // tree is in render order and a label is stabler than an index.
    guard args.count >= 4 else { die("usage: ax-settings pick <pid> <label> <option>") }
    guard settledWindow(titled: "Settings") != nil else { die("Settings is not open") }
    guard let target = labelledPopup(args[2]) else { die("no popup labelled \(args[2])") }
    // Setting AXValue is refused by NSPopUpButton, so open it and press the
    // row: the same path a person takes, and the only one that fires the
    // action the renderer listens for.
    let menusBefore = menuWindowCount()
    guard press(target) else { die("could not open the \(args[2]) popup") }
    // AppKit publishes AXMenu under the popup. WebKit's <select> does not, so
    // a new menu-layer window is the only signal it drew. Race them. #797
    var option: AXUIElement?
    var drew = false
    let deadline = Date().addingTimeInterval(5)
    repeat {
        option = find(target) { string($0, kAXTitleAttribute) == args[3] }
        drew = menuWindowCount() > menusBefore
        if option != nil || drew { break }
        usleep(100_000)
    } while Date() < deadline
    if let option = option {
        guard press(option) else { die("could not pick \(args[3])") }
    } else if drew {
        typeSelect(args[3])
    } else {
        die("the \(args[2]) popup drew no menu; last: \(lastError)")
    }
    // The row commits off-thread and the webview rebuilds the tree, so the
    // captured element then reads nil. Wait on a freshly resolved popup. #797
    guard
        waitFor(
            10,
            {
                guard let fresh = labelledPopup(args[2]),
                    string(fresh, kAXValueAttribute) == args[3],
                    find(fresh, where: { string($0, kAXRoleAttribute) == "AXMenu" }) == nil
                else { return nil }
                return fresh
            }) != nil
    else {
        let current =
            labelledPopup(args[2]).flatMap { string($0, kAXValueAttribute) } ?? "nothing"
        die("the \(args[2]) popup still reads \(current) after picking \(args[3])")
    }

case "frame":
    // For `screencapture -R`, so the still is the window and not the desktop.
    guard let window = settingsWindow(), let rect = frame(window) else {
        die("Settings is not open")
    }
    print("\(Int(rect.origin.x)),\(Int(rect.origin.y)),\(Int(rect.width)),\(Int(rect.height))")

case "dump":
    // One line per element as role|title|value|placeholder|enabled|settable, so
    // the shell can grep for a label and read the enabled flag beside it. Order
    // is the tree's own, which is the render order, so section order is assertable.
    let wanted = args.count >= 3 ? args[2] : "Settings"
    guard let window = settledWindow(titled: wanted) else { die("\(wanted) is not open") }
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
