
import Foundation
import OspreyShared
import SwiftUI

// Implements [NATIVE-HOSTS-CONTRACT] and [NATIVE-SHM-NODES]: the macOS entry point. It opens the
// application's library over the shared frame and shows the root region with this host's native
// components. No application state, screen or policy lives here. `--self-test <script>` plays a
// script against the real window and compares its pictures with goldens ([UI-MACHINE-SELF-TEST]);
// `--open-check` and `--view-check` open an application with no window at all. The same library
// hosts iOS: Sources/OspreyViewHost/IOSApp.swift.

// [UI-MACHINE-SELF-TEST] A self-test owns the process: it plays its script and exits with the verdict.
@MainActor
private func runSelfTest(_ options: SelfTestOptions) {
    do {
        try FileManager.default.createDirectory(atPath: options.shots, withIntermediateDirectories: true)
        let store = options.store.map { StoreAdapter(root: URL(fileURLWithPath: $0, isDirectory: true)) } ?? bundleStore()
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

struct HostApp: App {
    @StateObject private var store = makeStore(arguments: HostLaunch.arguments(from: Array(CommandLine.arguments.dropFirst())),
                                               adapters: Adapters.standard(store: bundleStore()))

    var body: some Scene {
        WindowGroup(Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Osprey") {
            RootView(store: store).frame(minWidth: 860, minHeight: 560)
        }
        .defaultSize(width: 1320, height: 860)
        .windowStyle(.hiddenTitleBar)
    }
}

// Launch arguments belong to the application; AppKit would otherwise read paths among them as documents to open, and open no window. A string, not a Boolean: the open panel service reads this default as text and crashes on any other type.
UserDefaults.standard.set("NO", forKey: "NSTreatUnknownArgumentsAsOpen")

let launch = HostLaunch.ready(from: Array(CommandLine.arguments.dropFirst()))
if CommandLine.arguments.contains("--open-check") {
    exit(MainActor.assumeIsolated { SelfCheck.opened() })
}
if CommandLine.arguments.contains("--view-check") {
    // The main thread is this process's UI thread already; no window is opened.
    exit(MainActor.assumeIsolated { SelfCheck.run() })
}
do {
    if let options = try SelfTestOptions.parse(launch) {
        MainActor.assumeIsolated { runSelfTest(options) }
    } else {
        HostApp.main()
    }
} catch {
    print("FAILED: \(error)")
    exit(1)
}
