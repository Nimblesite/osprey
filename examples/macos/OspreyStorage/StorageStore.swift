import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

private struct StorageCommand: Decodable {
    let kind: String
    let path: String
    var graft: Bool?
    var apps: [String]?
}

private struct StorageEnvelope: Decodable {
    let model: String
    let ui: NativeNode
    let commands: [StorageCommand]
}

// Implements [MOBILE-REACTIVE-HOST]: keep Osprey's opaque browser, show the
// screen it returns, and do the host work it asks for. Nothing here decides
// what the screen holds.
@MainActor
final class StorageStore: ObservableObject {
    @Published private(set) var ui: NativeNode?
    @Published private(set) var failure: String?
    @Published var picking = false
    private var model = ""
    private var walk: Task<Void, Never>?
    private var scoped: URL?
    #if os(macOS)
    static let platform = "macos"
    #else
    static let platform = "ios"
    #endif

    func start() {
        guard ui == nil, failure == nil else { return }
        let status = osprey_main()
        guard status == 0 else { return fail("Osprey initialization failed: \(status)") }
        #if os(iOS)
        deviceModel = UIDevice.current.model
        #endif
        accept(osprey_storage_start(canonical(NSHomeDirectory()), StorageStore.platform, fullDiskAccess()))
    }

    func send(_ event: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: event),
            let json = String(data: data, encoding: .utf8)
        else { return fail("An event could not be encoded for Osprey") }
        accept(osprey_storage_dispatch(model, json))
    }

    /// A folder chosen in the system picker. On iPhone it stays readable only
    /// while its security scope is held, so the scope lasts until the next pick.
    func picked(_ result: Result<URL, Error>) {
        guard case let .success(url) = result else { return }
        scoped?.stopAccessingSecurityScopedResource()
        scoped = url.startAccessingSecurityScopedResource() ? url : nil
        send(["type": "picked", "value": canonical(url.path)])
    }

    private func accept(_ pointer: UnsafePointer<CChar>?) {
        guard let pointer else { return fail("Osprey returned a screen that could not be read") }
        // Implements [IOS-HOST-ABI]: release owned results even when decoding fails.
        defer { osprey_release_string(pointer) }
        guard let envelope = try? JSONDecoder().decode(StorageEnvelope.self, from: Data(String(cString: pointer).utf8))
        else { return fail("Osprey returned a screen that could not be read") }
        model = envelope.model
        ui = envelope.ui
        envelope.commands.forEach(run)
    }

    private func run(_ command: StorageCommand) {
        switch command.kind {
        case "measure": measure(command)
        case "pick": pick(command.path)
        case "reveal": reveal(command.path)
        case "photos": PhotoLibrary.measure { [weak self] in self?.send(["type": "photos"]) }
        case "settings": settings()
        default: fail("Osprey asked for unknown host work: \(command.kind)")
        }
    }

    /// The system folder picker. On the Mac it opens on `path`, so granting a
    /// protected folder is one click; choosing it is the person's consent, and
    /// macOS shows no prompt.
    private func pick(_ path: String) {
        #if os(macOS)
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.directoryURL = URL(fileURLWithPath: path, isDirectory: true)
        panel.begin { [weak self] response in
            if response == .OK, let url = panel.url { self?.picked(.success(url)) }
        }
        #else
        picking = true
        #endif
    }

    /// A Finder window on `path`. Finder reads the folder, not this app, so a
    /// protected folder opens without a prompt here.
    private func reveal(_ path: String) {
        #if os(macOS)
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: path)
        #endif
    }

    /// The privacy settings Osprey points to: Full Disk Access on the Mac, the
    /// app's own page, with its Photos access, on iPhone.
    private func settings() {
        #if os(macOS)
        let page = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        if let url = URL(string: page) { NSWorkspace.shared.open(url) }
        #else
        if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
        #endif
    }

    /// Walks off the main thread; every result goes back to Osprey as an event.
    private func measure(_ command: StorageCommand) {
        let root = command.path, graft = command.graft == true
        let guarding = DiskTree.Guard(apps: Set(command.apps ?? []))
        walk?.cancel()
        walk = Task.detached(priority: .userInitiated) { [weak self] in
            let tree = DiskTree.measure(root, guarding: guarding) { items, bytes in
                Task { @MainActor in self?.send(["type": "progress", "path": root, "items": items, "bytes": bytes]) }
            }
            await self?.finish(root, graft, tree)
        }
    }

    private func finish(_ root: String, _ graft: Bool, _ tree: DiskTree?) {
        guard !Task.isCancelled else { return }
        if let tree {
            if graft, let current = DiskTree.current { current.graft(tree, at: root) } else { DiskTree.current = tree }
        }
        send(["type": tree == nil ? "failed" : "measured", "path": root])
    }

    private func fail(_ message: String) {
        failure = message
        walk?.cancel()
    }
}
