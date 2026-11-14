import CryptoKit
import Foundation

// Implements [UI-EFFECTS] file facts: immediate children with flags, bounded property-list and
// text reads written to a slice, and SHA-256 of one regular file. No traversal, no
// interpretation; every read is bounded by the limit Osprey named, and an inaccessible parent is
// reported, never mistaken for absence.

public struct ListDirectoriesAdapter: CapabilityAdapter {
    public let kinds = ["list-directories"]
    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let limit = try required(ask, whole: 2, "entry limit")
        guard limit >= 0 else { throw CapabilityError.invalidArgument("Invalid directory entry limit") }
        let paths = try required(ask, texts: 1, "paths")
        let listings: [ResultDocument] = await withTaskGroup(of: (Int, ResultDocument).self) { group in
            for (at, path) in paths.enumerated() {
                group.addTask { (at, Self.listing(path, limit: limit)) }
            }
            var held: [Int: ResultDocument] = [:]
            for await (at, entry) in group { held[at] = entry }
            return held.sorted { $0.key < $1.key }.map(\.value)
        }
        let document: ResultDocument = ["paths": .array(listings)]
        slices.write(try required(ask, text: 3, "slice"), document.text)
        return .document([:])
    }

    /// One folder's bounded listing; [UI-EFFECTS-MEASURE] an inaccessible parent is not absence.
    private static func listing(_ path: String, limit: Int) -> ResultDocument {
        let url = URL(fileURLWithPath: path)
        do {
            let (children, truncated) = try boundedChildren(url, limit: limit)
            return ["path": .init(path), "error": "", "missing": false, "truncated": .bool(truncated), "entries": .array(children.map(entry))]
        } catch {
            let failure = error as NSError
            let missing = failure.domain == NSCocoaErrorDomain && failure.code == NSFileReadNoSuchFileError
            return ["path": .init(path), "error": .init(missing ? "" : error.localizedDescription), "missing": .bool(missing), "truncated": false, "entries": []]
        }
    }

    /// Reads only the bounded prefix and one lookahead, so a huge folder cannot exhaust memory.
    private static func boundedChildren(_ url: URL, limit: Int) throws -> ([URL], Bool) {
        var failure: Error?
        let keys: [URLResourceKey] = [.isDirectoryKey, .isSymbolicLinkKey, .isHiddenKey]
        guard let iterator = FileManager.default.enumerator(at: url, includingPropertiesForKeys: keys, options: [.skipsSubdirectoryDescendants], errorHandler: { _, problem in failure = problem; return false }) else {
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

    private static func entry(_ url: URL) -> ResultDocument {
        let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey, .isHiddenKey])
        return ["name": .init(url.lastPathComponent), "path": .init(url.path), "directory": .bool(values?.isDirectory ?? false), "link": .bool(values?.isSymbolicLink ?? false), "hidden": .bool(values?.isHidden ?? url.lastPathComponent.hasPrefix("."))]
    }
}

public struct ReadPropertyListsAdapter: CapabilityAdapter {
    public let kinds = ["read-plists"]
    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let limit = try required(ask, whole: 4, "byte limit")
        let paths = try required(ask, texts: 1, "paths")
        let files = paths.map { file($0, keys: ask.texts(2), suffixes: ask.texts(3), limit: limit) }
        let document: ResultDocument = ["files": .array(files)]
        slices.write(try required(ask, text: 5, "slice"), document.text)
        return .document([:])
    }

    private func file(_ path: String, keys: [String], suffixes: [String], limit: Int) -> ResultDocument {
        do {
            let data = try Bounded.read(path, limit: limit)
            let raw = try PropertyListSerialization.propertyList(from: data, format: nil)
            guard let members = raw as? [String: Any] else { throw CapabilityError.invalidArgument("Not a dictionary property list") }
            let selected = members.filter { keys.contains($0.key) }.compactMapValues { PropertyLists.document($0) }
            let suffixed = members.keys.filter { key in suffixes.contains { key.hasSuffix($0) } }.sorted()
            return ["path": .init(path), "error": "", "values": .object(selected), "suffixed": .array(suffixed.map { .string($0) })]
        } catch {
            return ["path": .init(path), "error": .init(describe(error)), "values": [:], "suffixed": []]
        }
    }
}

public struct ReadTextsAdapter: CapabilityAdapter {
    public let kinds = ["read-texts"]
    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let limit = try required(ask, whole: 2, "byte limit")
        let paths = try required(ask, texts: 1, "paths")
        let files = paths.map { path -> ResultDocument in
            do { return ["path": .init(path), "error": "", "text": .string(try Bounded.text(try Bounded.read(Self.located(path), limit: limit)))] }
            catch { return ["path": .init(path), "error": .init(describe(error)), "text": ""] }
        }
        let document: ResultDocument = ["files": .array(files)]
        slices.write(try required(ask, text: 3, "slice"), document.text)
        return .document([:])
    }

    /// An absolute path as given; a relative one inside the application's own resources, as its
    /// lessons and catalogues travel with it, and never outside them ([UI-COMPONENTS-MEDIA]).
    static func located(_ path: String) throws -> String {
        if path.hasPrefix("/") { return path }
        guard let found = Bundled.url(path) else { throw CapabilityError.notAllowed("\(path) is not inside the application's resources") }
        return found.path
    }
}

public struct FingerprintAdapter: CapabilityAdapter {
    public let kinds = ["fingerprint"]
    public init() {}

    /// SHA-256 of one regular file, with its size and identity so a changed file during the read is visible.
    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let path = try required(ask, text: 1, "path")
        let snapshot = try Bounded.snapshot(path, limit: try required(ask, whole: 2, "byte limit"))
        let digest = SHA256.hash(data: snapshot.data).map { String(format: "%02x", $0) }.joined()
        return .document(["path": .init(path), "sha256": .init(digest), "bytes": .number(Double(snapshot.data.count)),
                          "device": .number(Double(snapshot.identity.st_dev)), "inode": .number(Double(snapshot.identity.st_ino))])
    }
}

enum PropertyLists {
    /// A property-list value as a document: dates as ISO 8601 text and data as base64, so nothing is lost silently.
    static func document(_ value: Any) -> ResultDocument? {
        switch value {
        case let text as String: return .string(text)
        case let number as NSNumber: return CFGetTypeID(number) == CFBooleanGetTypeID() ? .bool(number.boolValue) : .number(number.doubleValue)
        case let date as Date: return .string(ISO8601DateFormatter().string(from: date))
        case let data as Data: return .string(data.base64EncodedString())
        case let items as [Any]: return .array(items.compactMap(document))
        case let members as [String: Any]: return .object(members.compactMapValues(document))
        default: return nil
        }
    }
}

/// An adapter's error as the text its result document carries.
func describe(_ error: Error) -> String {
    if let problem = error as? CapabilityError { return problem.description }
    return error.localizedDescription
}
