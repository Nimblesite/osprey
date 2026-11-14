import Foundation
import OspreyViewApp

// Implements [NATIVE-SHM-OWNERSHIP], [NATIVE-SHM-PUBLISH] and [NATIVE-SHM-EVENT] on the host's
// side of the bridge: one instance, and one frame sealed after each application call. Every
// export — `open`, `event`, even an event the application refused — writes a whole frame, and
// the host always commits it and shows it; the four sealed runs (scene rows, view rows, text and
// ask rows) are leased, read, checked and copied into typed values, so no lease outlives the
// call that made it and nothing the application wrote is trusted unchecked.

/// A bridge call that did not answer zero, with the call that made it.
public struct SharedFailure: Error, CustomStringConvertible {
    public let call: String
    public let status: Int32

    public init(call: String, status: Int32) {
        self.call = call
        self.status = status
    }

    public var description: String { "\(call) returned \(status)" }
}

/// Owns the bridge the application writes its frames into.
public final class SharedBridge {
    /// Everything the bridge may hold at once, counting frames a painter still leases.
    private static let bytes: Int64 = 512 << 20
    private static let chunks: Int64 = 1024
    private static let owners: Int64 = 4096
    /// How many runs one frame seals: scene rows, view rows, text and ask rows.
    private static let frameRuns = 4

    private var bridge: OpaquePointer?

    /// Opens the single instance the application writes its frames into. The
    /// instance is random, so a stale pointer from another launch refuses.
    public init() throws {
        var instance = td_ApplicationInstance()
        withUnsafeMutableBytes(of: &instance) { storage in
            for index in storage.indices { storage[index] = UInt8.random(in: .min ... .max) }
        }
        var opened: OpaquePointer?
        try checked(osp_native_open(&instance, TD_SHARED_FINGERPRINT, Self.bytes, Self.chunks, Self.owners, &opened), "open")
        bridge = opened
    }

    /// Runs the application's top level; its rows belong to no frame, so they are dropped.
    public func main() throws {
        try abi()
        let status = osprey_main()
        SliceStore.shared.releaseBorrowed()
        osp_native_frame_discard()
        guard status == 0 else { throw SharedFailure(call: "osprey_main", status: status) }
    }

    /// Hands the application one launch argument, in order, before `open`. The frame such a
    /// call writes is committed and checked, but no view is adopted from it.
    public func argument(_ text: String) throws {
        _ = try call("argument", { text.utf8CString.withUnsafeBufferPointer { osprey_app_argument($0.baseAddress!) } })
    }

    /// The first frame, for the host's clock, zone offset, platform, home folder and appearance.
    public func open(now: Int64, offset: Int64, platform: String, home: String, dark: Bool) throws -> Shown {
        try call("open", {
            platform.utf8CString.withUnsafeBufferPointer { platformC in
                home.utf8CString.withUnsafeBufferPointer { homeC in
                    osprey_app_open(now, offset, platformC.baseAddress!, homeC.baseAddress!, dark)
                }
            }
        })
    }

    /// The frame after one host event at the clock reading `now`. A refused event still drew a
    /// whole frame — the same view, with its reason among the frame's ask rows.
    public func event(now: Int64, touch: Touch) throws -> Shown {
        try call("event", {
            touch.words.utf8CString.withUnsafeBufferPointer { words in
                touch.detail.utf8CString.withUnsafeBufferPointer { detail in
                    osprey_app_event(now, touch.kind, touch.ordinal, words.baseAddress!,
                                     detail.baseAddress!, touch.x, touch.y, touch.down, touch.shift)
                }
            }
        })
    }

    /// Closes the instance behind it.
    public func close() {
        if let open = bridge {
            _ = osp_native_close(open)
            _ = osp_native_destroy(open)
            bridge = nil
        }
    }

    deinit { close() }

    /// Runs one export, commits the frame it wrote and returns it checked. The export's own
    /// answer is not an error: a refused event drew the same view again, and its frame shows.
    private func call(_ name: String, _ export: () throws -> Int64) throws -> Shown {
        try abi()
        defer { SliceStore.shared.releaseBorrowed() }
        _ = try export()
        var revision: Int64 = 0
        try checked(osp_native_frame_commit(bridge, &revision), "\(name): frame commit")
        let frame = try leased()
        defer { frame.forEach { release($0.lease) } }
        return try SharedRows.read(shapes: try rows(frame, 0, OspNativeShape.self),
                                   views: try rows(frame, 1, OspNativeView.self),
                                   text: try text(frame),
                                   asks: try rows(frame, 3, OspNativeAsk.self))
    }

    /// The ownership ABI this host speaks; anything else is refused before a call is made.
    private func abi() throws {
        let version = osprey_host_abi_version()
        guard version == 1 else { throw SharedFailure(call: "osprey_host_abi_version", status: version) }
    }

    /// The four sealed runs of the frame just committed, with their leases. The root itself is
    /// released once its children are taken; a run that cannot be leased fails the frame whole,
    /// releasing everything taken before it.
    private func leased() throws -> [(lease: td_OwnedLease, pointer: UnsafeRawPointer?, length: Int64)] {
        var root = td_OwnedLease()
        var revision: Int64 = 0
        try checked(osp_native_acquire(bridge, &root, &revision), "acquire")
        var taken: [(lease: td_OwnedLease, pointer: UnsafeRawPointer?, length: Int64)] = []
        do {
            for run in 0..<Self.frameRuns { taken.append(try child(of: root, run)) }
        } catch {
            taken.forEach { release($0.lease) }
            release(root)
            throw error
        }
        release(root)
        return taken
    }

    /// The UTF-8 text run every row's offsets index into, borrowed in place.
    private func text(_ frame: [(lease: td_OwnedLease, pointer: UnsafeRawPointer?, length: Int64)]) throws -> UnsafeRawBufferPointer {
        guard let base = frame[2].pointer else { return UnsafeRawBufferPointer(start: nil, count: 0) }
        return UnsafeRawBufferPointer(start: base, count: min(Int(max(frame[2].length, 0)), Int(OSP_NATIVE_TEXT_LIMIT)))
    }

    /// The rows of run `index` as `T`; a run that is not a whole number of rows refuses the frame.
    private func rows<T>(_ frame: [(lease: td_OwnedLease, pointer: UnsafeRawPointer?, length: Int64)],
                         _ index: Int, _ row: T.Type) throws -> UnsafeBufferPointer<T> {
        let run = frame[index]
        guard run.length > 0, let base = run.pointer else { return UnsafeBufferPointer(start: nil, count: 0) }
        let stride = Int64(MemoryLayout<T>.stride)
        guard run.length % stride == 0 else {
            throw SharedFailure(call: "a run that is not a whole number of rows", status: 1 + TD_BridgeError_InvalidRange_TAG)
        }
        let count = Int(run.length / stride)
        let capped = index == 1 ? min(count, Int(OSP_NATIVE_VIEW_LIMIT)) : count
        return UnsafeBufferPointer(start: base.assumingMemoryBound(to: T.self), count: capped)
    }

    /// One new owner of the `run`th child, with its bytes borrowed in place.
    private func child(of root: td_OwnedLease, _ run: Int) throws -> (lease: td_OwnedLease, pointer: UnsafeRawPointer?, length: Int64) {
        var lease = td_OwnedLease()
        var owner = root
        try checked(osp_native_child(bridge, &owner, Int64(run), &lease), "child")
        var length: Int64 = 0
        var pointer: UnsafeRawPointer?
        do {
            try checked(osp_native_length(bridge, &lease, &length), "length")
            try checked(osp_native_view(bridge, &lease, Int64(run) + 1, 0, length, &pointer), "view")
        } catch {
            release(lease)
            throw error
        }
        return (lease, pointer, max(length, 0))
    }

    private func release(_ lease: td_OwnedLease) {
        var held = lease
        osp_native_release(bridge, &held)
    }

    private func checked(_ status: Int32, _ call: String) throws {
        guard status == OSP_NATIVE_OK else { throw SharedFailure(call: call, status: status) }
    }
}
