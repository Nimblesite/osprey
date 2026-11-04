#if os(iOS)
import UIKit
import UniformTypeIdentifiers

// [CAST-MOBILE-DELIVERY] Security-scoped documents are copied into app storage
// before Osprey sees their paths; cancelling the picker never changes the file.
public struct DocumentAdapter: CapabilityAdapter {
    public let kinds = ["choose-file"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let types = try request.texts("types").compactMap { UTType(filenameExtension: $0) }
        guard !types.isEmpty else { throw CapabilityError.invalidArgument("No supported file types") }
        return try await DocumentPicker.choose(types)
    }
}

@MainActor
private final class DocumentPicker: NSObject, @MainActor UIDocumentPickerDelegate {
    private static var active: DocumentPicker?
    private let picker: UIDocumentPickerViewController
    private var continuation: CheckedContinuation<JSONValue, Error>?

    private init(_ types: [UTType]) {
        picker = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: true)
        super.init()
        picker.delegate = self
        picker.allowsMultipleSelection = false
    }

    static func choose(_ types: [UTType]) async throws -> JSONValue {
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

    private func present(_ continuation: CheckedContinuation<JSONValue, Error>) {
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

    private func finish(_ result: Result<JSONValue, Error>) {
        let waiting = continuation
        continuation = nil
        picker.dismiss(animated: true)
        waiting?.resume(with: result)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(.success(.object(["cancelled": .bool(true), "path": .string("")])))
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
            finish(.success(.object(["cancelled": .bool(false), "path": .string(target.path)])))
        } catch { finish(.failure(CapabilityError.platform("Cannot import the selected file: \(error.localizedDescription)"))) }
    }
}
#endif
