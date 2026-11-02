// Prints the on-screen window id of the process named by the argument, for
// `screencapture -l`, so a test captures the app's window and nothing else.
import CoreGraphics
import Foundation

let name = CommandLine.arguments[1]
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
for window in list where (window[kCGWindowOwnerName as String] as? String) == name && (window[kCGWindowLayer as String] as? Int) == 0 {
    print(window[kCGWindowNumber as String] ?? "")
    exit(0)
}
exit(1)
