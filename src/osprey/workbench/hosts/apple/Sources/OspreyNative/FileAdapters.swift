import CryptoKit
import Foundation

// Implements [UI-EFFECTS]: file facts. Immediate children with flags, bounded
// property-list and text reads converted mechanically to JSON, and SHA-256 of
// one regular file. No traversal, no interpretation.

public struct ListDirectoriesAdapter: CapabilityAdapter {
    public let kinds = ["list-directories"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let limit = try request.number("limit")
        guard limit >= 0 else { throw CapabilityError.invalidArgument("Invalid directory entry limit") }
        let paths = try request.texts("paths").map { listing($0, limit: limit) }
        slices.write(try request.text("slice"), JSONValue.object(["paths": .array(paths)]).text)
        return .object([:])
    }

    private func listing(_ path: String, limit: Int) -> JSONValue {
        let url = URL(fileURLWithPath: path)
        do {
            let (children, truncated) = try boundedChildren(url, limit: limit)
            return .object(["path": .string(path), "error": "", "missing": false, "truncated": .bool(truncated), "entries": .array(children.map(entry))])
        } catch {
            // [SECURITY-COLLECTORS-RECEIPTS]: an inaccessible parent is not absence.
            let failure = error as NSError
            let missing = failure.domain == NSCocoaErrorDomain && failure.code == NSFileReadNoSuchFileError
            return .object(["path": .string(path), "error": .string(missing ? "" : error.localizedDescription), "missing": .bool(missing), "truncated": false, "entries": .array([])])
        }
    }

    // [SECURITY-COLLECTORS-RECEIPTS]: read only the bounded prefix and one lookahead.
    private func boundedChildren(_ url: URL, limit: Int) throws -> ([URL], Bool) {
        var failure: Error?
        let keys: [URLResourceKey] = [.isDirectoryKey, .isSymbolicLinkKey, .isHiddenKey]
        guard let iterator = FileManager.default.enumerator(at: url, includingPropertiesForKeys: keys, options: [.skipsSubdirectoryDescendants], errorHandler: { _, error in failure = error; return false }) else {
            throw CocoaError(.fileReadUnknown)
        }
        var children: [URL] = []
        while let child = iterator.nextObject() as? URL {
            try Task.checkCancellation()
            if children.count == limit { return (children, true) }
            children.append(child)
        }
        if let failure { throw failure }
        return (children, false)
    }

    private func entry(_ url: URL) -> JSONValue {
        let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey, .isHiddenKey])
        return .object(["name": .string(url.lastPathComponent), "path": .string(url.path), "directory": .bool(values?.isDirectory ?? false), "link": .bool(values?.isSymbolicLink ?? false), "hidden": .bool(values?.isHidden ?? url.lastPathComponent.hasPrefix("."))])
    }
}

public struct ReadPropertyListsAdapter: CapabilityAdapter {
    public let kinds = ["read-plists"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let keys = try request.texts("keys"), suffixes = try request.texts("suffixes"), limit = try request.number("limit")
        let files = try request.texts("paths").map { file($0, keys: keys, suffixes: suffixes, limit: limit) }
        slices.write(try request.text("slice"), JSONValue.object(["files": .array(files)]).text)
        return .object([:])
    }

    private func file(_ path: String, keys: [String], suffixes: [String], limit: Int) -> JSONValue {
        do {
            let data = try Bounded.read(path, limit: limit)
            let raw = try PropertyListSerialization.propertyList(from: data, format: nil)
            guard let members = raw as? [String: Any] else { throw CapabilityError.invalidArgument("Not a dictionary property list") }
            let selected = members.filter { keys.contains($0.key) }.compactMapValues { PropertyLists.json($0) }
            let suffixed = members.keys.filter { key in suffixes.contains { key.hasSuffix($0) } }.sorted()
            return .object(["path": .string(path), "error": "", "values": .object(selected), "suffixed": .array(suffixed.map(JSONValue.string))])
        } catch {
            return .object(["path": .string(path), "error": .string(describe(error)), "values": .object([:]), "suffixed": .array([])])
        }
    }
}

public struct ReadTextsAdapter: CapabilityAdapter {
    public let kinds = ["read-texts"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let limit = try request.number("limit")
        let files = try request.texts("paths").map { path -> JSONValue in
            do { return .object(["path": .string(path), "error": "", "text": .string(try Bounded.text(try Bounded.read(path, limit: limit)))]) } catch { return .object(["path": .string(path), "error": .string(describe(error)), "text": ""]) }
        }
        slices.write(try request.text("slice"), JSONValue.object(["files": .array(files)]).text)
        return .object([:])
    }
}

public struct FingerprintAdapter: CapabilityAdapter {
    public let kinds = ["fingerprint"]
    public init() {}

    /// SHA-256 of one regular file, with its size and identity so a changed file during the read is visible.
    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let path = try request.text("path")
        let snapshot = try Bounded.snapshot(path, limit: try request.number("limit"))
        let data = snapshot.data
        let status = snapshot.identity
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return .object(["path": .string(path), "sha256": .string(digest), "bytes": .number(Double(data.count)), "device": .number(Double(status.st_dev)), "inode": .number(Double(status.st_ino))])
    }
}

enum PropertyLists {
    /// A property-list value as JSON: dates as ISO 8601 text and data as base64, so nothing is lost silently.
    static func json(_ value: Any) -> JSONValue? {
        switch value {
        case let text as String: return .string(text)
        case let number as NSNumber: return CFGetTypeID(number) == CFBooleanGetTypeID() ? .bool(number.boolValue) : .number(number.doubleValue)
        case let date as Date: return .string(ISO8601DateFormatter().string(from: date))
        case let data as Data: return .string(data.base64EncodedString())
        case let items as [Any]: return .array(items.compactMap(json))
        case let members as [String: Any]: return .object(members.compactMapValues(json))
        default: return nil
        }
    }
}

func describe(_ error: Error) -> String {
    if let problem = error as? CapabilityError { return problem.description }
    return error.localizedDescription
}
