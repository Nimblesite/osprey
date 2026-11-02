import OspreyApp
import OspreyNative
import SwiftUI

// Implements [UI-HOST] and [UI-PROTOCOL]: the entry point. It binds the
// application library's exports, answers the slice import and shows the root
// region. No application state, screen or policy lives here.

/// Answers Osprey's `native_slice` import with the host-held slice `key`.
@_cdecl("native_slice")
func nativeSlice(_ key: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    SliceStore.shared.pointer(key.map { String(cString: $0) } ?? "")
}

struct HostApp: App {
    @StateObject private var store = HostApp.makeStore()

    var body: some Scene {
        WindowGroup(Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Osprey") {
            RootView(store: store).frame(minWidth: 860, minHeight: 560)
        }
        .defaultSize(width: 1320, height: 860)
        .windowStyle(.hiddenTitleBar)
    }

    @MainActor
    private static func makeStore() -> RegionStore {
        let entry = LibraryEntry(main: osprey_main, start: osprey_app_start, dispatch: osprey_app_dispatch,
                                 releaseString: osprey_release_string, hostABIVersion: osprey_host_abi_version)
        let bundle = Bundle.main.bundleIdentifier ?? "org.ospreylang.host"
        let runner = CapabilityRunner(adapters: Adapters.standard(store: StoreAdapter.standard(bundle: bundle)))
        return RegionStore(library: Library(entry: entry), context: HostContext(platform: "macos", home: NSHomeDirectory(), arguments: Array(CommandLine.arguments.dropFirst())), capabilities: runner)
    }
}

// Launch arguments belong to the application; AppKit would otherwise read paths among them as documents to open, and open no window.
UserDefaults.standard.set(false, forKey: "NSTreatUnknownArgumentsAsOpen")
HostApp.main()
