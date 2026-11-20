// Clicks a running window like a user would, for the daw video [DAW-VIDEO].
// usage: swift daw-clicks.swift <pid> "<x>,<y>,<seconds after>;..."   (points relative to the window's top-left)
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 3, let pid = Int(args[1]) else {
    FileHandle.standardError.write("usage: daw-clicks.swift <pid> \"x,y,wait;...\"\n".data(using: .utf8)!)
    exit(2)
}
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }),
      let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
      let left = bounds["X"], let top = bounds["Y"] else {
    FileHandle.standardError.write("no window for pid \(pid)\n".data(using: .utf8)!)
    exit(1)
}
func post(_ type: CGEventType, _ at: CGPoint) {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: at, mouseButton: .left)?.post(tap: .cghidEventTap)
}
for step in args[2].split(separator: ";") {
    let parts = step.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
    guard parts.count == 3 else { continue }
    let at = CGPoint(x: left + parts[0], y: top + parts[1])
    post(.mouseMoved, at)
    usleep(250_000)
    post(.leftMouseDown, at)
    usleep(60_000)
    post(.leftMouseUp, at)
    usleep(useconds_t(parts[2] * 1_000_000))
}
