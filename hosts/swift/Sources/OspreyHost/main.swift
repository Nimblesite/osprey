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

/// The running application's store, with the capabilities and arguments this launch asks for.
@MainActor
private func makeStore(arguments: [String], adapters: [CapabilityAdapter]) -> RegionStore {
    let entry = LibraryEntry(main: osprey_main, start: osprey_app_start, dispatch: osprey_app_dispatch,
                             releaseString: osprey_release_string, hostABIVersion: osprey_host_abi_version)
    return RegionStore(library: Library(entry: entry), context: HostContext(platform: "macos", home: NSHomeDirectory(), arguments: arguments), capabilities: CapabilityRunner(adapters: adapters))
}

@MainActor
private var bundleStore: StoreAdapter { StoreAdapter.standard(bundle: Bundle.main.bundleIdentifier ?? "org.ospreylang.host") }

struct HostApp: App {
    @StateObject private var store = makeStore(arguments: Array(CommandLine.arguments.dropFirst()), adapters: Adapters.standard(store: bundleStore))

    var body: some Scene {
        WindowGroup(Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Osprey") {
            RootView(store: store).frame(minWidth: 860, minHeight: 560)
        }
        .defaultSize(width: 1320, height: 860)
        .windowStyle(.hiddenTitleBar)
    }
}

// [UI-MACHINE-SELF-TEST] A self-test owns the process: it plays its script and exits with the verdict.
@MainActor
private func runSelfTest(_ options: SelfTestOptions) {
    do {
        try FileManager.default.createDirectory(atPath: options.shots, withIntermediateDirectories: true)
        let store = options.store.map { StoreAdapter(root: URL(fileURLWithPath: $0, isDirectory: true)) } ?? bundleStore
        var adapters = Adapters.standard(store: store)
        if let folder = options.folderDouble {
            adapters = adapters.filter { !$0.kinds.contains("choose-folder") } + [ChooseFolderDouble(path: folder)]
        }
        if let double = options.httpDouble {
            let requests = URL(fileURLWithPath: options.shots).appendingPathComponent("requests.jsonl")
            try? FileManager.default.removeItem(at: requests)
            adapters = adapters.filter { !$0.kinds.contains("http") } + [try HttpDoubleAdapter(file: double, requests: requests)]
        }
        let run = SelfTestRun(options: options, store: makeStore(arguments: options.appArguments, adapters: adapters))
        Task { @MainActor in
            let passed = await run.run()
            exit(passed ? 0 : 1)
        }
        NSApplication.shared.run()
    } catch {
        print("FAILED: \(error)")
        exit(1)
    }
}

// Launch arguments belong to the application; AppKit would otherwise read paths among them as documents to open, and open no window.
UserDefaults.standard.set(false, forKey: "NSTreatUnknownArgumentsAsOpen")
do {
    if let options = try SelfTestOptions.parse(Array(CommandLine.arguments.dropFirst())) {
        MainActor.assumeIsolated { runSelfTest(options) }
    } else {
        HostApp.main()
    }
} catch {
    print("FAILED: \(error)")
    exit(1)
}
