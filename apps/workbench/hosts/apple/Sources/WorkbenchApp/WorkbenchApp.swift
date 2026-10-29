import OspreyApp
import OspreyNative
import SwiftUI

// Implements [UI-HOST] and [WORKBENCH-STORE] for the Mac: the entry point. It
// binds the library's exports, answers the slice import, makes the bundled
// lessons the working folder so the application's relative paths resolve, and
// shows the root region. No application state, screen or policy lives here.

/// Answers Osprey's `workbench_slice` import with the host-held slice `key`; the string stays valid for the process.
@_cdecl("workbench_slice")
func workbenchSlice(_ key: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    SliceStore.shared.pointer(key.map { String(cString: $0) } ?? "")
}

@main
struct WorkbenchApp: App {
    @StateObject private var store = WorkbenchApp.makeStore()

    var body: some Scene {
        WindowGroup("Osprey Workbench") {
            RootView(store: store).frame(minWidth: 1000, minHeight: 680)
        }
        .defaultSize(width: 1440, height: 920)
    }

    @MainActor
    private static func makeStore() -> RegionStore {
        do { try FontRegistration.registerBundled() }
        catch { fatalError("The bundled mathematical fonts could not load: \(error)") }
        if let resources = Bundle.main.resourcePath { FileManager.default.changeCurrentDirectoryPath(resources) }
        let entry = LibraryEntry(main: osprey_main, start: osprey_app_start, dispatch: osprey_app_dispatch)
        let bundle = Bundle.main.bundleIdentifier ?? "org.ospreylang.workbench"
        let adapters = Adapters.standard(store: StoreAdapter.standard(bundle: bundle)) + [RecogniseAdapter()]
        let runner = CapabilityRunner(adapters: adapters)
        return RegionStore(library: Library(entry: entry), context: HostContext(platform: "macos", home: NSHomeDirectory(), arguments: Array(CommandLine.arguments.dropFirst())), capabilities: runner)
    }
}
