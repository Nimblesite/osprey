import Foundation
import OspreyShared
import SwiftUI

// Implements [NATIVE-SHM-VIEW] in the host's entry point: a window that draws whatever rows the
// application writes, with the host's diagnostic strip beneath it. `--self-test` drives Tally's
// journey with no window at all, so the host can be checked where no display exists.

/// The store this host's window draws from, answering the capabilities it carries.
@MainActor
private func makeStore() -> SharedStore {
    let bundle = Bundle.main.bundleIdentifier ?? "org.ospreylang.host"
    return SharedStore(adapters: SharedStore.standardAdapters(store: StoreAdapter.standard(bundle: bundle)))
}

/// The window and the store it draws from.
struct ViewWindow: View {
    @StateObject private var store = makeStore()
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        SharedView(store: store)
            .frame(minWidth: 480, minHeight: 360)
            .task {
                store.start(arguments: Array(CommandLine.arguments.dropFirst()), dark: scheme == .dark)
            }
            .onChange(of: scheme) { _, now in
                store.appear(dark: now == .dark)
            }
    }
}

/// The application, shown by the system as one window.
struct OspreyViewHost: App {
    var body: some Scene {
        WindowGroup("Osprey") { ViewWindow() }
    }
}

// Launch arguments belong to the application; AppKit would otherwise read paths among them as
// documents to open, and open no window. A string, not a Boolean: the open panel service reads
// this default as text and crashes on any other type.
UserDefaults.standard.set("NO", forKey: "NSTreatUnknownArgumentsAsOpen")
if CommandLine.arguments.contains("--self-test") {
    // The main thread is this process's UI thread already; no window is opened.
    exit(MainActor.assumeIsolated { SelfCheck.run() })
}
OspreyViewHost.main()
