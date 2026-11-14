import Foundation

// Implements [NATIVE-SHM-NODES] on the host: the component tree the application told, read from
// the checked rows of the shared frame. A `field` row is one typed value, a `record` row a
// component or one of its members and a `list` row the items under one name. Nothing here is
// parsed from text: every value is a row the frame check already accepted.

/// One told value: text, a number, a flag, a list of items or a record of named members.
public indirect enum Tree: Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([Tree])
    case object([String: Tree])

    public var string: String? { if case let .string(value) = self { return value } else { return nil } }
    public var bool: Bool? { if case let .bool(value) = self { return value } else { return nil } }
    public var int: Int? { if case let .number(value) = self, value.isFinite, abs(value) < 9e15 { return Int(value) } else { return nil } }
    /// A whole number a JavaScript-safe count can hold, from 0 to 2^53 - 1; anything else is none.
    public var whole: Int? { if case let .number(value) = self, value == value.rounded(), (0...9_007_199_254_740_991).contains(value) { return Int(value) } else { return nil } }
    public var array: [Tree]? { if case let .array(value) = self { return value } else { return nil } }
    public var object: [String: Tree]? { if case let .object(value) = self { return value } else { return nil } }

    /// The words a control event carries for this value: nothing, text, a flag or a whole number.
    public var words: String {
        switch self {
        case let .string(value): return value
        case let .bool(value): return value ? "true" : "false"
        case let .number(value): return value == value.rounded() && abs(value) < 9e15 ? String(Int(value)) : String(value)
        default: return ""
        }
    }
}

extension Tree: ExpressibleByStringLiteral, ExpressibleByBooleanLiteral, ExpressibleByIntegerLiteral {
    public init(stringLiteral value: String) { self = .string(value) }
    public init(booleanLiteral value: Bool) { self = .bool(value) }
    public init(integerLiteral value: Int) { self = .number(Double(value)) }
}

/// The frame a host draws: the theme and every region's component tree, by key.
public struct ToldFrame: Equatable, Sendable {
    public let theme: Theme?
    public let regions: [(key: String, node: Tree)]

    public static func == (first: ToldFrame, second: ToldFrame) -> Bool {
        first.theme == second.theme && first.regions.map(\.key) == second.regions.map(\.key) && first.regions.map(\.node) == second.regions.map(\.node)
    }
}

/// One colour role, `#RRGGBB` in each appearance ([UI-THEME]).
public struct ThemeToken: Sendable, Equatable {
    public let name: String
    public let light: String
    public let dark: String
}

/// The application's theme: colour tokens, a spacing scale and motion durations ([UI-THEME]).
public struct Theme: Sendable, Equatable {
    public let tokens: [ThemeToken]
    public let spacing: [Int]
    public let feedback: Int
    public let settle: Int

    /// The theme a told record names; nil when it is no record.
    public init?(_ told: Tree?) {
        guard let told, told.object != nil else { return nil }
        tokens = told.list("tokens").map { ThemeToken(name: $0.text("name"), light: $0.text("light"), dark: $0.text("dark")) }
        spacing = told.list("spacing").compactMap(\.int)
        feedback = told.count("feedback")
        settle = told.count("settle")
    }
}

/// Reads the told rows of a checked frame into trees.
public enum Told {
    /// The value of one row and the rows inside it. A record that a scene names carries its row
    /// as `sceneRow`, so the canvas finds the scene the frame gave it.
    static func tree(_ shown: Shown, _ index: Int) -> Tree {
        let row = shown.controls[index]
        switch row.kind {
        case .field:
            switch row.look.weight {
            case 1: return .number(Double(row.value))
            case 2: return .number(row.look.size)
            case 3: return .bool(row.value != 0)
            case 4: return .number(-row.look.size)
            default: return .string(row.words)
            }
        case .list:
            return .array(row.children.map { tree(shown, $0) })
        default:
            var members: [String: Tree] = [:]
            for child in row.children where !shown.controls[child].hint.isEmpty {
                members[shown.controls[child].hint] = tree(shown, child)
            }
            if shown.scenes[index] != nil { members["sceneRow"] = .number(Double(index)) }
            return .object(members)
        }
    }

    /// The frame's theme and regions, or nil when the frame tells no components: a frame of
    /// plain view rows, which the row view draws.
    public static func frame(_ shown: Shown) -> ToldFrame? {
        let told = shown.roots.filter { shown.controls[$0].kind == .record }
        guard !told.isEmpty else { return nil }
        var theme: Theme?
        var regions: [(key: String, node: Tree)] = []
        for root in told {
            let value = tree(shown, root)
            switch shown.controls[root].hint {
            case "theme": theme = Theme(value)
            case "region": regions.append((key: value.text("key"), node: value["node"] ?? .null))
            default: break
            }
        }
        return ToldFrame(theme: theme, regions: regions)
    }
}
