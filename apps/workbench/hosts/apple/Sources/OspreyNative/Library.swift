import Foundation

// Implements [UI-BOUNDARY]: the C boundary to the Osprey library. The library
// is initialized once and called only from the main actor. Every string it
// returns is copied at once. A string the host hands back through a slice
// import must stay valid for the rest of the process, so superseded slice
// buffers are retained, never freed.

public typealias StartFunction = @convention(c) (UnsafePointer<CChar>?) -> UnsafePointer<CChar>?
public typealias DispatchFunction = @convention(c) (UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> UnsafePointer<CChar>?
public typealias MainFunction = @convention(c) () -> Int32

/// The three library entry points, bound by the application target that imports the generated header.
public struct LibraryEntry: Sendable {
    public let main: MainFunction
    public let start: StartFunction
    public let dispatch: DispatchFunction

    public init(main: MainFunction, start: StartFunction, dispatch: DispatchFunction) {
        self.main = main
        self.start = start
        self.dispatch = dispatch
    }
}

public enum LibraryError: Error, Equatable {
    case initialization(Int32)
    case noResult
    case eventTooLarge(Int)
}

@MainActor
public final class Library {
    /// Events larger than this are refused before the call, as the protocol requires the host to bound input.
    public static let eventLimit = 16_000_000
    private let entry: LibraryEntry
    private var started = false

    public init(entry: LibraryEntry) {
        self.entry = entry
    }

    public func initialize() throws {
        guard !started else { return }
        let status = entry.main()
        guard status == 0 else { throw LibraryError.initialization(status) }
        started = true
    }

    public func start(context: String) throws -> String {
        try initialize()
        return try copy(context.withCString { entry.start($0) })
    }

    public func dispatch(state: String, event: String) throws -> String {
        try initialize()
        guard event.utf8.count <= Library.eventLimit else { throw LibraryError.eventTooLarge(event.utf8.count) }
        return try copy(state.withCString { saved in event.withCString { entry.dispatch(saved, $0) } })
    }

    private func copy(_ pointer: UnsafePointer<CChar>?) throws -> String {
        guard let pointer else { throw LibraryError.noResult }
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

    /// The C string for `key`, valid for the process; identical writes reuse its current buffer.
    public func pointer(_ key: String) -> UnsafePointer<CChar> {
        lock.lock()
        defer { lock.unlock() }
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
}
