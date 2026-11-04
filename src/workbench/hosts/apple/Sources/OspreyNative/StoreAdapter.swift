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
        let parts = name.split(separator: "/").map(String.init)
        guard !name.isEmpty, !name.hasPrefix("/"), !parts.contains(".."), !parts.contains("") else { throw CapabilityError.notAllowed("Invalid store name \(name)") }
        return parts.reduce(root) { $0.appendingPathComponent($1) }
    }

    private func read(_ request: Request, slices: SliceStore) throws -> JSONValue {
        let url = try location(try request.text("name"))
        guard FileManager.default.fileExists(atPath: url.path) else { return .object(["found": false]) }
        slices.write(try request.text("slice"), try Bounded.text(try Bounded.read(url.path, limit: try request.number("limit"))))
        return .object(["found": true])
    }

    private func write(_ request: Request, slices: SliceStore) throws -> JSONValue {
        let url = try location(try request.text("name"))
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try Data(slices.text(try request.text("slice")).utf8).write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        return .object(["path": .string(url.path)])
    }

    private func list(_ request: Request) throws -> JSONValue {
        let url = try location(try request.text("name"))
        guard FileManager.default.fileExists(atPath: url.path) else { return .object(["names": .array([])]) }
        let names = try FileManager.default.contentsOfDirectory(atPath: url.path).sorted()
        return .object(["names": .array(names.map(JSONValue.string))])
    }

    private func delete(_ request: Request) throws -> JSONValue {
        let url = try location(try request.text("name"))
        if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) }
        return .object([:])
    }
}
