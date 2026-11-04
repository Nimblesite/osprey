import Foundation

// A small, total JSON value for mechanical results and request members. The
// backend never interprets these; it carries them between Osprey and the platform.
public indirect enum JSONValue: Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public var string: String? { if case let .string(value) = self { return value } else { return nil } }
    public var bool: Bool? { if case let .bool(value) = self { return value } else { return nil } }
    public var int: Int? { if case let .number(value) = self, value.isFinite, abs(value) < 9e15 { return Int(value) } else { return nil } }
    public var array: [JSONValue]? { if case let .array(value) = self { return value } else { return nil } }
    public var object: [String: JSONValue]? { if case let .object(value) = self { return value } else { return nil } }

    /// Serialized as compact JSON text with sorted keys, so equal values encode equally.
    public var text: String {
        switch self {
        case .null: return "null"
        case let .bool(value): return value ? "true" : "false"
        case let .number(value): return value == value.rounded() && abs(value) < 9e15 ? String(Int(value)) : String(value)
        case let .string(value): return JSONValue.quote(value)
        case let .array(items): return "[" + items.map(\.text).joined(separator: ",") + "]"
        case let .object(members): return "{" + members.keys.sorted().map { JSONValue.quote($0) + ":" + members[$0]!.text }.joined(separator: ",") + "}"
        }
    }

    /// JSON string quoting through Foundation, so escapes are never hand-written.
    public static func quote(_ value: String) -> String {
        let data = try? JSONSerialization.data(withJSONObject: [value], options: [.withoutEscapingSlashes])
        let wrapped = data.flatMap { String(data: $0, encoding: .utf8) } ?? "[\"\"]"
        return String(wrapped.dropFirst().dropLast())
    }

    public static func parse(_ text: String) -> JSONValue? {
        guard let object = try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed]) else { return nil }
        return JSONValue(any: object)
    }

    init?(any: Any) {
        switch any {
        case is NSNull: self = .null
        case let number as NSNumber: self = CFGetTypeID(number) == CFBooleanGetTypeID() ? .bool(number.boolValue) : .number(number.doubleValue)
        case let text as String: self = .string(text)
        case let items as [Any]: self = .array(items.compactMap(JSONValue.init(any:)))
        case let members as [String: Any]: self = .object(members.compactMapValues(JSONValue.init(any:)))
        default: return nil
        }
    }
}

extension JSONValue: Decodable {
    public init(from decoder: Decoder) throws {
        let single = try decoder.singleValueContainer()
        if single.decodeNil() { self = .null }
        else if let value = try? single.decode(Bool.self) { self = .bool(value) }
        else if let value = try? single.decode(Double.self) { self = .number(value) }
        else if let value = try? single.decode(String.self) { self = .string(value) }
        else if let value = try? single.decode([JSONValue].self) { self = .array(value) }
        else { self = .object(try single.decode([String: JSONValue].self)) }
    }
}

extension JSONValue: ExpressibleByStringLiteral, ExpressibleByBooleanLiteral, ExpressibleByIntegerLiteral {
    public init(stringLiteral value: String) { self = .string(value) }
    public init(booleanLiteral value: Bool) { self = .bool(value) }
    public init(integerLiteral value: Int) { self = .number(Double(value)) }
}
