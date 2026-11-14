#if os(iOS)
import UIKit
import UniformTypeIdentifiers

// [CAST-MOBILE-DELIVERY] Security-scoped documents are copied into app storage before Osprey
// sees their paths; cancelling the picker never changes the file. A chooser completes with the
// bare path, or nothing at all when it was cancelled ([NATIVE-SHM-ASK]).
public struct DocumentAdapter: CapabilityAdapter {
    public let kinds = ["choose-file"]
    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let types = ask.texts(1).compactMap { UTType(filenameExtension: $0) }
        guard !types.isEmpty else { throw CapabilityError.invalidArgument("No supported file types") }
        return .bare(try await DocumentPicker.choose(types))
    }
}

@MainActor
private final class DocumentPicker: NSObject, @MainActor UIDocumentPickerDelegate {
    private static var active: DocumentPicker?
    private let picker: UIDocumentPickerViewController
    private var continuation: CheckedContinuation<String, Error>?

    private init(_ types: [UTType]) {
        picker = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: true)
        super.init()
        picker.delegate = self
        picker.allowsMultipleSelection = false
    }

    static func choose(_ types: [UTType]) async throws -> String {
        guard active == nil else { throw CapabilityError.notAllowed("A file picker is already open") }
        let coordinator = DocumentPicker(types)
        active = coordinator
        defer { active = nil }
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return try await withCheckedThrowingContinuation { continuation in
                coordinator.present(continuation)
            }
        } onCancel: {
            Task { @MainActor in coordinator.finish(.failure(CapabilityError.cancelled)) }
        }
    }

    private func present(_ continuation: CheckedContinuation<String, Error>) {
        self.continuation = continuation
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard var root = scene?.windows.first(where: \.isKeyWindow)?.rootViewController else {
            finish(.failure(CapabilityError.platform("No foreground window for the file picker")))
            return
        }
        while let presented = root.presentedViewController { root = presented }
        root.present(picker, animated: true)
    }

    private func finish(_ result: Result<String, Error>) {
        let waiting = continuation
        continuation = nil
        picker.dismiss(animated: true)
        waiting?.resume(with: result)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(.success(""))
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard continuation != nil else { return }
        guard let source = urls.first else {
            finish(.failure(CapabilityError.platform("The file picker returned no document")))
            return
        }
        do {
            let folder = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                .appendingPathComponent("SelectedMedia", isDirectory: true)
                .appendingPathComponent(UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let target = folder.appendingPathComponent(source.lastPathComponent)
            let scoped = source.startAccessingSecurityScopedResource()
            defer { if scoped { source.stopAccessingSecurityScopedResource() } }
            try FileManager.default.copyItem(at: source, to: target)
            finish(.success(target.path))
        } catch { finish(.failure(CapabilityError.platform("Cannot import the selected file: \(error.localizedDescription)"))) }
    }
}
#elseif os(macOS)
import AppKit
import UniformTypeIdentifiers

// The system's open and save panels: a chooser completes with the bare path the person chose,
// or "" when they cancelled ([NATIVE-SHM-ASK]). A chosen file's bytes, within the ask's limit, are
// kept in the slice it names, and a saved file is written from its slice, so the application
// reads and exports content without touching the file system. Panels run on the main actor.
public struct DocumentAdapter: CapabilityAdapter {
    public let kinds = ["choose-file", "choose-folder", "save-file"]
    private let panel: @MainActor @Sendable (Ask) -> String

    /// The system's panels, or `panel` standing in for the person who answers them.
    public init(panel: @escaping @MainActor @Sendable (Ask) -> String = { (try? DocumentAdapter.choose($0)) ?? "" }) {
        self.panel = panel
    }

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let path = await MainActor.run { panel(ask) }
        guard !path.isEmpty else { return .bare("") }
        switch ask.kind {
        case "choose-file": try keep(ask, path, slices)
        case "save-file": try persist(ask, path, slices)
        default: break
        }
        return .bare(path)
    }

    /// The chosen file's text in the slice the ask names, refused past its byte limit.
    private func keep(_ ask: Ask, _ path: String, _ slices: SliceStore) throws {
        let limit = try required(ask, whole: 2, "limit")
        guard limit > 0 else { throw CapabilityError.invalidArgument("Invalid file byte limit") }
        slices.write(try required(ask, text: 3, "slice"), try Bounded.text(try Bounded.read(path, limit: limit)))
    }

    /// The named slice's text, written to the file the person chose.
    private func persist(_ ask: Ask, _ path: String, _ slices: SliceStore) throws {
        let key = try required(ask, text: 2, "slice")
        guard slices.keys().contains(key) else { throw CapabilityError.invalidArgument("Save requested the absent host slice \(key)") }
        try Data(slices.text(key).utf8).write(to: URL(fileURLWithPath: path), options: .atomic)
    }

    /// The panel this ask opens; `nil` — an empty completion — when it was cancelled.
    @MainActor
    public static func choose(_ ask: Ask) throws -> String {
        switch ask.kind {
        case "choose-file": return try openPanel(ask)
        case "choose-folder": return try folderPanel()
        default: return try savePanel(ask)
        }
    }

    private static func openPanel(_ ask: Ask) throws -> String {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = ask.texts(1).compactMap { UTType(filenameExtension: $0) }
        return panel.runModal() == .OK ? panel.url?.path ?? "" : ""
    }

    private static func folderPanel() throws -> String {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        return panel.runModal() == .OK ? panel.url?.path ?? "" : ""
    }

    private static func savePanel(_ ask: Ask) throws -> String {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = ask.text(1)
        return panel.runModal() == .OK ? panel.url?.path ?? "" : ""
    }
}
#endif
