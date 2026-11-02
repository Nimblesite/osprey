import Foundation

// Implements [UI-PROTOCOL]: the envelope Osprey returns. A node stays the JSON
// the framework encoded, so the renderer reads exactly what crossed the
// boundary; it never interprets what a node means ([NATIVE-HOSTS-CONTRACT]).

public enum ProtocolError: Error, Equatable {
    case unknownKind(String)
    case unsupportedVersion(Int)
    case missingMember(String, String)
    case malformed(String)
}

public struct ThemeToken: Decodable, Sendable, Equatable {
    public let name: String
    public let light: String
    public let dark: String
}

public struct Theme: Decodable, Sendable, Equatable {
    public let tokens: [ThemeToken]
    public let spacing: [Int]
    public let feedback: Int
    public let settle: Int
}

public struct Patch: Decodable, Sendable, Equatable {
    public let key: String
    public let revision: Int
    public let node: JSONValue
}

public struct SliceText: Decodable, Sendable, Equatable {
    public let key: String
    public let text: String
}

/// A capability request as Osprey encoded it; adapters read the members they need.
public struct Request: Sendable, Equatable {
    public let ticket: String
    public let kind: String
    public let members: [String: JSONValue]

    public func text(_ key: String) throws -> String {
        guard case let .string(value)? = members[key] else { throw ProtocolError.missingMember("string", key) }
        return value
    }

    public func number(_ key: String) throws -> Int {
        guard case let .number(value)? = members[key], value.isFinite, abs(value) < 9e15 else { throw ProtocolError.missingMember("number", key) }
        return Int(value)
    }

    public func flag(_ key: String) throws -> Bool {
        guard case let .bool(value)? = members[key] else { throw ProtocolError.missingMember("bool", key) }
        return value
    }

    public func texts(_ key: String) throws -> [String] {
        guard case let .array(items)? = members[key] else { throw ProtocolError.missingMember("array", key) }
        return try items.map { item in
            guard case let .string(value) = item else { throw ProtocolError.missingMember("string", key) }
            return value
        }
    }
}

extension Request: Decodable {
    public init(from decoder: Decoder) throws {
        let raw = try JSONValue(from: decoder)
        guard case let .object(members) = raw, let ticket = members["ticket"]?.string, let kind = members["kind"]?.string else {
            throw ProtocolError.unknownKind("request")
        }
        self.init(ticket: ticket, kind: kind, members: members)
    }
}

public struct Envelope: Decodable, Sendable {
    public static let version = 1
    public let version: Int
    public let state: String
    public let slices: [SliceText]
    public let patches: [Patch]
    public let removed: [String]
    public let requests: [Request]
    public let diagnostics: [String]
    public let theme: Theme?

    /// Decodes one library result; a protocol version this host does not speak is refused.
    public static func decode(_ text: String) throws -> Envelope {
        let envelope = try JSONDecoder().decode(Envelope.self, from: Data(text.utf8))
        guard envelope.version == Envelope.version else { throw ProtocolError.unsupportedVersion(envelope.version) }
        return envelope
    }
}
