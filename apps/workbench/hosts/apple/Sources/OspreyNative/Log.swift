import Foundation
import os

// Implements [UI-DIAGNOSTICS]: the backend writes what it does to the unified
// log under the app's bundle identifier, so a failure leaves a trace. Region
// keys, revisions, ordinals, tickets, kinds and sizes are public; control
// values and capability errors can hold evidence, so they are private.

enum Log {
    static let subsystem = Bundle.main.bundleIdentifier ?? "OspreyNative"
    /// Envelopes: patched and removed regions, requests, Osprey's diagnostics and failures.
    static let regions = Logger(subsystem: subsystem, category: "regions")
    /// Control events, lifecycle phases and capability completions on their way to Osprey.
    static let events = Logger(subsystem: subsystem, category: "events")
    /// Layouts that do not fit their window.
    static let layout = Logger(subsystem: subsystem, category: "layout")
}

/// A plain-text trace of every event and envelope, written only when `OSPREY_TRACE` names a file ([UI-DIAGNOSTICS]).
enum Trace {
    private static let path = ProcessInfo.processInfo.environment["OSPREY_TRACE"]

    static func line(_ text: String) {
        guard let path, let data = (text + "\n").data(using: .utf8) else { return }
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }
}
