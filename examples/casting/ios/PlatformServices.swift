import Foundation
import UIKit
import UniformTypeIdentifiers

// [GPUI-HOST-CAPABILITIES] The OS-service boundary the shared Rust GPUI host
// calls on a background thread: one JSON request in, one owned JSON answer
// out. Every failure is a typed `{ok:false,error}` answer; the main thread is
// never blocked waiting on main-queue work, and every wait is finite.
// Implements [CAST-MOBILE-DELIVERY]: native Bonjour discovery and the scoped
// document picker; no process execution, no player UI.

/// One boundary answer as plain JSON members.
private typealias Boundary = [String: Any]

/// The browse wait when the request names none; also its longest accepted wait.
private let browseMs = 5_000
private let browseCeilingMs = 60_000
/// A person may take their time in the picker; the wait is still finite.
private let pickerMs = 600_000

@_cdecl("osprey_gpui_capability")
public func osprey_gpui_capability(_ request: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let request else { return nil }
    // Synchronous main-queue work from the main thread would deadlock; refuse it.
    guard !Thread.isMainThread else { return owned(failed("Capabilities cannot be answered on the main thread")) }
    return owned(answered(String(cString: request)))
}

@_cdecl("osprey_gpui_capability_free")
public func osprey_gpui_capability_free(_ answer: UnsafeMutablePointer<CChar>?) {
    if let answer { free(UnsafeMutableRawPointer(answer)) }
}

/// One owned UTF-8 answer; an allocation failure answers nil for Rust to report.
private func owned(_ boundary: Boundary) -> UnsafeMutablePointer<CChar>? {
    guard let data = try? JSONSerialization.data(withJSONObject: boundary, options: [.sortedKeys]) else { return nil }
    return strdup(String(decoding: data, as: UTF8.self))
}

private func succeeded(result: [String: Any], slices: [[String: Any]] = []) -> Boundary {
    ["ok": true, "result": result, "slices": slices]
}

private func failed(_ problem: String) -> Boundary {
    ["ok": false, "error": problem]
}

private func sliceEntry(_ key: String, _ text: String) -> [String: Any] {
    ["key": key, "text": text]
}

private func answered(_ text: String) -> Boundary {
    guard let members = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] else {
        return failed("The capability request is not a JSON object")
    }
    switch members["kind"] as? String {
    case "run": return browsed(members)
    case "choose-file": return choseFile(members)
    case let .some(kind): return failed("unsupported capability \(kind)")
    case nil: return failed("The capability request names no kind")
    }
}

/// The single supported run: the `discover` ticket staging `cast-browse`
/// ([CAST-MOBILE-DELIVERY]). Every other run is refused; iOS launches nothing.
private func browsed(_ members: [String: Any]) -> Boundary {
    do {
        let ticket = try member(members, "ticket", as: "text", { $0 as? String })
        let slice = try member(members, "slice", as: "text", { $0 as? String }) ?? ""
        guard ticket == "discover", slice == "cast-browse" else {
            return failed("Mobile casting does not execute processes")
        }
        let timeoutMs = try browseWait(members)
        return awaited(CastBrowseService(slice: slice), timeoutMs: timeoutMs)
    } catch {
        return failed(error.localizedDescription)
    }
}

/// The browse wait: `browseMs` when the request names none, and a typed refusal
/// for any named wait outside the host's 1...`browseCeilingMs` range.
private func browseWait(_ members: [String: Any]) throws -> Int {
    let named = try member(members, "timeout", as: "a whole number of milliseconds", wholeNumber)
        ?? browseMs
    guard named >= 1, named <= browseCeilingMs else {
        throw PlatformFailure.detail("The requested \(named) ms wait is outside 1...\(browseCeilingMs) ms")
    }
    return named
}

/// The single-file picker: only requested extensions, copied into app
/// Documents before Osprey ever sees the path ([CAST-MOBILE-DELIVERY]).
private func choseFile(_ members: [String: Any]) -> Boundary {
    do {
        let wanted = try member(members, "types", as: "a list of extensions", { $0 as? [String] }) ?? []
        guard !wanted.isEmpty, wanted.allSatisfy({ UTType(filenameExtension: $0) != nil }) else {
            return failed("No supported file types")
        }
        let limit = try member(members, "limit", as: "a whole number", wholeNumber) ?? 0
        let slice = try member(members, "slice", as: "text", { $0 as? String }) ?? ""
        let service = FilePickService(extensions: wanted, limit: limit, slice: slice)
        return awaited(service, timeoutMs: pickerMs)
    } catch {
        return failed(error.localizedDescription)
    }
}

/// A whole non-negative JSON number below 9e15, as the Swift host accepts. A
/// JSON boolean is not a number here, although Foundation bridges it as one.
private func wholeNumber(_ value: Any) -> Int? {
    guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
          number.doubleValue.rounded() == number.doubleValue,
          number.doubleValue >= 0, number.doubleValue < 9e15 else { return nil }
    return Int(number.doubleValue)
}

/// One request member read for a service: absent is `nil`, while a supplied
/// value of another shape is a typed error rather than a silent default.
private func member<T>(_ members: Boundary, _ name: String, as shape: String,
                       _ convert: (Any) -> T?) throws -> T? {
    guard let value = members[name] else { return nil }
    guard let converted = convert(value) else {
        throw PlatformFailure.detail("The request's \(name) is not \(shape)")
    }
    return converted
}

/// The first answer through the boundary; a later completion is dropped.
private final class Waiter: @unchecked Sendable {
    private let lock = NSLock()
    private let arrived = DispatchSemaphore(value: 0)
    private var boundary: Boundary?

    func finish(_ boundary: Boundary) {
        lock.lock()
        let first = self.boundary == nil
        if first { self.boundary = boundary }
        lock.unlock()
        if first { arrived.signal() }
    }

    func answer(timeoutMs: Int) -> Boundary? {
        _ = arrived.wait(timeout: .now() + .milliseconds(max(timeoutMs, 1)))
        lock.lock()
        defer { lock.unlock() }
        return boundary
    }
}

/// Work the main actor performs until it completes exactly once; `stop` ends
/// it without an answer reaching a waiter that already timed out.
@MainActor
private protocol MainService {
    func start(completion: @escaping (Boundary) -> Void)
    func stop()
}

/// Schedules one service on the main queue and waits for its answer within
/// `timeoutMs`; a timeout stops the service and answers a typed error.
private func awaited<S: MainService>(_ service: S, timeoutMs: Int) -> Boundary {
    let waiter = Waiter()
    DispatchQueue.main.async {
        MainActor.assumeIsolated { service.start { waiter.finish($0) } }
    }
    if let boundary = waiter.answer(timeoutMs: timeoutMs) { return boundary }
    DispatchQueue.main.async { MainActor.assumeIsolated { service.stop() } }
    return failed("Timed out after \(timeoutMs) ms")
}

/// One typed platform failure at the document-import boundary; its message is
/// the text the host reports, never a generic description.
private enum PlatformFailure: Error, LocalizedError {
    case detail(String)

    var errorDescription: String? {
        switch self {
        case let .detail(problem): return problem
        }
    }
}

/// The `discover` run answered by native Bonjour discovery.
@MainActor
private final class CastBrowseService: MainService {
    private let discovery = CastDiscovery()
    private let slice: String

    nonisolated init(slice: String) { self.slice = slice }

    func start(completion: @escaping (Boundary) -> Void) {
        discovery.start { result in
            switch result {
            case let .success(transcript): completion(succeeded(result: [:], slices: [sliceEntry(self.slice, transcript)]))
            case let .failure(problem): completion(failed(problem.localizedDescription))
            }
        }
    }

    func stop() { discovery.cancel() }
}

/// The native document picker ([CAST-MOBILE-DELIVERY]): one at a time, kept
/// alive until the person picks, cancels or the wait runs out.
@MainActor
private final class FilePickService: NSObject, MainService, @MainActor UIDocumentPickerDelegate {
    private static var active: FilePickService?
    private var picker: UIDocumentPickerViewController?
    private let wanted: [String]
    private let limit: Int
    private let slice: String
    private var completion: ((Boundary) -> Void)?

    nonisolated init(extensions: [String], limit: Int, slice: String) {
        self.wanted = extensions
        self.limit = limit
        self.slice = slice
        super.init()
    }

    func start(completion: @escaping (Boundary) -> Void) {
        guard Self.active == nil else { return completion(failed("A file picker is already open")) }
        Self.active = self
        self.completion = completion
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: wanted.compactMap { UTType(filenameExtension: $0) }, asCopy: true)
        picker.delegate = self
        picker.allowsMultipleSelection = false
        self.picker = picker
        present(picker)
    }

    func stop() { finish(failed("Cancelled")) }

    private func present(_ picker: UIDocumentPickerViewController) {
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard var root = scene?.windows.first(where: \.isKeyWindow)?.rootViewController else {
            finish(failed("No foreground window for the file picker"))
            return
        }
        while let presented = root.presentedViewController { root = presented }
        root.present(picker, animated: true)
    }

    private func finish(_ boundary: Boundary) {
        guard let waiting = completion else { return }
        completion = nil
        if Self.active === self { Self.active = nil }
        picker?.dismiss(animated: true)
        waiting(boundary)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(succeeded(result: ["cancelled": true, "path": ""]))
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard completion != nil else { return }
        guard let source = urls.first else { return finish(failed("The file picker returned no document")) }
        finish(imported(source))
    }

    private func imported(_ source: URL) -> Boundary {
        guard wanted.contains(where: { $0.caseInsensitiveCompare(source.pathExtension) == .orderedSame }) else {
            return failed("\(source.lastPathComponent) is not one of \(wanted.joined(separator: ", "))")
        }
        switch copied(source) {
        case let .success(target): return published(target)
        case let .failure(.detail(problem)): return failed(problem)
        }
    }

    /// Copies the scoped original into app Documents; an unreadable source is a typed error.
    private func copied(_ source: URL) -> Result<URL, PlatformFailure> {
        let scoped = source.startAccessingSecurityScopedResource()
        defer { if scoped { source.stopAccessingSecurityScopedResource() } }
        do {
            let folder = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                .appendingPathComponent("ChosenMedia", isDirectory: true)
                .appendingPathComponent(UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let target = folder.appendingPathComponent(source.lastPathComponent)
            try FileManager.default.copyItem(at: source, to: target)
            return .success(target)
        } catch {
            return .failure(.detail("Cannot import the selected file: \(error.localizedDescription)"))
        }
    }

    /// The chosen answer; a named slice with a positive limit also reads its bounded text.
    private func published(_ target: URL) -> Boundary {
        var entries: [[String: Any]] = []
        if !slice.isEmpty, limit > 0 {
            switch readText(target.path, limit: limit) {
            case let .success(text): entries = [sliceEntry(slice, text)]
            case let .failure(.detail(problem)): return failed(problem)
            }
        }
        return succeeded(result: ["cancelled": false, "path": target.path], slices: entries)
    }
}

/// The largest block one bounded read asks for at a time.
private let readChunkBytes = 64 * 1024

/// Reads at most `limit` UTF-8 bytes from the already imported copy, mirroring
/// the host's bounded read: a link, special file, read or close failure, an
/// over-large file or non-text bytes is a typed error, never an empty success.
private func readText(_ path: String, limit: Int) -> Result<String, PlatformFailure> {
    guard isRegularFile(path), let handle = FileHandle(forReadingAtPath: path) else {
        return .failure(.detail("Cannot read the chosen file"))
    }
    let read = attempted("Cannot read the chosen file") { try boundedRead(handle, limit: limit) }
    let closed = attempted("Cannot close the chosen file") { try handle.close() }
    // A read failure came first, so it is what the host reports; the close is
    // still attempted, and its own failure is reported when the read succeeded.
    return read.flatMap { data in closed.flatMap { _ in text(data, limit: limit) } }
}

/// Reads at most `limit` bytes plus the one byte that proves the file is larger,
/// so the reported size matches the host's own bounded read exactly.
private func boundedRead(_ handle: FileHandle, limit: Int) throws -> Data {
    var data = Data()
    while data.count <= limit {
        let want = min(readChunkBytes, limit + 1 - data.count)
        guard let chunk = try handle.read(upToCount: want), !chunk.isEmpty else { break }
        data.append(chunk)
    }
    return data
}

/// Validates one bounded read's size and UTF-8 text.
private func text(_ data: Data, limit: Int) -> Result<String, PlatformFailure> {
    guard data.count <= limit else {
        return .failure(.detail("\(data.count) bytes exceeds the \(limit) byte limit"))
    }
    guard let decoded = String(data: data, encoding: .utf8) else {
        return .failure(.detail("Not UTF-8 text"))
    }
    return .success(decoded)
}

/// Runs one throwing step, naming its typed failure.
private func attempted<T>(_ problem: String, _ step: () throws -> T) -> Result<T, PlatformFailure> {
    do {
        return .success(try step())
    } catch {
        return .failure(.detail("\(problem): \(error.localizedDescription)"))
    }
}

/// Whether `path` is a regular file, refusing links and special files exactly as
/// the host's `O_NOFOLLOW` open does, so a read can never block on a device.
private func isRegularFile(_ path: String) -> Bool {
    let attributes = try? FileManager.default.attributesOfItem(atPath: path)
    return attributes?[.type] as? FileAttributeType == .typeRegular
}
