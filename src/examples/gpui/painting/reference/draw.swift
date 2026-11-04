// Draws the same strokes into the named process's window with synthetic mouse events: a freehand
// zigzag, then a Shift-held drag that the demo straightens. Usage: swift draw.swift <process>
import AppKit

// Posting synthetic events requires Accessibility permission; without it every event is dropped
// silently, so the missing permission is reported before the window lookup and nothing is posted.
// Nothing prompts for it: the caller grants it and retries.
guard CGPreflightPostEventAccess() else {
    FileHandle.standardError.write(Data("draw.swift: no Accessibility permission to post mouse events; grant it to the terminal or IDE running this comparison, then retry\n".utf8))
    exit(2)
}
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
guard let window = list.first(where: { ($0["kCGWindowOwnerName"] as? String) == CommandLine.arguments[1] && ($0["kCGWindowLayer"] as? Int) == 0 }) else { exit(1) }
let bounds = window["kCGWindowBounds"] as! [String: CGFloat]
// Window content coordinates: the title bar is 28 points.
let left = bounds["X"]!, top = bounds["Y"]! + 28
// The events go to whatever window is under the pointer: the target must be frontmost, as each
// example is when it has just opened (both activate themselves on launch).

func post(_ type: CGEventType, _ x: CGFloat, _ y: CGFloat, shift: Bool = false) {
    let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: CGPoint(x: left + x, y: top + y), mouseButton: .left)!
    if shift { event.flags = .maskShift }
    event.post(tap: .cghidEventTap)
    usleep(20000)
}

func stroke(_ points: [(CGFloat, CGFloat)], shift: Bool = false) {
    post(.leftMouseDown, points[0].0, points[0].1, shift: shift)
    for (x, y) in points.dropFirst() { post(.leftMouseDragged, x, y, shift: shift) }
    post(.leftMouseUp, points.last!.0, points.last!.1, shift: shift)
    usleep(200000)
}

var zigzag: [(CGFloat, CGFloat)] = []
for i in 0..<30 {
    let x: CGFloat = 300 + CGFloat(i) * 12
    let y: CGFloat = i % 2 == 0 ? 520 : 560
    zigzag.append((x, y))
}
var slope: [(CGFloat, CGFloat)] = []
for i in 0..<20 {
    let x: CGFloat = 900 + CGFloat(i) * 10
    let y: CGFloat = 500 + CGFloat(i) * 3
    slope.append((x, y))
}
stroke(zigzag)
stroke(slope, shift: true)
