import Foundation

// Implements [UI-EFFECTS]: each capability code maps to one mechanical adapter that runs off the
// main actor, is bounded by the limits Osprey named and can be cancelled by ticket. Adapters
// take the typed `Ask` the frame's ask rows became, return raw platform facts as their result
// document — the same document the JSON host produced, as text — or a typed error; they decide
// nothing.

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

/// A capability's result as the application reads it: the result document as compact JSON text,
/// or the bare path a chooser completes with ([NATIVE-SHM-ASK]).
public indirect enum ResultDocument: Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([ResultDocument])
    case object([String: ResultDocument])

    /// Serialized as compact JSON text with sorted keys, so equal values encode equally.
    public var text: String {
        switch self {
        case .null: return "null"
        case let .bool(value): return value ? "true" : "false"
        case let .number(value): return value == value.rounded() && abs(value) < 9e15 ? String(Int(value)) : String(value)
        case let .string(value): return ResultDocument.quote(value)
        case let .array(items): return "[" + items.map(\.text).joined(separator: ",") + "]"
        case let .object(members):
            let entries = members.sorted { $0.key < $1.key }
            return "{" + entries.map { ResultDocument.quote($0.key) + ":" + $0.value.text }.joined(separator: ",") + "}"
        }
    }

    public var string: String? { if case let .string(value) = self { return value } else { return nil } }
    public var number: Double? { if case let .number(value) = self { return value } else { return nil } }
    public var object: [String: ResultDocument]? { if case let .object(value) = self { return value } else { return nil } }
    public var array: [ResultDocument]? { if case let .array(value) = self { return value } else { return nil } }

    /// JSON string quoting through Foundation, so escapes are never hand-written.
    public static func quote(_ value: String) -> String {
        let data = try? JSONSerialization.data(withJSONObject: [value], options: [.withoutEscapingSlashes])
        let wrapped = data.flatMap { String(data: $0, encoding: .utf8) } ?? "[\"\"]"
        return String(wrapped.dropFirst().dropLast())
    }
}

/// Literals spell documents the way adapters build them; duplicate keys keep the later value.
extension ResultDocument: ExpressibleByDictionaryLiteral, ExpressibleByArrayLiteral, ExpressibleByStringLiteral,
                            ExpressibleByBooleanLiteral, ExpressibleByIntegerLiteral, ExpressibleByFloatLiteral,
                            ExpressibleByNilLiteral {
    public init(dictionaryLiteral elements: (String, ResultDocument)...) {
        self = .object(Dictionary(elements, uniquingKeysWith: { _, later in later }))
    }

    public init(arrayLiteral elements: ResultDocument...) { self = .array(elements) }
    public init(stringLiteral value: String) { self = .string(value) }
    public init(booleanLiteral value: Bool) { self = .bool(value) }
    public init(integerLiteral value: Int) { self = .number(Double(value)) }
    public init(floatLiteral value: Double) { self = .number(value) }
    public init(nilLiteral: ()) { self = .null }

    /// A text value, so adapters build documents from computed strings as plainly as literals.
    public init(_ value: String) { self = .string(value) }
}

/// What an adapter hands its runner: a result document, or a chooser's bare path.
public enum AskResult: Sendable, Equatable {
    case document(ResultDocument)
    case bare(String)

    /// The completion's text: the document as JSON text, or the path as it is.
    public var text: String {
        switch self {
        case let .document(document): return document.text
        case let .bare(path): return path
        }
    }
}

/// An adapter writes staged slices; only its live successful completion publishes them.
public protocol CapabilityAdapter: Sendable {
    var kinds: [String] { get }
    func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult
    /// A completed result nobody will receive, because its request was cancelled or replaced first.
    func discarded(_ ask: Ask, _ value: AskResult)
}

public extension CapabilityAdapter {
    func discarded(_ ask: Ask, _ value: AskResult) {}
}

/// The argument a required text is missing from, as its error spells it.
func missing(_ ask: Ask, _ name: String) -> CapabilityError {
    .invalidArgument("\(ask.kind) has no valid \(name)")
}

/// A required text at argument `field`, refusing an ask that carries none.
public func required(_ ask: Ask, text field: Int64, _ name: String) throws -> String {
    guard ask.fields.contains(where: { $0.field == field }) else { throw missing(ask, name) }
    return ask.text(field)
}

/// Required texts at argument `field`, refusing an ask that carries none; empty texts count.
public func required(_ ask: Ask, texts field: Int64, _ name: String) throws -> [String] {
    guard ask.fields.contains(where: { $0.field == field }) else { throw missing(ask, name) }
    return ask.texts(field)
}

/// A required whole number at argument `field`, refusing an ask that carries none; the adapter
/// checks its range.
public func required(_ ask: Ask, whole field: Int64, _ name: String) throws -> Int {
    guard let held = ask.fields.first(where: { $0.field == field }) else { throw missing(ask, name) }
    return Int(held.number)
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

    public func knows(_ kind: String) -> Bool { kind == "cancel" || adapters[kind] != nil }

    public func run(_ ask: Ask, completion: @escaping @MainActor (Result<AskResult, CapabilityError>) -> Void) {
        if ask.kind == "cancel" { return cancel(ask, completion: completion) }
        guard let adapter = adapters[ask.kind] else { return completion(.failure(.unknownKind(ask.kind))) }
        // [UI-EFFECTS]: cancelled/late writes never reach the core.
        inflight.removeValue(forKey: ask.ticket)?.task.cancel()
        let store = slices.staged(), identity = UUID()
        let task = Task { [weak self] in
            let outcome = await CapabilityRunner.outcome(adapter, ask, store)
            let published = self?.publish(ask.ticket, identity, store, outcome, Task.isCancelled) ?? .failure(.cancelled)
            if case let .success(value) = outcome, case .failure = published { adapter.discarded(ask, value) }
            completion(published)
        }
        inflight[ask.ticket] = (identity, task)
    }

    private func publish(_ ticket: String, _ identity: UUID, _ staged: SliceStore,
                         _ outcome: Result<AskResult, CapabilityError>, _ cancelled: Bool) -> Result<AskResult, CapabilityError> {
        guard inflight[ticket]?.identity == identity else { return .failure(.cancelled) }
        inflight.removeValue(forKey: ticket)
        guard !cancelled else { return .failure(.cancelled) }
        if case .success = outcome { staged.publications().forEach { slices.write($0.key, $0.value) } }
        return outcome
    }

    private func cancel(_ ask: Ask, completion: @escaping @MainActor (Result<AskResult, CapabilityError>) -> Void) {
        inflight.removeValue(forKey: ask.text(1))?.task.cancel()
        completion(.success(.document(.object([:]))))
    }

    private nonisolated static func outcome(_ adapter: CapabilityAdapter, _ ask: Ask, _ store: SliceStore) async -> Result<AskResult, CapabilityError> {
        do { return .success(try await adapter.perform(ask, slices: store)) } catch let problem as CapabilityError { return .failure(problem) } catch is CancellationError { return .failure(.cancelled) } catch { return .failure(.platform(error.localizedDescription)) }
    }
}

public enum Bounded {
    /// Reads a regular file of at most `limit` bytes without following a symbolic link.
    public static func read(_ path: String, limit: Int) throws -> Data {
        try snapshot(path, limit: limit).data
    }

    /// A bounded read of `leaf` inside an already pinned directory ([UI-EFFECTS]):
    /// the leaf is opened descriptor-relative with O_NOFOLLOW, so a link there is
    /// refused rather than followed; `nil` when the entry does not exist.
    static func read(parent: Int32, leaf: String, limit: Int) throws -> Data? {
        guard limit >= 0, limit < Int.max else { throw CapabilityError.invalidArgument("Invalid file byte limit") }
        try Task.checkCancellation()
        let descriptor = openat(parent, leaf, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
        guard descriptor >= 0 else {
            if errno == ENOENT { return nil }
            if errno == ELOOP { throw CapabilityError.notAllowed("\(leaf) is a link, not a store file") }
            throw CapabilityError.platform(String(cString: strerror(errno)))
        }
        defer { close(descriptor) }
        let status = try identity(descriptor)
        guard (status.st_mode & S_IFMT) == S_IFREG else { throw CapabilityError.notAllowed("\(leaf) is not a regular file") }
        guard status.st_size <= limit else { throw CapabilityError.tooLarge(Int(status.st_size), limit) }
        let data = try contents(FileHandle(fileDescriptor: descriptor, closeOnDealloc: false), limit: limit)
        guard unchanged(status, try identity(descriptor)), status.st_size == data.count else { throw CapabilityError.platform("The file changed while it was read") }
        return data
    }

    /// Mechanical facts from the same open file; pathname replacement invalidates the read.
    public static func snapshot(_ path: String, limit: Int) throws -> (data: Data, identity: stat) {
        guard limit >= 0, limit < Int.max else { throw CapabilityError.invalidArgument("Invalid file byte limit") }
        try Task.checkCancellation()
        // [UI-EFFECTS]: opening a FIFO must not wait before fstat rejects it.
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

    // [UI-EFFECTS]: never trust the initial size to bound a later read.
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

    public static func text(_ data: Data) throws -> String {
        guard let text = String(data: data, encoding: .utf8) else { throw CapabilityError.invalidArgument("Not UTF-8 text") }
        return text
    }
}
