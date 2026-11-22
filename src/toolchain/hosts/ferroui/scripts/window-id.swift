// Prints the CGWindowID of the largest on-screen window belonging to the supplied process ID.
import CoreGraphics
import Foundation

guard let argument = CommandLine.arguments.dropFirst().first, let pid = Int(argument) else { exit(2) }
let area = { (window: [String: Any]) -> Double in
    let bounds = window[kCGWindowBounds as String] as? [String: Double] ?? [:]
    return (bounds["Width"] ?? 0) * (bounds["Height"] ?? 0)
}
let deadline = ProcessInfo.processInfo.systemUptime + 20
repeat {
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    let found = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid }.max { area($0) < area($1) }
    if let number = found?[kCGWindowNumber as String] as? Int { print(number); exit(0) }
    if kill(pid_t(pid), 0) != 0 { exit(1) }
    Thread.sleep(forTimeInterval: 0.1)
} while ProcessInfo.processInfo.systemUptime < deadline
exit(1)
