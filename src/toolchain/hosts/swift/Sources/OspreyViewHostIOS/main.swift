#if canImport(UIKit) && !canImport(AppKit)
import Foundation
import OspreyShared
import SwiftUI

// Implements [NATIVE-HOSTS-CONTRACT] and [NATIVE-SHM-NODES]: the iOS entry point, the same
// OspreyShared library the macOS host runs. It opens the application's archive over the shared
// frame — platform "ios", home the app's Documents folder, dark from the system — and shows its
// root region with this device's native components. No application state, screen or policy
// lives here. `--view-check` runs Tally's SelfCheck journey with no person present, reports the
// verdict on stdout and in the app's Documents, and exits; `--self-test <script>` plays an app's
// self-test.json in the open app (Journey.swift). macOS's entry is Sources/OspreyViewHost.

let launch = HostLaunch.ready(from: Array(CommandLine.arguments.dropFirst()))
let journey = IOSJourney.named(in: Array(CommandLine.arguments.dropFirst()))
guard launch.contains("--view-check") else {
    IOSHostApp.main()
    exit(0)
}
exit(MainActor.assumeIsolated { IOSCheck.run(SelfCheck.run()) })

struct IOSHostApp: App {
    @StateObject private var store = makeStore(arguments: HostLaunch.arguments(from: Array(CommandLine.arguments.dropFirst())),
                                               adapters: Adapters.standard(store: bundleStore()))

    var body: some Scene {
        WindowGroup(Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Osprey") {
            RootView(store: store)
                .task { if let journey { await IOSJourney(store: store, script: journey).run() } }
        }
    }
}

/// The headless verdict, written where the launch that asked for the check reads it back.
@MainActor
enum IOSCheck {
    static func run(_ verdict: Int32) -> Int32 {
        let marker = verdict == 0 ? "OSPREY_IOS_VIEW_OK" : "OSPREY_IOS_VIEW_FAILED"
        print(marker)
        guard let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else {
            print("FAILED: the app has no Documents folder")
            return 1
        }
        do {
            try marker.write(to: documents.appendingPathComponent("view-check.txt"), atomically: true, encoding: .utf8)
        } catch {
            print("FAILED: cannot write the verdict: \(error.localizedDescription)")
            return 1
        }
        return verdict
    }
}
#else
// This executable is the iOS host; macOS runs OspreyViewHost over the same OspreyShared library.
print("OspreyViewHostIOS runs on iOS; on macOS run OspreyViewHost.")
#endif
