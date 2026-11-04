// Window helpers for driving the app in a test.
//   window-id <process>            the on-screen window id, for `screencapture -l`
//   window-id <process> <x> <y>    a real mouse click at a point in that window, in points from its top left
import CoreGraphics
import Foundation

let name = CommandLine.arguments[1]
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
for window in list where (window[kCGWindowOwnerName as String] as? String) == name && (window[kCGWindowLayer as String] as? Int) == 0 {
    if CommandLine.arguments.count == 4, let bounds = window[kCGWindowBounds as String] as? [String: CGFloat], let x = Double(CommandLine.arguments[2]), let y = Double(CommandLine.arguments[3]) {
        let point = CGPoint(x: bounds["X"]! + x, y: bounds["Y"]! + y)
        for kind in [CGEventType.mouseMoved, .leftMouseDown, .leftMouseUp] {
            CGEvent(mouseEventSource: nil, mouseType: kind, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
            usleep(60000)
        }
    } else {
        print(window[kCGWindowNumber as String] ?? "")
    }
    exit(0)
}
exit(1)
