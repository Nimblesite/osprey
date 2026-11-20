// Prints the CGWindowID of the largest on-screen window belonging to the supplied process ID.
import CoreGraphics
import Foundation

guard let argument = CommandLine.arguments.dropFirst().first, let pid = Int(argument) else { exit(2) }
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let area = { (window: [String: Any]) -> Double in
    let bounds = window[kCGWindowBounds as String] as? [String: Double] ?? [:]
    return (bounds["Width"] ?? 0) * (bounds["Height"] ?? 0)
}
let found = windows
    .filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid }
    .max { area($0) < area($1) }
if let number = found?[kCGWindowNumber as String] as? Int { print(number) } else { exit(1) }
