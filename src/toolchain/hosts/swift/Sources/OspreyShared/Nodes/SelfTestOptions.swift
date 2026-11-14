import Foundation

// Implements [UI-MACHINE-SELF-TEST]'s tooling, platform-neutral on purpose: the script flags a
// launch parses, the steps and regions a script names, the model and folder doubles a run can
// install, and the fixture files a self-test reads and writes as JSON. The macOS window half —
// the pictures a run compares and the run itself — is SelfTest.swift.
/// The host's own flags; whatever else is on the command line belongs to the application.
public struct SelfTestOptions: Equatable {
    public var script = ""
    public var shots = ""
    public var headless = false
    public var goldens: String?
    public var updateGoldens = false
    public var httpDouble: String?
    public var store: String?
    public var folderDouble: String?
    public var patience = 60.0
    public var appArguments: [String] = []

    /// Nil when this launch is not a self-test.
    public static func parse(_ arguments: [String]) throws -> SelfTestOptions? {
        guard arguments.contains("--self-test") else { return nil }
        var options = SelfTestOptions()
        var given = arguments.makeIterator()
        func value(_ flag: String) throws -> String {
            guard let next = given.next() else { throw SelfTestProblem("\(flag) needs a value") }
            return next
        }
        while let argument = given.next() {
            switch argument {
            case "--self-test":
                options.script = try value(argument)
                options.shots = try value(argument)
            case "--headless": options.headless = true
            case "--update-goldens": options.updateGoldens = true
            case "--goldens": options.goldens = try value(argument)
            case "--http-double": options.httpDouble = try value(argument)
            case "--store": options.store = try value(argument)
            case "--folder-double": options.folderDouble = try value(argument)
            case "--patience":
                guard let seconds = Double(try value(argument)), seconds > 0 else { throw SelfTestProblem("--patience needs a number of seconds") }
                options.patience = seconds
            default: options.appArguments.append(argument)
            }
        }
        return options
    }
}

public struct SelfTestProblem: Error, Equatable, CustomStringConvertible {
    public let description: String
    public init(_ description: String) { self.description = description }
}

/// One step of a script: a verb and its words.
public struct SelfTestStep: Equatable {
    public let verb: String
    public let words: [String]

    /// `{"steps": [{"verb": "word"}, {"verb": ["word", "word"]}]}`.
    public static func parse(_ text: String) throws -> [SelfTestStep] {
        guard let script = Tree.fixture(Data(text.utf8)), let steps = script["steps"]?.array else {
            throw SelfTestProblem("A script holds a list of steps")
        }
        return try steps.map { step in
            guard let members = step.object, members.count == 1, let (verb, value) = members.first else { throw SelfTestProblem("A step is one verb: \(step.fixtureText)") }
            return SelfTestStep(verb: verb, words: value.array?.compactMap(\.string) ?? value.string.map { [$0] } ?? [])
        }
    }
}

/// One event a step sends, bound to its region's current revision when it is sent.
public struct SelfTestSend: Equatable {
    public let region: String
    public let ordinal: Int
    public let value: Tree
}

/// What a step needs of the regions as they stand.
public enum SelfTestAttempt: Equatable {
    case waiting(String)
    case sending(SelfTestSend?)
    case recording(String)
    case sleeping(Double)
    case panel
    /// A drag between two words, or `click` on one, through the window's real events; a frame's
    /// selection group ([UI-COMPONENTS-FRAMED]) answers.
    case selecting([String])
    case checkingSelected(String, contains: Bool)
    case copying(String)
    case clicking(String)
    case pressing(String)
    case pastingImage
    case keyPress(String)
}

/// The regions as words and controls, with no host or window behind them.
public struct SelfTestRegions {
    public let nodes: [(key: String, node: Tree)]

    /// A region no slot draws is not on show; without a `root`, every region is.
    public init(nodes: [(key: String, node: Tree)]) {
        let all = Dictionary(nodes.map { ($0.key, $0.node) }, uniquingKeysWith: { first, _ in first })
        var reached: [String] = []
        func follow(_ node: Tree) {
            if node.text("kind") == "slot", all[node.text("key")] != nil, !reached.contains(node.text("key")) {
                reached.append(node.text("key"))
                follow(all[node.text("key")] ?? .null)
            }
            Self.inside(node).forEach(follow)
        }
        if let root = all["root"] {
            reached.append("root")
            follow(root)
        } else {
            reached = Array(all.keys)
        }
        self.nodes = reached.sorted().compactMap { key in all[key].map { (key: key, node: $0) } }
    }

    // What a closed disclosure holds is not on show.
    private static func inside(_ node: Tree) -> [Tree] {
        if node.text("kind") == "disclosure", !node.flag("expanded") { return [] }
        return node.object?.values.flatMap { $0.array ?? [$0] }.filter { $0.object != nil } ?? []
    }

    private static func holds(_ node: Tree, _ words: String) -> Bool {
        let own = node.object?.values.contains { $0.string?.contains(words) == true || $0.array?.contains { $0.string?.contains(words) == true } == true } ?? false
        return own || inside(node).contains { holds($0, words) }
    }

    private static func find(_ node: Tree, _ wanted: (Tree) -> Bool) -> Tree? {
        if wanted(node) { return node }
        for child in inside(node) { if let found = find(child, wanted) { return found } }
        return nil
    }

    public func shows(_ words: String) -> Bool { nodes.contains { Self.holds($0.node, words) } }

    private func control(_ wanted: (Tree) -> Bool) -> (region: String, node: Tree)? {
        for (key, root) in nodes {
            if let node = Self.find(root, { $0["event"]?.int != nil && wanted($0) }) { return (key, node) }
        }
        return nil
    }

    private func named(_ label: String) -> (region: String, node: Tree)? {
        control { $0.text("label") == label || $0.text("title") == label }
    }

    /// A label read in full, or else the first control whose label holds the words: a disclosure's title counts what it did.
    private func invocable(_ words: String) -> (region: String, node: Tree)? {
        named(words) ?? control { $0.text("label").contains(words) || $0.text("title").contains(words) }
    }

    private func enabled(_ node: Tree) -> Bool { node["enabled"]?.bool ?? true }

    private func chooser(_ words: String) -> SelfTestSend? {
        // An exact choice wins over a substring, so `High` picks an effort picker's `High` before a model named `Highspeed`, and `GLM-5.3` picks `GLM · GLM-5.3` before `GLM · GLM-5.3 Highspeed`.
        for exact in [true, false] {
            for field in ["rows", "choices"] {
                for (key, root) in nodes {
                    let wanted = { (row: Tree) in
                        Self.holds(row, words) && (!exact || row.text("label") == words || row.text("label").hasSuffix(" · " + words) || row.text("key") == words)
                    }
                    guard let node = Self.find(root, { $0["event"]?.int != nil && $0.list(field).contains(where: wanted) }) else { continue }
                    if let row = node.list(field).first(where: wanted) {
                        return SelfTestSend(region: key, ordinal: node.count("event"), value: .string(row.text("key")))
                    }
                }
            }
        }
        return nil
    }

    public func attempt(_ step: SelfTestStep) throws -> SelfTestAttempt {
        func word(_ index: Int) throws -> String {
            guard index < step.words.count else { throw SelfTestProblem("\(step.verb) needs \(index + 1) words") }
            return step.words[index]
        }
        switch step.verb {
        case "wait": return shows(try word(0)) ? .sending(nil) : .waiting("nothing shows \"\(try word(0))\"")
        case "gone": return shows(try word(0)) ? .waiting("\"\(try word(0))\" still shows") : .sending(nil)
        case "invoke":
            guard let found = invocable(try word(0)), enabled(found.node) else { return .waiting("no enabled control is named \"\(try word(0))\"") }
            let value: Tree = found.node.text("kind") == "disclosure" ? .bool(!found.node.flag("expanded")) : .null
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: value))
        case "toggle":
            guard let found = named(try word(0)), enabled(found.node) else { return .waiting("no enabled switch is named \"\(try word(0))\"") }
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: .bool(!found.node.flag("on"))))
        case "type":
            guard let found = named(try word(0)) else { return .waiting("no field is named \"\(try word(0))\"") }
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: .string(try word(1))))
        case "choose":
            guard let send = chooser(try word(0)) else { return .waiting("nothing offers \"\(try word(0))\"") }
            return .sending(send)
        case "resize":
            guard let width = Double(try word(0)) else { throw SelfTestProblem("resize needs a width") }
            guard let found = control({ $0.text("kind") == "aside" }) else { return .waiting("no column can be resized") }
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: .number(width)))
        case "shot": return .recording(try word(0))
        case "panel":
            guard try word(0) == "cancel" else { throw SelfTestProblem("panel needs cancel") }
            return .panel
        case "select":
            guard step.words.count == 2 else { throw SelfTestProblem("select needs two words") }
            return .selecting(step.words)
        case "selected", "selected-contains":
            return .checkingSelected(try word(0), contains: step.verb == "selected-contains")
        case "copy": return .copying(try word(0))
        case "click": return .clicking(try word(0))
        case "press": return .pressing(try word(0))
        case "paste-image": return .pastingImage
        case "select-all": return .keyPress("a")
        case "sleep":
            guard let seconds = Double(try word(0)) else { throw SelfTestProblem("sleep needs a number of seconds") }
            return .sleeping(seconds)
        default: throw SelfTestProblem("Unknown step \(step.verb)")
        }
    }

    /// The tree as an agent reads it: one line for each control, then the words on show.
    public var outline: String {
        var lines: [String] = []
        func walk(_ node: Tree) {
            let name = node.text("label").isEmpty ? node.text("title") : node.text("label")
            if node["event"]?.int != nil {
                lines.append("\(node.text("kind")) \"\(name)\"\(enabled(node) ? "" : " disabled")")
            } else if !node.text("text").isEmpty {
                lines.append("text \"\(node.text("text"))\"")
            }
            Self.inside(node).sorted { $0.fixtureText < $1.fixtureText }.forEach(walk)
        }
        nodes.forEach { walk($0.node) }
        return lines.joined(separator: "\n") + "\n"
    }
}

/// Answers `http` from a file of replies, in order, and keeps each request beside the run.
public struct HttpDoubleAdapter: CapabilityAdapter {
    public let kinds = ["http"]
    private let replies: [Tree]
    private let models: Tree?
    private let requests: URL
    private let position = Position()

    private final class Position: @unchecked Sendable {
        private let lock = NSLock()
        private var next = 0
        func take() -> Int {
            lock.lock()
            defer { lock.unlock() }
            next += 1
            return next - 1
        }
    }

    /// `{"models": {"deepseek-chat": ...}, "replies": [{"status": 200, "body": "...", "delayMs": 0}]}`.
    /// A provider's model list is a GET that takes no reply: `models` answers it for any
    /// provider, and with no `models` it answers what a provider with no such list would.
    public init(file: String, requests: URL) throws {
        guard let value = Tree.fixture(try Data(contentsOf: URL(fileURLWithPath: file))), let replies = value["replies"]?.array else { throw SelfTestProblem("A double holds a list of replies") }
        self.replies = replies
        self.models = value["models"]
        self.requests = requests
    }

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        // A server on this machine is real in a doubled run: only the model is the double.
        if let url = URL(string: try required(ask, text: 2, "url")), HttpAdapter.isLoopback(url) { return try await HttpAdapter().perform(ask, slices: slices) }
        let outgoing = try HttpAdapter.outgoing(ask, timeout: max(try required(ask, whole: 6, "deadline"), 1))
        if outgoing.httpMethod == "GET", outgoing.url?.path.hasSuffix("/models") == true, outgoing.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Bearer ") == true {
            guard let models else { return Self.status(404) }
            slices.write(try required(ask, text: 7, "slice"), models.fixtureText)
            return Self.status(200)
        }
        let index = position.take()
        guard index < replies.count else { throw CapabilityError.platform("The double has no reply for request \(index + 1)") }
        guard outgoing.httpMethod == "POST", outgoing.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Bearer ") == true else {
            throw CapabilityError.notAllowed("The double takes only an authorized POST")
        }
        // A strict double answers the status the real server would, without logging the key.
        let entry = replies[index]
        if let wanted = entry["requireUrl"]?.string, wanted != outgoing.url?.absoluteString {
            return Self.status(404)
        }
        if entry["requireBearer"]?.bool == true, outgoing.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Bearer ") != true {
            return Self.status(401)
        }
        if let delay = entry["delayMs"]?.int { try await Task.sleep(for: .milliseconds(delay)) }
        // The record holds where the request went and what it carried, never the key.
        let record = Tree.object(["url": .string(outgoing.url?.absoluteString ?? ""), "body": .string(try required(ask, text: 4, "body"))])
        let line = Data((record.fixtureText + "\n").utf8)
        if let handle = try? FileHandle(forWritingTo: requests) {
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: line)
            try? handle.close()
        } else {
            try line.write(to: requests)
        }
        slices.write(try required(ask, text: 7, "slice"), entry.text("body"))
        return Self.status(entry.count("status"))
    }

    /// A reply of `code` with no headers, as the real transport's receipt spells it.
    private static func status(_ code: Int) -> AskResult {
        .document(.object(["status": .number(Double(code)), "headers": .array([])]))
    }
}

/// Answers the folder picker, which no script can reach, with one folder.
public struct ChooseFolderDouble: CapabilityAdapter {
    public let kinds = ["choose-folder"]
    private let path: String
    public init(path: String) { self.path = path }

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        .bare(path)
    }
}


// The self-test's own files — its script, the model double's replies and the record of what was
// asked — are JSON documents on disk. Only this test tooling reads or writes them; nothing here
// crosses between the application and the host.
extension Tree {
    /// A script or double file read from disk.
    public static func fixture(_ data: Data) -> Tree? {
        (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])).flatMap(Tree.init(file:))
    }

    private init?(file: Any) {
        switch file {
        case is NSNull: self = .null
        case let number as NSNumber: self = CFGetTypeID(number) == CFBooleanGetTypeID() ? .bool(number.boolValue) : .number(number.doubleValue)
        case let text as String: self = .string(text)
        case let items as [Any]: self = .array(items.compactMap(Tree.init(file:)))
        case let members as [String: Any]: self = .object(members.compactMapValues(Tree.init(file:)))
        default: return nil
        }
    }

    private var plain: Any {
        switch self {
        case .null: return NSNull()
        case let .bool(value): return value
        case let .number(value): return value == value.rounded() && abs(value) < 9e15 ? Int(value) as Any : value as Any
        case let .string(value): return value
        case let .array(items): return items.map(\.plain)
        case let .object(members): return members.mapValues(\.plain)
        }
    }

    /// The compact text a record file or a doubled response body holds, keys sorted.
    var fixtureText: String {
        let data = try? JSONSerialization.data(withJSONObject: plain, options: [.sortedKeys, .withoutEscapingSlashes, .fragmentsAllowed])
        return data.flatMap { String(data: $0, encoding: .utf8) } ?? "null"
    }
}
