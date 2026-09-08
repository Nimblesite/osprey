import SwiftUI

// Implements [MOBILE-NATIVE-HOST]. This shell has no inbox rules or layout.
@main
struct IssueInboxApp: App {
    @StateObject private var store = InboxStore()

    var body: some Scene {
        WindowGroup {
            Group {
                if let ui = store.ui {
                    NativeRenderer(node: ui, send: store.send)
                } else if let error = store.hostError {
                    Text(error).foregroundColor(.red).padding()
                } else {
                    ProgressView()
                }
            }
            .overlay(alignment: .bottom) {
                if let error = store.hostError {
                    Text(error).font(.caption).padding().background(.regularMaterial)
                }
            }
            .task {
                let arguments = ProcessInfo.processInfo.arguments
                if arguments.contains("--inbox-smoke") {
                    await HostSmoke.run()
                }
                store.start()
                if arguments.contains("--inbox-open-first") {
                    await store.waitUntilIdle()
                    store.openFirstIssue()
                }
            }
        }
    }
}
