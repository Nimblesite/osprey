import Foundation

// [NATIVE-HOSTS-TRANSPORT] Generic C ABI: initialization and calls run on the main actor.
// Results are copied, then released only through an explicitly supplied ABI.
// Legacy entries never free results. ABI v1 slice buffers end with each call;
// legacy and unscoped slice borrows retain their process lifetime.
// Returned-string release and imported-buffer ownership are separate contracts.

public typealias StartFunction = @convention(c) (UnsafePointer<CChar>?) -> UnsafePointer<CChar>?
public typealias DispatchFunction = @convention(c) (UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> UnsafePointer<CChar>?
public typealias MainFunction = @convention(c) () -> Int32
public typealias ReleaseStringFunction = @convention(c) (UnsafePointer<CChar>?) -> Void

/// Entry points and optional result release, bound from a verified generated header.
public struct LibraryEntry: Sendable {
    public let main: MainFunction
    public let start: StartFunction
    public let dispatch: DispatchFunction
    /// [NATIVE-HOSTS-TRANSPORT] Only for an ABI declaring owned exported strings.
    public let releaseString: ReleaseStringFunction?
    /// [NATIVE-HOSTS-TRANSPORT] Explicit version marker; unknown ABIs are refused.
    public let hostABIVersion: MainFunction?

    public init(main: MainFunction, start: StartFunction, dispatch: DispatchFunction, releaseString: ReleaseStringFunction? = nil, hostABIVersion: MainFunction? = nil) {
        self.main = main
        self.start = start
        self.dispatch = dispatch
        self.releaseString = releaseString
        self.hostABIVersion = hostABIVersion
    }
}

public enum LibraryError: Error, Equatable {
    case initialization(Int32)
    case noResult
    case eventTooLarge(Int)
    case unsupportedABI(Int32)
    case missingStringRelease
    case incompatibleImportLifetime
    case reentrantCall
}

@MainActor
public final class Library {
    /// Events larger than this are refused before the call, as the protocol requires the host to bound input.
    public static let eventLimit = 16_000_000
    private let entry: LibraryEntry
    private let slices: SliceStore
    private var started = false
    private var calling = false

    public init(entry: LibraryEntry, slices: SliceStore = .shared) {
        self.entry = entry
        self.slices = slices
    }

    public func initialize() throws {
        guard !started else { return }
        let status = try invoke { entry.main() }
        guard status == 0 else { throw LibraryError.initialization(status) }
        started = true
    }

    public func start(context: String) throws -> String {
        try initialize()
        return try invoke { try copy(context.withCString { entry.start($0) }) }
    }

    public func dispatch(state: String, event: String) throws -> String {
        try initialize()
        guard event.utf8.count <= Library.eventLimit else { throw LibraryError.eventTooLarge(event.utf8.count) }
        return try invoke { try copy(state.withCString { saved in event.withCString { entry.dispatch(saved, $0) } }) }
    }

    private func invoke<T>(_ operation: () throws -> T) throws -> T {
        guard !calling else { throw LibraryError.reentrantCall }
        calling = true
        defer { calling = false }
        let version = entry.hostABIVersion?()
        if let version, version != 1 { throw LibraryError.unsupportedABI(version) }
        if version == 1 && entry.releaseString == nil { throw LibraryError.missingStringRelease }
        try slices.beginCall(copiedImports: version == 1)
        defer { slices.endCall() }
        return try operation()
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
    private var copiedImports: Bool?
    private var borrowing = false
    private static let emptyBuffer: UnsafeMutablePointer<CChar> = {
        let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: 1)
        buffer.initialize(to: 0)
        return buffer
    }()
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

    /// Legacy borrows live for the process; ABI v1 imports last through the enclosing call.
    public func pointer(_ key: String) -> UnsafePointer<CChar> {
        lock.lock()
        defer { lock.unlock() }
        if !borrowing || !Thread.isMainThread { copiedImports = false }
        if let buffer = buffers[key] { return UnsafePointer(buffer) }
        guard let text = texts[key], let buffer = strdup(text) else { return UnsafePointer(Self.emptyBuffer) }
        buffers[key] = buffer
        return UnsafePointer(buffer)
    }

    // [NATIVE-HOSTS-TRANSPORT] Select a lifetime before the first callback.
    func beginCall(copiedImports requested: Bool) throws {
        lock.lock()
        defer { lock.unlock() }
        guard !borrowing else { throw LibraryError.reentrantCall }
        guard copiedImports == nil || copiedImports == requested else { throw LibraryError.incompatibleImportLifetime }
        copiedImports = requested
        borrowing = true
    }

    // ABI v1 owns independent copies when its enclosing synchronous call ends.
    func endCall() {
        lock.lock()
        defer { lock.unlock() }
        if copiedImports == true {
            for buffer in Array(buffers.values) + retained { free(buffer) }
            buffers.removeAll()
            retained.removeAll()
        }
        borrowing = false
    }

    /// Bytes held for superseded values, so growth is measurable ([NATIVE-HOSTS-TRANSPORT]).
    public var retainedBytes: Int {
        lock.lock()
        defer { lock.unlock() }
        return retained.reduce(0) { $0 + strlen($1) + 1 }
    }

    /// All host-owned C slice bytes, including current and superseded buffers.
    public var borrowedBytes: Int {
        lock.lock()
        defer { lock.unlock() }
        return (Array(buffers.values) + retained).reduce(0) { $0 + strlen($1) + 1 }
    }
}
