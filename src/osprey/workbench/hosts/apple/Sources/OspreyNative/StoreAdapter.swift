import Foundation

// Implements [UI-EFFECTS]: app-private files. The
// directory is 0700 and files 0600; writes are atomic; names are relative
// paths inside the store and never escape it. Every request resolves its
// name one component at a time from an open descriptor with O_NOFOLLOW, so a
// link planted inside the store never redirects a read, a listing, a write
// or a deletion outside it.

public struct StoreAdapter: CapabilityAdapter {
    public let kinds = ["store-read", "store-write", "store-list", "store-delete"]
    public let root: URL

    public init(root: URL) {
        self.root = root
    }

    /// The default store: Application Support/<bundle identifier>, private to the user.
    public static func standard(bundle: String) -> StoreAdapter {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first ?? FileManager.default.temporaryDirectory
        return StoreAdapter(root: support.appendingPathComponent(bundle, isDirectory: true))
    }

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        try prepare()
        switch request.kind {
        case "store-read": return try read(request, slices: slices)
        case "store-write": return try write(request, slices: slices)
        case "store-list": return try list(request)
        default: return try delete(request)
        }
    }

    private func prepare() throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    }

    /// A name stays inside the store: no absolute paths, no parent references.
    func location(_ name: String) throws -> URL {
        try components(name).reduce(root) { $0.appendingPathComponent($1) }
    }

    private func components(_ name: String) throws -> [String] {
        let parts = name.split(separator: "/").map(String.init)
        guard !name.isEmpty, !name.hasPrefix("/"), !name.utf8.contains(0), !parts.contains("..") else {
            throw CapabilityError.notAllowed("Invalid store name \(name)")
        }
        return parts
    }

    private static let directoryFlags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC

    private func read(_ request: Request, slices: SliceStore) throws -> JSONValue {
        let parts = try components(try request.text("name"))
        guard let leaf = parts.last, let parent = try pinnedDirectory(parts.dropLast(), create: false) else {
            return .object(["found": false])
        }
        defer { close(parent) }
        guard let data = try Bounded.read(parent: parent, leaf: leaf, limit: try request.number("limit")) else {
            return .object(["found": false])
        }
        slices.write(try request.text("slice"), try Bounded.text(data))
        return .object(["found": true])
    }

    private func write(_ request: Request, slices: SliceStore) throws -> JSONValue {
        let parts = try components(try request.text("name"))
        guard let leaf = parts.last, let parent = try pinnedDirectory(parts.dropLast(), create: true) else {
            throw CapabilityError.notAllowed("Missing store file name")
        }
        defer { close(parent) }
        try replace(leaf, in: parent, with: Data(slices.text(try request.text("slice")).utf8))
        let url = parts.reduce(root) { $0.appendingPathComponent($1) }
        return .object(["path": .string(url.path)])
    }

    private func list(_ request: Request) throws -> JSONValue {
        let parts = try components(try request.text("name"))
        guard let directory = try pinnedDirectory(parts[...], create: false) else { return .object(["names": .array([])]) }
        let names = try entries(of: directory).sorted()
        return .object(["names": .array(names.map(JSONValue.string))])
    }

    private func delete(_ request: Request) throws -> JSONValue {
        let parts = try components(try request.text("name"))
        guard let leaf = parts.last, let parent = try pinnedDirectory(parts.dropLast(), create: false) else { return .object([:]) }
        defer { close(parent) }
        try remove(leaf, in: parent)
        return .object([:])
    }

    // [UI-EFFECTS] Resolve each component relative to an open directory.
    // A link substituted before its open is refused, and a later substitution
    // cannot redirect the descriptor-relative operation. Without `create`, a
    // missing component means the name is absent, which is `nil`.
    private func pinnedDirectory(_ parts: ArraySlice<String>, create: Bool) throws -> Int32? {
        var directory = open(root.path, Self.directoryFlags)
        guard directory >= 0 else { throw directoryFailure(errno) }
        do {
            for part in parts {
                if create { guard mkdirat(directory, part, 0o700) == 0 || errno == EEXIST else { throw directoryFailure(errno) } }
                let next = openat(directory, part, Self.directoryFlags)
                guard next >= 0 else {
                    if !create && errno == ENOENT { close(directory); return nil }
                    throw directoryFailure(errno)
                }
                close(directory)
                directory = next
            }
            return directory
        } catch {
            close(directory)
            throw error
        }
    }

    private func replace(_ leaf: String, in directory: Int32, with data: Data) throws {
        let temporary = ".osprey-writing-\(UUID())"
        let file = openat(directory, temporary, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
        guard file >= 0 else { throw storeFailure(errno) }
        defer { close(file) }
        var renamed = false
        defer { if !renamed { unlinkat(directory, temporary, 0) } }
        #if os(iOS) && !targetEnvironment(simulator)
        // XNU protection class C is complete until first user authentication.
        let protectedUntilFirstAuthentication: Int32 = 3
        guard fcntl(file, F_SETPROTECTIONCLASS, protectedUntilFirstAuthentication) == 0 else { throw storeFailure(errno) }
        #endif
        try FileHandle(fileDescriptor: file, closeOnDealloc: false).write(contentsOf: data)
        guard renameat(directory, temporary, directory, leaf) == 0 else { throw storeFailure(errno) }
        renamed = true
    }

    /// The entry names of an open directory, consuming the descriptor.
    private func entries(of directory: Int32) throws -> [String] {
        guard let stream = fdopendir(directory) else {
            let failure = errno
            close(directory)
            throw storeFailure(failure)
        }
        defer { closedir(stream) }
        var names: [String] = []
        while let entry = readdir(stream) {
            let name = withUnsafePointer(to: entry.pointee.d_name) { String(cString: UnsafeRawPointer($0).assumingMemoryBound(to: CChar.self)) }
            if name != "." && name != ".." { names.append(name) }
        }
        return names
    }

    /// Removes one entry of a pinned directory: a file is unlinked, a directory
    /// is emptied through its own descriptor first, a link is removed as a link.
    private func remove(_ leaf: String, in directory: Int32) throws {
        var metadata = stat()
        guard fstatat(directory, leaf, &metadata, AT_SYMLINK_NOFOLLOW) == 0 else {
            if errno == ENOENT { return }
            throw storeFailure(errno)
        }
        if (metadata.st_mode & S_IFMT) == S_IFDIR {
            let child = openat(directory, leaf, Self.directoryFlags)
            guard child >= 0 else { throw directoryFailure(errno) }
            for name in try entries(of: dup(child)) { try remove(name, in: child) }
            close(child)
            guard unlinkat(directory, leaf, AT_REMOVEDIR) == 0 else { throw storeFailure(errno) }
            return
        }
        guard unlinkat(directory, leaf, 0) == 0 || errno == ENOENT else { throw storeFailure(errno) }
    }

    private func directoryFailure(_ failure: Int32) -> CapabilityError {
        if failure == ELOOP || failure == ENOTDIR { return .notAllowed("Invalid store directory") }
        return storeFailure(failure)
    }

    private func storeFailure(_ failure: Int32) -> CapabilityError {
        .platform("Store operation failed: \(String(cString: strerror(failure)))")
    }
}
