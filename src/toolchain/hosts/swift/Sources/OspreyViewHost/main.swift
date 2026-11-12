import Foundation
import OspreyShared
import SwiftUI

// Implements [NATIVE-SHM-VIEW] in the host's entry point: a window that draws
// whatever rows the application writes. `--self-test` drives Tally's journey
// with no window at all, so the host can be checked where no display exists.

/// The window and the store it draws from.
struct ViewWindow: View {
    @StateObject private var store = SharedStore()

    var body: some View {
        SharedView(store: store)
            .frame(minWidth: 480, minHeight: 360)
            .task { store.start() }
    }
}

/// The application, shown by the system as one window.
struct OspreyViewHost: App {
    var body: some Scene {
        WindowGroup("Osprey") { ViewWindow() }
    }
}

if CommandLine.arguments.contains("--self-test") {
    exit(SelfCheck.run())
}
OspreyViewHost.main()
