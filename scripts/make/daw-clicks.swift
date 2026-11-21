// Clicks a running window like a user would, for the daw video [DAW-VIDEO].
// usage: swift daw-clicks.swift <pid> "<x>,<y>[,<toX>,<toY>],<seconds after>;..."   (points relative to the window's top-left)
import AppKit
import ApplicationServices
import Foundation

let args = CommandLine.arguments
func fail(_ reason: String) -> Never {
    FileHandle.standardError.write(Data((reason + "\n").utf8))
    exit(1)
}
func attribute(_ element: AXUIElement, _ key: CFString) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, key, &value) == .success else { return nil }
    return value
}
func children(_ element: AXUIElement) -> [AXUIElement] {
    attribute(element, kAXChildrenAttribute as CFString) as? [AXUIElement] ?? []
}
func titled(_ element: AXUIElement, _ title: String) -> Bool {
    attribute(element, kAXTitleAttribute as CFString) as? String == title
}
guard (3...4).contains(args.count), let pid = Int32(args[1]) else {
    fail("usage: daw-clicks <pid> <x,y,wait> or daw-clicks <pid> <menu> <item>")
}
if args.count == 4 {
    guard let bar = attribute(AXUIElementCreateApplication(pid), kAXMenuBarAttribute as CFString),
          let menu = children(bar as! AXUIElement).first(where: { titled($0, args[2]) }),
          let item = children(menu).flatMap(children).first(where: { titled($0, args[3]) }),
          AXUIElementPerformAction(item, kAXPressAction as CFString) == .success else {
        fail("could not select \(args[2]) > \(args[3])")
    }
    if args[3] == "Quit" {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while kill(pid, 0) == 0 && ProcessInfo.processInfo.systemUptime < deadline { usleep(50_000) }
        guard kill(pid, 0) == -1 && errno == ESRCH else { fail("Quit left process \(pid) running") }
    }
    exit(0)
}
_ = NSRunningApplication(processIdentifier: pid)?.activate(options: [])
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == Int(pid) && ($0[kCGWindowLayer as String] as? Int) == 0 }),
      let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
      let left = bounds["X"], let top = bounds["Y"] else {
    FileHandle.standardError.write("no window for pid \(pid)\n".data(using: .utf8)!)
    exit(1)
}
func post(_ type: CGEventType, _ at: CGPoint) {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: at, mouseButton: .left)?.post(tap: .cghidEventTap)
}
// "x,y,wait" clicks; "x,y,toX,toY,wait" drags in 30 visible moves, like a hand on a knob or fader.
func drag(_ from: CGPoint, _ to: CGPoint) {
    for i in 1...30 {
        let t = CGFloat(i) / 30
        post(.leftMouseDragged, CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t))
        usleep(25_000)
    }
}
for step in args[2].split(separator: ";") {
    let parts = step.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
    guard parts.count == 3 || parts.count == 5 else { fail("invalid pointer step: \(step)") }
    let at = CGPoint(x: left + parts[0], y: top + parts[1])
    post(.mouseMoved, at)
    usleep(250_000)
    post(.leftMouseDown, at)
    usleep(60_000)
    if parts.count == 5 { drag(at, CGPoint(x: left + parts[2], y: top + parts[3])) }
    post(.leftMouseUp, parts.count == 5 ? CGPoint(x: left + parts[2], y: top + parts[3]) : at)
    usleep(useconds_t(parts[parts.count - 1] * 1_000_000))
}
