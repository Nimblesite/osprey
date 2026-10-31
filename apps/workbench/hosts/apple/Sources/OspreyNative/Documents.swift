#if os(iOS)
import SwiftUI
import UIKit
import UniformTypeIdentifiers

// Implements [UI-EFFECTS] on iOS: the document picker chooses a file and the
// share sheet saves one. Each needs a person's gesture and reports what was
// chosen or that nothing was. Nothing here reads, names or interprets a file.

/// The system document picker and share sheet, presented over the key window.
@MainActor
enum Documents {
    /// Copies the chosen file into the app's temporary folder and reports its path, or that nothing was chosen.
    static func choose(types: [String]) async throws -> JSONValue {
        let contentTypes = types.compactMap { UTType(filenameExtension: $0) }
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: contentTypes.isEmpty ? [.item] : contentTypes, asCopy: true)
        picker.allowsMultipleSelection = false
        let delegate = PickerDelegate()
        picker.delegate = delegate
        let presenter = try presenter()
        return await withCheckedContinuation { continuation in
            delegate.finish = { continuation.resume(returning: $0) }
            presenter.present(picker, animated: true)
        }
    }

    /// Offers `data` as the file `name` through the share sheet and reports where it was staged, or that sharing was cancelled.
    static func save(name: String, data: Data) async throws -> JSONValue {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-share-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let url = folder.appendingPathComponent(name.isEmpty ? "export" : name)
        try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        let sheet = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        let presenter = try presenter()
        sheet.popoverPresentationController?.sourceView = presenter.view
        sheet.popoverPresentationController?.sourceRect = CGRect(origin: CGPoint(x: presenter.view.bounds.midX, y: presenter.view.bounds.midY), size: .zero)
        return await withCheckedContinuation { continuation in
            sheet.completionWithItemsHandler = { _, completed, _, _ in
                continuation.resume(returning: .object(["path": .string(completed ? url.path : ""), "cancelled": .bool(!completed)]))
            }
            presenter.present(sheet, animated: true)
        }
    }

    /// The front-most view controller of the key window, which presents the system sheets.
    static func presenter() throws -> UIViewController {
        let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap(\.windows)
        guard var top = (windows.first(where: \.isKeyWindow) ?? windows.first)?.rootViewController else { throw CapabilityError.platform("No window to present from") }
        while let presented = top.presentedViewController { top = presented }
        return top
    }
}

/// Reports the picker's outcome once: the copied file's path, or a cancellation.
final class PickerDelegate: NSObject, UIDocumentPickerDelegate {
    var finish: ((JSONValue) -> Void)?

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        report(.object(["path": .string(urls.first?.path ?? ""), "cancelled": .bool(urls.isEmpty)]))
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        report(.object(["path": "", "cancelled": true]))
    }

    private func report(_ outcome: JSONValue) {
        finish?(outcome)
        finish = nil
    }
}
#endif
