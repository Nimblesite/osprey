import Foundation

// Implements [UI-EFFECTS]: each request kind maps to one mechanical adapter
// that runs off the main actor, is bounded by the limits Osprey named and can
// be cancelled by ticket. Adapters return raw platform facts or a typed error;
// they decide nothing.

public enum CapabilityError: Error, Equatable, CustomStringConvertible {
    case unknownKind(String)
    case invalidArgument(String)
    case notAllowed(String)
    case tooLarge(Int, Int)
    case timedOut(Int)
    case platform(String)
    case cancelled

    public var description: String {
        switch self {
        case let .unknownKind(kind): return "Unknown capability \(kind)"
        case let .invalidArgument(detail): return detail
        case let .notAllowed(detail): return detail
        case let .tooLarge(size, limit): return "\(size) bytes exceeds the \(limit) byte limit"
        case let .timedOut(milliseconds): return "Timed out after \(milliseconds) ms"
        case let .platform(detail): return detail
        case .cancelled: return "Cancelled"
        }
    }
}

public typealias Outcome = Result<JSONValue, CapabilityError>

/// An adapter writes staged slices; only its live successful completion publishes them.
public protocol CapabilityAdapter: Sendable {
    var kinds: [String] { get }
    func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue
}

@MainActor
public final class CapabilityRunner {
    private let adapters: [String: CapabilityAdapter]
    private let slices: SliceStore
    private var inflight: [String: (identity: UUID, task: Task<Void, Never>)] = [:]

    public init(adapters: [CapabilityAdapter], slices: SliceStore = .shared) {
        var table: [String: CapabilityAdapter] = [:]
        adapters.forEach { adapter in adapter.kinds.forEach { table[$0] = adapter } }
        self.adapters = table
        self.slices = slices
    }

    public var kinds: [String] { adapters.keys.sorted() }

    public func run(_ request: Request, completion: @escaping @MainActor (Outcome) -> Void) {
        if request.kind == "cancel" { return cancel(request, completion: completion) }
        guard let adapter = adapters[request.kind] else { return completion(.failure(.unknownKind(request.kind))) }
        // [UI-EFFECTS-RECEIPT-OWNERSHIP]: cancelled/late writes never reach the core.
        inflight.removeValue(forKey: request.ticket)?.task.cancel()
        let store = slices.staged(), identity = UUID()
        let task = Task { [weak self] in
            let outcome = await CapabilityRunner.outcome(adapter, request, store)
            completion(self?.publish(request.ticket, identity, store, outcome, Task.isCancelled) ?? .failure(.cancelled))
        }
        inflight[request.ticket] = (identity, task)
    }

    private func publish(_ ticket: String, _ identity: UUID, _ staged: SliceStore, _ outcome: Outcome, _ cancelled: Bool) -> Outcome {
        guard inflight[ticket]?.identity == identity else { return .failure(.cancelled) }
        inflight.removeValue(forKey: ticket)
        guard !cancelled else { return .failure(.cancelled) }
        if case .success = outcome { staged.publications().forEach { slices.write($0.key, $0.value) } }
        return outcome
    }

    private func cancel(_ request: Request, completion: @escaping @MainActor (Outcome) -> Void) {
        do {
            inflight.removeValue(forKey: try request.text("target"))?.task.cancel()
            completion(.success(.object([:])))
        } catch {
            completion(.failure(.platform(error.localizedDescription)))
        }
    }

    private nonisolated static func outcome(_ adapter: CapabilityAdapter, _ request: Request, _ store: SliceStore) async -> Outcome {
        do { return .success(try await adapter.perform(request, slices: store)) } catch let problem as CapabilityError { return .failure(problem) } catch is CancellationError { return .failure(.cancelled) } catch { return .failure(.platform(error.localizedDescription)) }
    }
}

enum Bounded {
    /// Reads a regular file of at most `limit` bytes without following a symbolic link.
    static func read(_ path: String, limit: Int) throws -> Data {
        try snapshot(path, limit: limit).data
    }

    /// Mechanical facts from the same open file; pathname replacement invalidates the read.
    static func snapshot(_ path: String, limit: Int) throws -> (data: Data, identity: stat) {
        guard limit >= 0, limit < Int.max else { throw CapabilityError.invalidArgument("Invalid file byte limit") }
        try Task.checkCancellation()
        // [UI-EFFECTS-BOUNDED-READ]: opening a FIFO must not wait before fstat rejects it.
        let descriptor = open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
        guard descriptor >= 0 else { throw CapabilityError.platform(String(cString: strerror(errno))) }
        defer { close(descriptor) }
        let status = try identity(descriptor)
        guard (status.st_mode & S_IFMT) == S_IFREG else { throw CapabilityError.notAllowed("\(path) is not a regular file") }
        guard status.st_size <= limit else { throw CapabilityError.tooLarge(Int(status.st_size), limit) }
        let data = try contents(FileHandle(fileDescriptor: descriptor, closeOnDealloc: false), limit: limit)
        let final = try identity(descriptor)
        guard unchanged(status, final), linked(path, to: final), final.st_size == data.count else { throw CapabilityError.platform("The file changed while it was read") }
        return (data, final)
    }

    private static func identity(_ descriptor: Int32) throws -> stat {
        var value = stat()
        guard fstat(descriptor, &value) == 0 else { throw CapabilityError.platform(String(cString: strerror(errno))) }
        return value
    }

    private static func unchanged(_ before: stat, _ after: stat) -> Bool {
        before.st_dev == after.st_dev && before.st_ino == after.st_ino && before.st_size == after.st_size
        && before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec && before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec
        && before.st_ctimespec.tv_sec == after.st_ctimespec.tv_sec && before.st_ctimespec.tv_nsec == after.st_ctimespec.tv_nsec
    }

    private static func linked(_ path: String, to opened: stat) -> Bool {
        var current = stat()
        return lstat(path, &current) == 0 && current.st_dev == opened.st_dev && current.st_ino == opened.st_ino && (current.st_mode & S_IFMT) == S_IFREG
    }

    // [UI-EFFECTS-BOUNDED-READ]: never trust the initial size to bound a later read.
    private static func contents(_ handle: FileHandle, limit: Int) throws -> Data {
        var data = Data()
        while true {
            try Task.checkCancellation()
            let chunk = try handle.read(upToCount: min(65536, limit - data.count + 1)) ?? Data()
            if chunk.isEmpty { return data }
            data.append(chunk)
            guard data.count <= limit else { throw CapabilityError.tooLarge(data.count, limit) }
        }
    }

    static func text(_ data: Data) throws -> String {
        guard let text = String(data: data, encoding: .utf8) else { throw CapabilityError.invalidArgument("Not UTF-8 text") }
        return text
    }
}
