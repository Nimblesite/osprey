import Foundation

// Implements [UI-EFFECTS-MEASURE] on the SwiftUI host, macOS and iOS alike, with Foundation
// and POSIX only: each named folder's children are measured to the bottom — allocated bytes
// (`st_blocks * 512`), a hard-linked file once per walk, links listed and never followed, the
// walk never leaving its start volume — within an entry limit and a deadline, and the result
// goes to the host-held slice Osprey named. The adapter sorts, hides, formats and decides
// nothing; the application does all of that.

/// The `measure-folders` capability: folders at field 1, the entry limit at 2, the deadline in
/// milliseconds at 3 and the slice at 4 [UI-EFFECTS-MEASURE]. The completion itself is `null`.
public struct MeasureAdapter: CapabilityAdapter {
    public let kinds = ["measure-folders"]

    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let limit = try required(ask, whole: 2, "entry limit")
        guard limit >= 0 else { throw CapabilityError.invalidArgument("Invalid measured entry limit") }
        let folders = try required(ask, texts: 1, "folders")
        let deadline = min(max(try required(ask, whole: 3, "deadline"), 0), MeasureBudget.maxDeadline)
        let report = try MeasureFolders.walk(folders, limit: min(limit, MeasureBudget.maxEntries), deadline: deadline)
        slices.write(try required(ask, text: 4, "slice"), report)
        return .document(.null)
    }
}

/// The walk of every folder one request named, in the order they were given.
enum MeasureFolders {
    /// The slice text `{"folders":[...]}`. The deadline runs from this call across all of them;
    /// past it nothing more is entered and what was measured is marked truncated.
    static func walk(_ folders: [String], limit: Int, deadline: Int) throws -> String {
        let budget = MeasureBudget(deadline: deadline)
        let measured = try folders.map { try one($0, limit: limit, budget: budget) }
        let report: ResultDocument = ["folders": .array(measured)]
        return report.text
    }

    /// One named folder: its volume, and either the walk of its children or why it was not read.
    static func one(_ path: String, limit: Int, budget: MeasureBudget) throws -> ResultDocument {
        let volume = MeasureVolume.of(path)
        if budget.expired { return MeasureWalker.walked(path, volume: volume, truncated: true) }
        let children: [MeasureChild]
        do { children = try MeasureWalker.children(path) } catch {
            return MeasureWalker.refused(path, volume: volume, problem: error)
        }
        let walker = MeasureWalker(budget: budget, device: volumeIdentity(path))
        return try walker.folder(path, children, limit: limit, volume: volume)
    }

    /// The start folder's volume identity, so the walk never leaves it; 0 when it cannot be asked.
    static func volumeIdentity(_ path: String) -> UInt64 {
        var status = stat()
        return stat(path, &status) == 0 ? UInt64(status.st_dev) : 0
    }
}

/// One child of a measured folder, as one `lstat` saw it: never followed, so a link is a link.
struct MeasureChild {
    let name: String
    let status: stat?

    /// Hidden as the platform's file manager hides: a leading dot, or `UF_HIDDEN` on Apple.
    var hidden: Bool { name.hasPrefix(".") || (status?.st_flags ?? 0) & UInt32(UF_HIDDEN) != 0 }

    var folder: Bool { kind == UInt32(S_IFDIR) }
    var link: Bool { kind == UInt32(S_IFLNK) }
    /// Allocated bytes, in the unit POSIX defines: 512-byte blocks.
    var allocated: Int64 { status.map { Int64($0.st_blocks) * 512 } ?? 0 }
    var key: MeasureFileKey? { status.map { MeasureFileKey(dev: UInt64($0.st_dev), ino: UInt64($0.st_ino)) } }
    func on(_ start: UInt64) -> Bool { status.map { UInt64($0.st_dev) == start } ?? false }

    private var kind: UInt32 { status.map { UInt32($0.st_mode & mode_t(S_IFMT)) } ?? 0 }
}

/// A hard-linked file's identity: its volume and its inode, so each file counts once per walk.
struct MeasureFileKey: Hashable {
    let dev: UInt64, ino: UInt64
}

/// The walk's shared state: its deadline from the request's start, cooperative cancellation
/// and the hard links already counted.
final class MeasureBudget {
    static let maxEntries = 100_000
    static let maxDeadline = 600_000

    private let started = Date()
    private let deadline: TimeInterval
    private var seen: Set<MeasureFileKey> = []

    init(deadline: Int) { self.deadline = TimeInterval(deadline) / 1000 }

    /// Stops the walk when the application cancelled its request.
    func gate() throws { try Task.checkCancellation() }

    var expired: Bool { Date().timeIntervalSince(started) >= deadline }

    /// The bytes a child adds where it is listed: none for a link, none for a hard link already
    /// counted, and a folder's own blocks.
    func own(_ child: MeasureChild) -> Int64 {
        if child.link { return 0 }
        if child.folder { return child.allocated }
        guard let key = child.key else { return 0 }
        return seen.insert(key).inserted ? child.allocated : 0
    }
}

/// Everything below one folder: allocated bytes, the bytes in sight, the items and the folders
/// that could not be read.
struct MeasureTally {
    var bytes: Int64 = 0, visible: Int64 = 0
    var items = 0, unreadable = 0

    mutating func add(_ other: MeasureTally) {
        bytes += other.bytes
        visible += other.visible
        items += other.items
        unreadable += other.unreadable
    }
}

/// One measured child as the slice lists it.
struct MeasureEntry {
    let name: String, path: String
    var bytes: Int64, visible: Int64, items: Int
    let hidden: Bool, folder: Bool, link: Bool
    var guarded: Bool

    /// A child `lstat` could not see is listed with no bytes and nothing below it.
    init(child: MeasureChild, path: String, bytes: Int64) {
        name = child.name
        self.path = path
        self.bytes = bytes
        visible = child.hidden ? 0 : bytes
        items = 0
        hidden = child.hidden
        folder = child.folder
        link = child.link
        guarded = child.status == nil
    }
}

/// The volume a folder is on: its name, capacity and what this user may still write; a folder
/// that cannot be asked leaves its parts empty.
struct MeasureVolume {
    let name: String
    let total: Int
    let available: Int

    static func of(_ path: String) -> MeasureVolume {
        let url = URL(fileURLWithPath: path)
        let keys: Set<URLResourceKey> = [.volumeNameKey, .volumeTotalCapacityKey, .volumeAvailableCapacityKey]
        for candidate in [url, url.deletingLastPathComponent()] {
            guard let values = try? candidate.resourceValues(forKeys: keys), let name = values.volumeName else { continue }
            return MeasureVolume(name: name, total: values.volumeTotalCapacity ?? 0,
                                 available: values.volumeAvailableCapacity ?? 0)
        }
        return MeasureVolume(name: "", total: 0, available: 0)
    }

    var document: ResultDocument {
        ["name": .init(name), "total": .number(Double(total)), "available": .number(Double(available))]
    }
}

/// One folder's walk: its children measured to the bottom with an explicit stack, so no tree is
/// deep enough to exhaust the thread's stack.
final class MeasureWalker {
    private let budget: MeasureBudget
    private let device: UInt64
    private var truncated = false

    init(budget: MeasureBudget, device: UInt64) {
        self.budget = budget
        self.device = device
    }

    /// The folder's document: its totals over every child and the first `limit` of them listed.
    func folder(_ path: String, _ children: [MeasureChild], limit: Int, volume: MeasureVolume) throws -> ResultDocument {
        truncated = children.count > limit
        var entries: [ResultDocument] = []
        var totals = MeasureTally()
        for child in children {
            try budget.gate()
            let (entry, unreadable) = try measure(path, child)
            totals.bytes += entry.bytes
            totals.visible += entry.visible
            totals.items += 1 + entry.items
            totals.unreadable += unreadable
            if entries.count < limit { entries.append(Self.document(entry)) }
        }
        return Self.walked(path, volume: volume, error: "", missing: false, truncated: truncated,
                           totals: totals, entries: entries)
    }

    /// One direct child: its own bytes plus, for a folder on this volume, everything below it.
    private func measure(_ parent: String, _ child: MeasureChild) throws -> (MeasureEntry, Int) {
        let path = MeasureWalker.join(parent, child.name)
        let own = budget.own(child)
        var entry = MeasureEntry(child: child, path: path, bytes: own)
        guard entry.folder, child.on(device) else { return (entry, 0) }
        if budget.expired {
            truncated = true
            return (entry, 0)
        }
        guard let below = try below(path, hidden: child.hidden) else {
            entry.bytes = 0
            entry.visible = 0
            entry.guarded = true
            return (entry, 1)
        }
        entry.bytes = own + below.bytes
        entry.visible = child.hidden ? 0 : own + below.visible
        entry.items = below.items
        return (entry, below.unreadable)
    }

    /// Everything below the folder at `path`, or nil when that folder cannot be read.
    private func below(_ path: String, hidden: Bool) throws -> MeasureTally? {
        guard let first = try? MeasureWalker.children(path) else { return nil }
        var pending: [(path: String, hidden: Bool)] = []
        var tally = spread(path, hidden: hidden, first, into: &pending)
        while let folder = pending.popLast() {
            try budget.gate()
            if budget.expired {
                truncated = true
                return tally
            }
            guard let children = try? MeasureWalker.children(folder.path) else {
                tally.unreadable += 1
                continue
            }
            tally.add(spread(folder.path, hidden: folder.hidden, children, into: &pending))
        }
        return tally
    }

    /// Counts one read folder's children and queues the subfolders the walk may enter.
    private func spread(_ parent: String, hidden: Bool, _ children: [MeasureChild],
                        into pending: inout [(path: String, hidden: Bool)]) -> MeasureTally {
        var tally = MeasureTally()
        for child in children {
            let inSight = !hidden && !child.hidden
            let bytes = budget.own(child)
            tally.bytes += bytes
            tally.visible += inSight ? bytes : 0
            tally.items += 1
            if child.folder, child.on(device) {
                pending.append((MeasureWalker.join(parent, child.name), !inSight))
            }
        }
        return tally
    }

    /// The children of one folder, each as one `lstat` saw it.
    static func children(_ path: String) throws -> [MeasureChild] {
        try FileManager.default.contentsOfDirectory(atPath: path).map { name in
            var status = stat()
            return MeasureChild(name: name, status: lstat(join(path, name), &status) == 0 ? status : nil)
        }
    }

    /// `folder` and `name` as one path.
    static func join(_ folder: String, _ name: String) -> String {
        (folder as NSString).appendingPathComponent(name)
    }

    /// One measured child as the slice lists it, with the keys [UI-EFFECTS-MEASURE] documents.
    static func document(_ entry: MeasureEntry) -> ResultDocument {
        ["name": .init(entry.name), "path": .init(entry.path), "bytes": .number(Double(entry.bytes)),
         "visible": .number(Double(entry.visible)), "items": .number(Double(entry.items)),
         "hidden": .bool(entry.hidden), "folder": .bool(entry.folder), "link": .bool(entry.link),
         "guarded": .bool(entry.guarded)]
    }

    /// One folder as the slice lists it, with the keys [UI-EFFECTS-MEASURE] documents.
    static func walked(_ path: String, volume: MeasureVolume, error: String = "", missing: Bool = false,
                       truncated: Bool = false, totals: MeasureTally = MeasureTally(),
                       entries: [ResultDocument] = []) -> ResultDocument {
        ["path": .init(path), "error": .init(error), "missing": .bool(missing), "truncated": .bool(truncated),
         "unreadable": .number(Double(totals.unreadable)), "bytes": .number(Double(totals.bytes)),
         "visible": .number(Double(totals.visible)), "items": .number(Double(totals.items)),
         "volume": volume.document, "entries": .array(entries)]
    }

    /// A folder the walk could not open: missing when it is not there, its platform error otherwise.
    static func refused(_ path: String, volume: MeasureVolume, problem: Error) -> ResultDocument {
        let failure = problem as NSError
        let missing = failure.domain == NSCocoaErrorDomain && failure.code == NSFileReadNoSuchFileError
        return walked(path, volume: volume, error: missing ? "" : problem.localizedDescription,
                      missing: missing, truncated: false)
    }
}
