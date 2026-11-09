import Foundation

// Implements [UI-EFFECTS] and [SECURITY-PRIVACY]: app-private files. The
// directory is 0700 and files 0600; writes are atomic; names are relative
// paths inside the store and never escape it.

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

    // [UI-EFFECTS-STORAGE-ACCURACY] An inaccessible path is not proven absent.
    private func exists(_ url: URL) throws -> Bool {
        var metadata = stat()
        if lstat(url.path, &metadata) == 0 { return true }
        let failure = errno
        guard failure == ENOENT else {
            throw CapabilityError.platform("Store metadata unavailable: \(String(cString: strerror(failure)))")
        }
        return false
    }

    private func read(_ request: Request, slices: SliceStore) throws -> JSONValue {
        let url = try location(try request.text("name"))
        guard try exists(url) else { return .object(["found": false]) }
        slices.write(try request.text("slice"), try Bounded.text(try Bounded.read(url.path, limit: try request.number("limit"))))
        return .object(["found": true])
    }

    private func write(_ request: Request, slices: SliceStore) throws -> JSONValue {
        let parts = try components(try request.text("name"))
        guard let leaf = parts.last else { throw CapabilityError.notAllowed("Missing store file name") }
        let parent = try writeDirectory(parts.dropLast())
        defer { close(parent) }
        try replace(leaf, in: parent, with: Data(slices.text(try request.text("slice")).utf8))
        let url = parts.reduce(root) { $0.appendingPathComponent($1) }
        return .object(["path": .string(url.path)])
    }

    // [UI-EFFECTS] Resolve each component relative to an open directory.
    // A link substituted before its open is refused, and a later substitution
    // cannot redirect the descriptor-relative write.
    private func writeDirectory(_ parts: ArraySlice<String>) throws -> Int32 {
        let flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC
        var directory = open(root.path, flags)
        guard directory >= 0 else { throw directoryFailure(errno) }
        do {
            for part in parts {
                guard mkdirat(directory, part, 0o700) == 0 || errno == EEXIST else { throw directoryFailure(errno) }
                let next = openat(directory, part, flags)
                guard next >= 0 else { throw directoryFailure(errno) }
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

    private func directoryFailure(_ failure: Int32) -> CapabilityError {
        if failure == ELOOP || failure == ENOTDIR { return .notAllowed("Invalid store directory") }
        return storeFailure(failure)
    }

    private func storeFailure(_ failure: Int32) -> CapabilityError {
        .platform("Store write failed: \(String(cString: strerror(failure)))")
    }

    private func list(_ request: Request) throws -> JSONValue {
        let url = try location(try request.text("name"))
        guard try exists(url) else { return .object(["names": .array([])]) }
        let names = try FileManager.default.contentsOfDirectory(atPath: url.path).sorted()
        return .object(["names": .array(names.map(JSONValue.string))])
    }

    private func delete(_ request: Request) throws -> JSONValue {
        let url = try location(try request.text("name"))
        if try exists(url) { try FileManager.default.removeItem(at: url) }
        return .object([:])
    }
}
