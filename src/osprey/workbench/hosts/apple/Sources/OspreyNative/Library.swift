import Foundation

// Implements [UI-BOUNDARY]: the C boundary to the Osprey library. The library
// is initialized once and called only from the main actor. Every string it
// returns is copied before borrowed inputs expire. ABI version 1 copies
// imports into Osprey ownership; its host input snapshot lives through the
// call and output copy. Unmarked legacy libraries retain process-lived inputs.

public typealias StartFunction = @convention(c) (UnsafePointer<CChar>?) -> UnsafePointer<CChar>?
public typealias DispatchFunction = @convention(c) (UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> UnsafePointer<CChar>?
public typealias MainFunction = @convention(c) () -> Int32
public typealias OwnershipVersionFunction = @convention(c) () -> Int32
public typealias ReleaseStringFunction = @convention(c) (UnsafePointer<CChar>?) -> Void

/// Library entry points and optional ownership capability from the generated header.
public struct LibraryEntry: Sendable {
    public let main: MainFunction
    public let start: StartFunction
    public let dispatch: DispatchFunction
    public let ownershipVersion: OwnershipVersionFunction?
    public let releaseString: ReleaseStringFunction?

    public init(main: MainFunction, start: StartFunction, dispatch: DispatchFunction, ownershipVersion: OwnershipVersionFunction? = nil, releaseString: ReleaseStringFunction? = nil) {
        self.main = main
        self.start = start
        self.dispatch = dispatch
        self.ownershipVersion = ownershipVersion
        self.releaseString = releaseString
    }
}

public enum LibraryError: Error, Equatable {
    case initialization(Int32)
    case noResult
    case eventTooLarge(Int)
    case ownershipVersion(Int32)
    case missingRelease
}

@MainActor
public final class Library {
    /// Events larger than this are refused before the call, as the protocol requires the host to bound input.
    public static let eventLimit = 16_000_000
    private let entry: LibraryEntry
    private var started = false
    private var copiedImports = false

    public init(entry: LibraryEntry) {
        self.entry = entry
    }

    public func initialize() throws {
        guard !started else { return }
        if let version = entry.ownershipVersion?() {
            guard version == 1 else { throw LibraryError.ownershipVersion(version) }
            guard entry.releaseString != nil else { throw LibraryError.missingRelease }
            copiedImports = true
        }
        let status = try scoped { entry.main() }
        guard status == 0 else { throw LibraryError.initialization(status) }
        started = true
    }

    public func start(context: String) throws -> String {
        try initialize()
        return try scoped { try context.withCString { try copy(entry.start($0)) } }
    }

    public func dispatch(state: String, event: String) throws -> String {
        try initialize()
        guard event.utf8.count <= Library.eventLimit else { throw LibraryError.eventTooLarge(event.utf8.count) }
        return try scoped {
            try state.withCString { saved in
                try event.withCString { try copy(entry.dispatch(saved, $0)) }
            }
        }
    }

    private func scoped<T>(_ body: () throws -> T) throws -> T {
        if copiedImports { return try SliceStore.shared.withSnapshot(body) }
        return try body()
    }

    private func copy(_ pointer: UnsafePointer<CChar>?) throws -> String {
        guard let pointer else { throw LibraryError.noResult }
        defer { entry.releaseString?(pointer) }
        return String(cString: pointer)
    }
}

/// Host-held slices by key. Osprey reads them through its slice import while a call is in progress.
public final class SliceStore: @unchecked Sendable {
    public static let shared = SliceStore()
    private let lock = NSLock()
    private var texts: [String: String] = [:]
    private var buffers: [String: UnsafeMutablePointer<CChar>] = [:]
    private var retained: [UnsafeMutablePointer<CChar>] = []
    private static let emptyBuffer = strdup("")!
    private var pending: Set<String>?
    private var snapshot: SliceReadSnapshot?

    public init() {}

    private init(snapshot: [String: String]) {
        texts = snapshot
        pending = []
    }

    /// Capabilities read a stable input snapshot and stage their own output writes.
    func staged() -> SliceStore {
        lock.lock()
        defer { lock.unlock() }
        return SliceStore(snapshot: texts)
    }

    /// Include attempted identical writes: a newer request still owns that output.
    func publications() -> [String: String] {
        lock.lock()
        defer { lock.unlock() }
        return Dictionary(uniqueKeysWithValues: (pending ?? []).compactMap { key in texts[key].map { (key, $0) } })
    }

    public func text(_ key: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        return texts[key] ?? ""
    }

    public func keys() -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return texts.keys.sorted()
    }

    public func write(_ key: String, _ text: String) {
        lock.lock()
        defer { lock.unlock() }
        pending?.insert(key)
        // Swift String equality normalizes Unicode; imported bytes must remain exact.
        if texts[key]?.utf8.elementsEqual(text.utf8) == true { return }
        texts[key] = text
        if let old = buffers.removeValue(forKey: key) { retained.append(old) }
    }

    /// A pinned call-snapshot string, or a process-lived buffer for an unmarked legacy library.
    public func pointer(_ key: String) -> UnsafePointer<CChar> {
        lock.lock()
        defer { lock.unlock() }
        if let snapshot { return snapshot.pointer(key) }
        if let buffer = buffers[key] { return UnsafePointer(buffer) }
        guard let text = texts[key], let buffer = strdup(text) else { return UnsafePointer(Self.emptyBuffer) }
        buffers[key] = buffer
        return UnsafePointer(buffer)
    }

    /// Bytes held for superseded values, so growth is measurable ([UI-BOUNDARY]).
    public var retainedBytes: Int {
        lock.lock()
        defer { lock.unlock() }
        return retained.reduce(0) { $0 + strlen($1) + 1 }
    }

    /// Imports read one immutable frame; concurrent publications belong to the next frame.
    func withSnapshot<T>(_ body: () throws -> T) throws -> T {
        try beginSnapshot()
        defer { endSnapshot() }
        return try body()
    }

    private func beginSnapshot() throws {
        lock.lock()
        defer { lock.unlock() }
        guard snapshot == nil else { throw SliceStoreError.readInProgress }
        snapshot = try SliceReadSnapshot(texts)
    }

    private func endSnapshot() {
        lock.lock()
        defer { lock.unlock() }
        snapshot = nil
    }

    public var borrowedBytes: Int {
        lock.lock()
        defer { lock.unlock() }
        return snapshot?.byteCount ?? 0
    }
}

public enum SliceStoreError: Error, Equatable {
    case readInProgress
    case allocation
}

private final class SliceReadSnapshot {
    private let buffers: [String: UnsafeMutablePointer<CChar>]
    private let empty: UnsafeMutablePointer<CChar>

    init(_ texts: [String: String]) throws {
        empty = try Self.copy("")
        do { buffers = try Self.copied(texts) }
        catch { free(empty); throw error }
    }

    deinit {
        buffers.values.forEach { free($0) }
        free(empty)
    }

    private static func copy(_ text: String) throws -> UnsafeMutablePointer<CChar> {
        guard let pointer = strdup(text) else { throw SliceStoreError.allocation }
        return pointer
    }

    private static func copied(_ texts: [String: String]) throws -> [String: UnsafeMutablePointer<CChar>] {
        var copies: [String: UnsafeMutablePointer<CChar>] = [:]
        do {
            for (key, text) in texts { copies[key] = try copy(text) }
            return copies
        } catch {
            copies.values.forEach { free($0) }
            throw error
        }
    }

    func pointer(_ key: String) -> UnsafePointer<CChar> {
        UnsafePointer(buffers[key] ?? empty)
    }

    var byteCount: Int { buffers.values.reduce(1) { $0 + strlen($1) + 1 } }
}
