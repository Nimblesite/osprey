import Foundation
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW] on the host's side of the bridge: one instance,
// one frame sealed after each application call, and the two leases — the rows
// and their text — that every drawn control reads in place. Nothing is decoded
// into another format, copied or serialized, and a lease is released only once
// nothing can read it any more.

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

/// Owns the bridge and the leases of the view it is showing.
public final class SharedBridge {
    /// The controls of the held view, read in place; empty until the first frame.
    public private(set) var rows = UnsafeBufferPointer<OspNativeView>(start: nil, count: 0)
    /// The UTF-8 text the rows' offsets index into; valid as long as `rows`.
    public private(set) var text = UnsafeRawBufferPointer(start: nil, count: 0)

    private var bridge: OpaquePointer?
    private var rowLease: td_OwnedLease?
    private var textLease: td_OwnedLease?

    /// One leased run of a frame: its owner and the bytes that owner pins.
    private struct Run {
        var lease: td_OwnedLease
        var pointer: UnsafeRawPointer?
        var length: Int64
    }

    /// Opens the single instance the application writes its frame into.
    /// The instance is random, so a stale pointer from another launch refuses.
    public init(bytes: Int64 = 1 << 24, chunks: Int64 = 256, owners: Int64 = 256) throws {
        var instance = td_ApplicationInstance()
        withUnsafeMutableBytes(of: &instance) { storage in
            for index in storage.indices { storage[index] = UInt8.random(in: .min ... .max) }
        }
        var opened: OpaquePointer?
        try checked(osp_native_open(&instance, TD_SHARED_FINGERPRINT, bytes, chunks, owners, &opened), "open")
        bridge = opened
    }

    /// Runs one application call and leases the frame it wrote, answering
    /// whether a new view is held. A call that answers nonzero drops its partial
    /// frame, so the view it already showed keeps showing [NATIVE-SHM-VIEW].
    @discardableResult
    public func call(_ name: String, _ export: () -> Int64) throws -> Bool {
        guard export() == 0 else {
            osp_native_frame_discard()
            return false
        }
        var revision: Int64 = 0
        try checked(osp_native_frame_commit(bridge, &revision), "\(name): frame commit")
        let fresh = try leaseBoth()
        releaseView()
        adopt(fresh)
        return true
    }

    /// The text at `offset` for `length` bytes, or empty when they lie outside
    /// the leased run. Invalid UTF-8 is replaced rather than refused.
    public func words(offset: Int64, length: Int64) -> String {
        guard length > 0, offset >= 0, offset <= Int64(text.count),
              length <= Int64(text.count) - offset, let base = text.baseAddress else { return "" }
        return String(decoding: UnsafeRawBufferPointer(start: base + Int(offset), count: Int(length)), as: UTF8.self)
    }

    /// Releases both leases of the held view and the instance behind it.
    public func close() {
        releaseView()
        if let open = bridge {
            _ = osp_native_close(open)
            _ = osp_native_destroy(open)
            bridge = nil
        }
    }

    deinit { close() }

    /// Both runs of the frame just committed. The rows owner is released again
    /// if the text owner cannot be taken, so no lease outlives this call.
    private func leaseBoth() throws -> (Run, Run) {
        let rows = try lease(OSP_NATIVE_VIEWS)
        do {
            return (rows, try lease(OSP_NATIVE_TEXT))
        } catch {
            var lease = rows.lease
            release(&lease)
            throw error
        }
    }

    /// Takes an owner of the `run`th sealed run of the newest frame.
    private func lease(_ run: Int) throws -> Run {
        var frame = td_OwnedLease()
        var revision: Int64 = 0
        try checked(osp_native_acquire(bridge, &frame, &revision), "acquire")
        defer {
            var held = frame
            release(&held)
        }
        var owned = Run(lease: td_OwnedLease(), pointer: nil, length: 0)
        try checked(osp_native_child(bridge, &frame, Int64(run), &owned.lease), "child")
        try checked(osp_native_length(bridge, &owned.lease, &owned.length), "length")
        try checked(osp_native_view(bridge, &owned.lease, Int64(run) + 1, 0, owned.length, &owned.pointer), "view")
        return owned
    }

    /// Keeps the two fresh leases as the held view. A row run that is not a
    /// whole number of rows keeps only its whole rows [NATIVE-SHM-SAFETY].
    private func adopt(_ fresh: (Run, Run)) {
        rowLease = fresh.0.lease
        textLease = fresh.1.lease
        let size = MemoryLayout<OspNativeView>.stride
        let whole = fresh.0.length > 0 ? Int(fresh.0.length) / size : 0
        rows = UnsafeBufferPointer(start: fresh.0.pointer?.assumingMemoryBound(to: OspNativeView.self),
                                   count: min(whole, Int(OSP_NATIVE_VIEW_LIMIT)))
        text = UnsafeRawBufferPointer(start: fresh.1.pointer, count: fresh.1.length > 0 ? Int(fresh.1.length) : 0)
    }

    private func releaseView() {
        if var lease = rowLease { rowLease = nil; release(&lease) }
        if var lease = textLease { textLease = nil; release(&lease) }
        rows = UnsafeBufferPointer(start: nil, count: 0)
        text = UnsafeRawBufferPointer(start: nil, count: 0)
    }

    /// Releases one owner. A refusal is fail-stop inside the bridge, which keeps
    /// the storage pinned and refuses every later call, so there is no recovery.
    private func release(_ lease: inout td_OwnedLease) {
        osp_native_release(bridge, &lease)
    }

    private func checked(_ status: Int32, _ call: String) throws {
        guard status == OSP_NATIVE_OK else { throw SharedFailure(call: call, status: status) }
    }
}
