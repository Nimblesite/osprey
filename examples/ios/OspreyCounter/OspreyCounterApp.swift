import SwiftUI

// Implements [IOS-TARGET-ENTRY] and [IOS-SWIFT-HOST]. All calls use the main thread.
@main
struct OspreyCounterApp: App {
    private let startupStatus: Int32

    init() {
        startupStatus = osprey_main()
        if ProcessInfo.processInfo.arguments.contains("--osprey-smoke") {
            SmokeCheck.run(startupStatus: startupStatus)
        }
    }

    var body: some Scene {
        WindowGroup {
            if startupStatus == 0 {
                ContentView()
            } else {
                Text("Osprey initialization failed: \(startupStatus)")
            }
        }
    }
}

enum HostLog {
    static var message = "Waiting for Osprey"
    static var calls = 0
}

// Implements [IOS-HOST-IMPORTS]. Copy the borrowed UTF-8 string during the call.
@_cdecl("ios_host_log")
func iosHostLog(_ message: UnsafePointer<CChar>?) -> Int64 {
    guard let message else { return -1 }
    HostLog.message = String(cString: message)
    HostLog.calls += 1
    NSLog("Osprey: %@", HostLog.message)
    return Int64(HostLog.message.utf8.count)
}

func ospreyString(_ pointer: UnsafePointer<CChar>?) -> String {
    pointer.map { String(cString: $0) } ?? "Osprey returned a null string"
}
