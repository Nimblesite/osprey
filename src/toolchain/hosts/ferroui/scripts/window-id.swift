// Prints the CGWindowID of the largest on-screen window whose owner name contains the text given.
import CoreGraphics
import Foundation

let owner = CommandLine.arguments.dropFirst().first ?? ""
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let area = { (window: [String: Any]) -> Double in
    let bounds = window[kCGWindowBounds as String] as? [String: Double] ?? [:]
    return (bounds["Width"] ?? 0) * (bounds["Height"] ?? 0)
}
let found = windows
    .filter { ($0[kCGWindowOwnerName as String] as? String)?.contains(owner) == true }
    .max { area($0) < area($1) }
if let number = found?[kCGWindowNumber as String] as? Int { print(number) } else { exit(1) }
