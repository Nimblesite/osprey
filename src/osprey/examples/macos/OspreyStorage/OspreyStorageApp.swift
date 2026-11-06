import SwiftUI
import UniformTypeIdentifiers

// Implements [IOS-TARGET-ENTRY], [IOS-TARGET-MACOS] and [MOBILE-NATIVE-HOST]:
// one SwiftUI app for the Mac and iPhone. Osprey owns the browser and every
// screen; Swift owns the window, the folder picker and the disk walk.
@main
struct OspreyStorageApp: App {
    @StateObject private var store = StorageStore()

    init() {
        if CommandLine.arguments.contains("--smoke") { exit(SmokeCheck.run() ? 0 : 1) }
    }

    var body: some Scene {
        WindowGroup {
            content
                .fileImporter(isPresented: $store.picking, allowedContentTypes: [.folder]) { store.picked($0) }
                .task { store.start() }
        }
        #if os(macOS)
        .defaultSize(width: 780, height: 860)
        #endif
    }

    @ViewBuilder private var content: some View {
        if let failure = store.failure {
            Text(failure).foregroundStyle(.red).padding()
        } else if let ui = store.ui {
            NativeRenderer(node: ui, send: store.send)
        } else {
            ProgressView()
        }
    }
}
