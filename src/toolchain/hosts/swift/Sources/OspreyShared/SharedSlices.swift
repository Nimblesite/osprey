import Foundation

// Implements [NATIVE-SHM-ASK]'s slice half: the host keeps the text a slice write named under
// its key, and the application's runtime reads it back through its `native_slice` import while
// a call is in progress. Writers publish whole texts on the main thread; a reader borrows a
// pinned C copy that stays valid even once the text is written again.

/// Host-held slices by key.
public final class SliceStore: @unchecked Sendable {
    public static let shared = SliceStore()
    private let lock = NSLock()
    private var texts: [String: String] = [:]
    private var buffers: [String: UnsafeMutablePointer<CChar>] = [:]
    private var retained: [UnsafeMutablePointer<CChar>] = []
    private static let emptyBuffer: UnsafeMutablePointer<CChar> = {
        let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: 1)
        buffer.initialize(to: 0)
        return buffer
    }()

    public init() {}

    /// A stable snapshot of the current texts, for an adapter to read while it runs; writes it
    /// stages land only when its request completes successfully.
    func staged() -> SliceStore {
        lock.lock()
        defer { lock.unlock() }
        let copy = SliceStore()
        copy.texts = texts
        copy.pending = []
        return copy
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

    /// Frees every C copy once the synchronous call that borrowed them has returned: the runtime
    /// copies each import it reads ([NATIVE-SHM-ASK]), so no buffer outlives its call.
    public func releaseBorrowed() {
        lock.lock()
        defer { lock.unlock() }
        for buffer in Array(buffers.values) + retained { free(buffer) }
        buffers.removeAll()
        retained.removeAll()
    }

    /// All host-owned C slice bytes, current and superseded, so growth is measurable.
    public var borrowedBytes: Int {
        lock.lock()
        defer { lock.unlock() }
        return (Array(buffers.values) + retained).reduce(0) { $0 + strlen($1) + 1 }
    }

    /// A pinned C string the runtime can borrow; a buffer superseded during a call stays valid
    /// until that call returns.
    public func pointer(_ key: String) -> UnsafePointer<CChar> {
        lock.lock()
        defer { lock.unlock() }
        if let buffer = buffers[key] { return UnsafePointer(buffer) }
        guard let text = texts[key], let buffer = strdup(text) else { return UnsafePointer(Self.emptyBuffer) }
        buffers[key] = buffer
        return UnsafePointer(buffer)
    }

    private var pending: Set<String>?
}

/// Answers the application's `native_slice` import with the host-held slice `key`. The key an
/// application passes is borrowed for the call; missing keys answer empty, never null.
@_cdecl("native_slice")
public func nativeSlice(_ key: UnsafePointer<CChar>?) -> UnsafePointer<CChar> {
    SliceStore.shared.pointer(key.map { String(cString: $0) } ?? "")
}
