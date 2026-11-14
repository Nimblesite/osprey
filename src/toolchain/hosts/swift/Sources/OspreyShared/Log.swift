import Foundation
import os

// Implements [NATIVE-HOSTS-DIAGNOSTICS]: the backend writes what it does to the unified
// log under the app's bundle identifier, so a failure leaves a trace. Region
// keys, revisions, ordinals, tickets, kinds and sizes are public; control
// values and capability errors can hold evidence, so they are private.

enum Log {
    static let subsystem = Bundle.main.bundleIdentifier ?? "OspreyViewHost"
    /// Envelopes: patched and removed regions, requests, Osprey's diagnostics and failures.
    static let regions = Logger(subsystem: subsystem, category: "regions")
    /// Control events, lifecycle phases and capability completions on their way to Osprey.
    static let events = Logger(subsystem: subsystem, category: "events")
    /// Layouts that do not fit their window.
    static let layout = Logger(subsystem: subsystem, category: "layout")
}
