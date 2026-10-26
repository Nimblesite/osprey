import Foundation

// Implements [IOS-HOST-IMPORTS] and [IOS-TARGET-MACOS]. Osprey calls these C
// functions synchronously on the main thread to read the volume and the last
// measured walk; it decides what any of it means.

/// Volume capacity in bytes, or -1. `important` adds the space the system can
/// purge on demand (caches, local snapshots), which is what Settings shows as
/// available; `available` excludes it.
@_cdecl("storage_volume")
func storageVolume(_ path: UnsafePointer<CChar>?, _ measure: UnsafePointer<CChar>?) -> Int64 {
    guard let path, let measure else { return -1 }
    let keys: Set<URLResourceKey> = [
        .volumeTotalCapacityKey, .volumeAvailableCapacityKey, .volumeAvailableCapacityForImportantUsageKey,
    ]
    guard let values = try? URL(fileURLWithPath: String(cString: path)).resourceValues(forKeys: keys) else {
        return -1
    }
    switch String(cString: measure) {
    case "total": return values.volumeTotalCapacity.map(Int64.init) ?? -1
    case "available": return values.volumeAvailableCapacity.map(Int64.init) ?? -1
    case "important": return values.volumeAvailableCapacityForImportantUsage ?? -1
    default: return -1
    }
}

/// The volume's name as Finder shows it. Osprey may keep a returned string
/// for the rest of the process, so each one is a heap copy Swift never frees.
@_cdecl("storage_volume_name")
func storageVolumeName(_ path: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    let url = path.map { URL(fileURLWithPath: String(cString: $0)) }
    let name = (try? url?.resourceValues(forKeys: [.volumeLocalizedNameKey]))?.volumeLocalizedName
    return UnsafePointer(strdup(deviceModel ?? name ?? "Disk"))
}

/// What an iPhone calls itself, set at launch: its data volume is named "User".
nonisolated(unsafe) var deviceModel: String?

/// Whether macOS granted this app Full Disk Access: only then can it open the
/// privacy database, and trying never shows a prompt.
func fullDiskAccess() -> Bool {
    #if os(macOS)
    let database = open(NSHomeDirectory() + "/Library/Application Support/com.apple.TCC/TCC.db", O_RDONLY)
    guard database >= 0 else { return false }
    close(database)
    return true
    #else
    return false
    #endif
}

/// The entries of the last listed folder.
nonisolated(unsafe) private var listed = DiskTree.Listing.none

/// Lists a measured folder's entries. Returns their count, or -1 when the
/// folder is not part of the last walk.
@_cdecl("storage_scan")
func storageScan(_ folder: UnsafePointer<CChar>?) -> Int64 {
    listed = folder.flatMap { DiskTree.current?.listing(of: String(cString: $0)) } ?? .none
    return listed.measured ? Int64(listed.entries.count) : -1
}

private func listedEntry(_ index: Int64) -> DiskTree.Entry? {
    listed.entries.indices.contains(Int(index)) ? listed.entries[Int(index)] : nil
}

@_cdecl("storage_entry_path")
func storageEntryPath(_ index: Int64) -> UnsafePointer<CChar>? { listedEntry(index)?.path ?? UnsafePointer(strdup("")) }

@_cdecl("storage_entry_bytes")
func storageEntryBytes(_ index: Int64) -> Int64 { listedEntry(index)?.bytes ?? -1 }

@_cdecl("storage_entry_visible")
func storageEntryVisible(_ index: Int64) -> Int64 { listedEntry(index)?.visible ?? -1 }

@_cdecl("storage_entry_hidden")
func storageEntryHidden(_ index: Int64) -> Bool { listedEntry(index)?.hidden ?? false }

@_cdecl("storage_entry_folder")
func storageEntryFolder(_ index: Int64) -> Bool { listedEntry(index)?.folder ?? false }

@_cdecl("storage_entry_link")
func storageEntryLink(_ index: Int64) -> Bool { listedEntry(index)?.link ?? false }

/// A folder the walk left alone because macOS asks before an app reads it.
@_cdecl("storage_entry_guarded")
func storageEntryGuarded(_ index: Int64) -> Bool { listedEntry(index)?.guarded ?? false }

/// Folders below the last listed one that the walk could not read.
@_cdecl("storage_unreadable")
func storageUnreadable() -> Int64 { listed.blocked }

/// An iCloud Drive or File Provider folder whose contents were evicted holds
/// nothing on this disk, and listing it would make the provider fetch it from
/// the network, so the walk does not enter it.
private func dataless(_ path: String) -> Bool {
    var status = stat()
    return lstat(path, &status) == 0 && status.st_flags & UInt32(SF_DATALESS) != 0
}

/// The path with every symlink resolved, as Foundation's enumerator reports
/// paths. `URL.resolvingSymlinksInPath` would strip `/private` back off `/var`.
func canonical(_ path: String) -> String {
    guard let resolved = realpath(path, nil) else { return path }
    defer { free(resolved) }
    return String(cString: resolved)
}

/// One measured walk of a folder. The system keeps no record of folder sizes
/// and Spotlight skips hidden files, so the host measures the tree once: each
/// folder records what lies below it, and what lies below it outside hidden
/// items. Hidden files and package contents count; a hard-linked file counts
/// once; mounted volumes, evicted cloud folders and the folders Osprey guards
/// are not entered. macOS asks once before the walk enters Desktop, Documents,
/// Downloads or iCloud Drive, and the walk waits for the answer.
final class DiskTree: @unchecked Sendable {
    struct Entry {
        let path: UnsafePointer<CChar>?
        let bytes: Int64
        let visible: Int64
        let hidden: Bool
        let folder: Bool
        let guarded: Bool
        var link = false
    }

    /// Folders Osprey asks the walk not to enter: every folder inside `apps`,
    /// each of which holds another app's data.
    struct Guard {
        let apps: Set<String>

        func covers(_ path: String) -> Bool {
            apps.contains((path as NSString).deletingLastPathComponent)
        }
    }

    struct Listing {
        let entries: [Entry]
        let blocked: Int64
        let measured: Bool
        static let none = Listing(entries: [], blocked: 0, measured: false)
    }

    private struct Folder {
        let parent: Int
        let hidden: Bool
        var guarded = false
        var bytes: Int64 = 0
        var visible: Int64 = 0
        var blocked: Int64 = 0
    }

    /// The walk Osprey reads; replaced on the main thread when a new one finishes.
    nonisolated(unsafe) static var current: DiskTree?

    private static let keys: [URLResourceKey] = [
        .isDirectoryKey, .isSymbolicLinkKey, .isVolumeKey, .isHiddenKey, .totalFileAllocatedSizeKey, .linkCountKey,
        .fileResourceIdentifierKey,
    ]

    private var folders: [Folder] = []
    private var index: [String: Int] = [:]
    private var listings: [String: Listing] = [:]
    private var hardLinkSizes: [String: Int64] = [:]

    /// Walks `root`, reporting files and bytes seen so far about four times a
    /// second. Returns nil when the root cannot be read or the task is cancelled.
    static func measure(_ root: String, guarding: Guard, progress: (Int64, Int64) -> Void) -> DiskTree? {
        let tree = DiskTree()
        let url = URL(fileURLWithPath: root, isDirectory: true)
        _ = tree.add(url.path, parent: -1, hidden: false)
        guard (try? url.checkResourceIsReachable()) == true,
            let walker = FileManager.default.enumerator(
                at: url, includingPropertiesForKeys: keys, options: [],
                errorHandler: { failed, _ in tree.block(failed.path) })
        else { return nil }
        var stack = [0], linked = Set<NSObject>(), files: Int64 = 0, bytes: Int64 = 0, reported = Date()
        for case let item as URL in walker {
            guard let values = try? item.resourceValues(forKeys: Set(keys)) else { continue }
            stack.removeLast(max(stack.count - walker.level, 0))
            let parent = stack.last ?? 0
            if values.isDirectory == true {
                let id = tree.add(item.path, parent: parent, hidden: values.isHidden == true)
                tree.folders[id].guarded = guarding.covers(item.path)
                if tree.folders[id].guarded || values.isVolume == true || dataless(item.path) { walker.skipDescendants() }
                stack.append(id)
                continue
            }
            guard let size = tree.fileSize(item, values, &linked) else { continue }
            tree.folders[parent].bytes += size
            if values.isHidden != true { tree.folders[parent].visible += size }
            files += 1
            bytes += size
            guard files % 1024 == 0 else { continue }
            if Task.isCancelled { return nil }
            if reported.timeIntervalSinceNow < -0.25 {
                reported = Date()
                progress(files, bytes)
            }
        }
        tree.total()
        return tree
    }

    /// A hard link keeps the bytes assigned during the walk, including zero
    /// when another path already accounts for that file.
    private func fileSize(_ item: URL, _ values: URLResourceValues, _ linked: inout Set<NSObject>) -> Int64? {
        let size = Int64(values.totalFileAllocatedSize ?? 0)
        guard (values.linkCount ?? 1) > 1, let id = values.fileResourceIdentifier as? NSObject else { return size }
        let counted = linked.insert(id).inserted
        hardLinkSizes[item.path] = counted ? size : 0
        return counted ? size : nil
    }

    private func add(_ path: String, parent: Int, hidden: Bool) -> Int {
        index[path] = folders.count
        folders.append(Folder(parent: parent, hidden: hidden))
        return folders.count - 1
    }

    private func block(_ path: String) -> Bool {
        folders[index[path] ?? 0].blocked += 1
        return true
    }

    /// Folders are recorded before anything inside them, so one pass from the
    /// end adds each folder's totals into its parent after its own are complete.
    private func total() {
        for id in folders.indices.reversed() where folders[id].parent >= 0 {
            let folder = folders[id]
            folders[folder.parent].bytes += folder.bytes
            folders[folder.parent].blocked += folder.blocked
            if !folder.hidden { folders[folder.parent].visible += folder.visible }
        }
    }

    /// Adds a walk of one folder inside this one: the folder takes the walk's
    /// totals, every folder above it moves by the difference, and its folders
    /// join the index.
    func graft(_ walked: DiskTree, at path: String) {
        guard let target = index[path], let measured = walked.folders.first else { return }
        let before = folders[target]
        folders[target] = Folder(parent: before.parent, hidden: before.hidden, bytes: measured.bytes,
                                 visible: measured.visible, blocked: measured.blocked)
        var hiddenBelow = before.hidden, parent = before.parent
        while parent >= 0 {
            folders[parent].bytes += measured.bytes - before.bytes
            folders[parent].blocked += measured.blocked - before.blocked
            if !hiddenBelow { folders[parent].visible += measured.visible - before.visible }
            hiddenBelow = hiddenBelow || folders[parent].hidden
            parent = folders[parent].parent
        }
        let offset = folders.count - 1
        for folder in walked.folders.dropFirst() {
            var moved = Folder(parent: folder.parent == 0 ? target : folder.parent + offset, hidden: folder.hidden,
                               bytes: folder.bytes, visible: folder.visible, blocked: folder.blocked)
            moved.guarded = folder.guarded
            folders.append(moved)
        }
        for (folder, id) in walked.index where id > 0 { index[folder] = id + offset }
        hardLinkSizes = hardLinkSizes.filter { !$0.key.hasPrefix(path + "/") }
            .merging(walked.hardLinkSizes) { _, measured in measured }
        listings = [:]
    }

    /// A measured folder's direct entries, largest first so Osprey's ranking
    /// rarely reorders. Each folder is listed once per walk, and its strings
    /// stay valid for the process as the boundary requires.
    func listing(of requested: String) -> Listing {
        let folder = canonical(requested)
        if let cached = listings[folder] { return cached }
        guard let id = index[folder] else { return .none }
        let items = (try? FileManager.default.contentsOfDirectory(
            at: URL(fileURLWithPath: folder, isDirectory: true), includingPropertiesForKeys: Self.keys)) ?? []
        let entries = items.compactMap(entry).sorted { $0.bytes > $1.bytes }
        let listing = Listing(entries: entries, blocked: folders[id].blocked, measured: true)
        listings[folder] = listing
        return listing
    }

    private func entry(_ item: URL) -> Entry? {
        guard let values = try? item.resourceValues(forKeys: Set(Self.keys)) else { return nil }
        let hidden = values.isHidden == true
        let path = UnsafePointer(strdup(item.path))
        if values.isSymbolicLink == true {
            let target = URL(fileURLWithPath: canonical(item.path))
            let folder = (try? target.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
            return Entry(path: path, bytes: 0, visible: 0, hidden: hidden, folder: folder, guarded: false, link: true)
        }
        if values.isDirectory == true, let child = index[item.path] {
            let folder = folders[child]
            return Entry(path: path, bytes: folder.bytes, visible: folder.visible, hidden: hidden, folder: true, guarded: folder.guarded)
        }
        let size = hardLinkSizes[item.path] ?? Int64(values.totalFileAllocatedSize ?? 0)
        return Entry(path: path, bytes: size, visible: size, hidden: hidden, folder: values.isDirectory == true, guarded: false)
    }
}
