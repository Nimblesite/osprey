import Foundation

// Implements [UI-PROTOCOL] and [UI-COMPONENTS]: the envelope and the typed
// node tree Osprey returns. Unknown kinds, roles and tones are decoding
// errors, never a drawn fallback. Nothing here knows what a node means.

public enum ProtocolError: Error, Equatable {
    case unknownKind(String)
    case unknownValue(String, String)
    case unsupportedVersion(Int)
    case missingMember(String, String)
}

public enum TextRole: String, Decodable, Sendable { case display, large, title, heading, lead, headline, body, callout, caption, eyebrow, mono }
public enum Tone: String, Decodable, Sendable { case primary, secondary, accent, good, caution, danger }
public enum ButtonRole: String, Decodable, Sendable { case prominent, plain, destructive, filled }
public enum Axis: String, Decodable, Sendable { case vertical, horizontal }
public enum Side: String, Decodable, Sendable { case leading, trailing }
public enum Commit: String, Decodable, Sendable { case edit, submit }

extension Commit {
    /// An edit commit sends each change that differs from the committed value.
    public func edited(_ next: String, from value: String) -> Bool { self == .edit && next != value }

    /// A submit commit sends only on submission, and only a change.
    public func submitted(_ draft: String, from value: String) -> Bool { self == .submit && draft != value }
}

public struct Choice: Decodable, Sendable, Equatable {
    public let key: String
    public let label: String
}

public struct TabEntry: Decodable, Sendable, Equatable {
    public let key: String
    public let label: String
    public let symbol: String
    public let child: Node
}

public struct RowEntry: Decodable, Sendable, Equatable {
    public let key: String
    public let symbol: String
    public let group: String
    public let child: Node
}

/// A control's binding: the depth-first ordinal the framework issued for it.
public typealias Ordinal = Int

/// An aside's splitter ([UI-COMPONENTS-SPLITTER]): the column's width in points, the limits Osprey keeps it
/// within, the least width the pane beside it keeps, and the ordinal that receives a dragged width.
public struct Splitter: Equatable, Sendable {
    public let width: Int
    public let least: Int
    public let most: Int
    public let keep: Int
    public let event: Ordinal
}

public indirect enum Node: Sendable, Equatable {
    case text(String, TextRole, Tone)
    case richText([TextRun], TextRole)
    case evidence(label: String, value: String)
    case badge(label: String, symbol: String, tone: Tone)
    case label(String, symbol: String)
    case icon(String)
    case link(label: String, url: URL)
    case spacer
    case divider
    case segments([Tone], label: String)
    case empty(symbol: String, title: String, detail: String)
    case progress(label: String, detail: String)
    case swatch(name: String, light: String, dark: String)
    case slot(String)
    case brand
    case button(label: String, symbol: String, role: ButtonRole, enabled: Bool, event: Ordinal)
    case field(label: String, value: String, prompt: String, secure: Bool, multiline: Bool, commit: Commit, event: Ordinal)
    case toggle(label: String, on: Bool, event: Ordinal)
    case picker(label: String, choices: [Choice], selected: String, event: Ordinal)
    case alert(title: String, message: String, visible: Bool, event: Ordinal)
    case disclosure(title: String, expanded: Bool, event: Ordinal, [Node])
    case stack(Axis, [Node])
    case section(title: String, footer: String, [Node])
    case screen(title: String, subtitle: String, toolbar: [Node], [Node])
    case split([Node])
    case layer([Node])
    case framed(FrameStyle, Node)
    case themed(Theme, Node)
    case dialog(title: String, body: Node, event: Ordinal)
    case aside(pane: Node, side: Side, aside: Node, splitter: Splitter)
    case tabs([TabEntry], selected: String, event: Ordinal)
    case rows([RowEntry], selected: String, event: Ordinal)
    case editor(label: String, value: String, inline: Bool, cursor: Int, focusEpoch: Int = 0, marks: [Mark], event: Ordinal, placeEvent: Ordinal)
    case board(slice: String, count: Int, lasso: Bool, backdrop: String, panning: Bool, panEvent: Ordinal?, event: Ordinal, lassoEvent: Ordinal)
    case math(MathTree, spoken: String)
    case image(source: String, alt: String, width: Int, height: Int)
    case video(source: String, poster: String, captions: String, alt: String, seconds: Int)
    case contextMenu(body: Node, items: [Node])
}

extension Node: Decodable {
    private enum Key: String, CodingKey {
        case kind, text, role, tone, label, value, symbol, tones, title, detail, name, light, dark, key, enabled, event
        case prompt, secure, multiline, commit, on, choices, selected, message, visible, axis, footer, children, subtitle, toolbar, tabs, rows, expanded
        case side, pane, aside, url, width, least, most, keep
        case cursor, focusEpoch, marks, placeEvent, slice, count, lasso, lassoEvent, math, spoken, source, alt, height, poster, captions, seconds, body, items, style, theme, backdrop, runs, panning, panEvent, inline
    }

    public init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: Key.self)
        let kind = try box.decode(String.self, forKey: .kind)
        switch kind {
        case "rich-text": self = .richText(try box.decode([TextRun].self, forKey: .runs), try box.decode(TextRole.self, forKey: .role))
        case "text", "evidence", "badge", "segments", "empty", "progress", "swatch", "slot", "brand", "label", "icon", "link", "spacer", "divider": self = try Node.leaf(kind, box)
        case "button", "field", "toggle", "picker", "alert", "disclosure", "tabs", "rows": self = try Node.control(kind, box)
        case "stack": self = .stack(try box.decode(Axis.self, forKey: .axis), try box.decode([Node].self, forKey: .children))
        case "section": self = .section(title: try box.decode(String.self, forKey: .title), footer: try box.decode(String.self, forKey: .footer), try box.decode([Node].self, forKey: .children))
        case "screen": self = .screen(title: try box.decode(String.self, forKey: .title), subtitle: try box.decode(String.self, forKey: .subtitle), toolbar: try box.decode([Node].self, forKey: .toolbar), try box.decode([Node].self, forKey: .children))
        case "split": self = .split(try box.decode([Node].self, forKey: .children))
        case "layer": self = .layer(try box.decode([Node].self, forKey: .children))
        case "themed": self = .themed(try box.decode(Theme.self, forKey: .theme), try box.decode(Node.self, forKey: .body))
        case "dialog": self = .dialog(title: try box.decode(String.self, forKey: .title), body: try box.decode(Node.self, forKey: .body), event: try box.decode(Int.self, forKey: .event))
        case "framed": self = .framed(try box.decode(FrameStyle.self, forKey: .style), try box.decode(Node.self, forKey: .body))
        case "editor", "board", "math", "image", "video", "context-menu": self = try Node.rich(kind, box)
        case "aside": self = .aside(pane: try box.decode(Node.self, forKey: .pane), side: try box.decode(Side.self, forKey: .side), aside: try box.decode(Node.self, forKey: .aside), splitter: try Node.splitter(box))
        default: throw ProtocolError.unknownKind(kind)
        }
    }

    private static func leaf(_ kind: String, _ box: KeyedDecodingContainer<Key>) throws -> Node {
        switch kind {
        case "text": return .text(try box.decode(String.self, forKey: .text), try box.decode(TextRole.self, forKey: .role), try box.decode(Tone.self, forKey: .tone))
        case "evidence": return .evidence(label: try box.decode(String.self, forKey: .label), value: try box.decode(String.self, forKey: .value))
        case "badge": return .badge(label: try box.decode(String.self, forKey: .label), symbol: try box.decode(String.self, forKey: .symbol), tone: try box.decode(Tone.self, forKey: .tone))
        case "segments": return .segments(try box.decode([Tone].self, forKey: .tones), label: try box.decode(String.self, forKey: .label))
        case "empty": return .empty(symbol: try box.decode(String.self, forKey: .symbol), title: try box.decode(String.self, forKey: .title), detail: try box.decode(String.self, forKey: .detail))
        case "progress": return .progress(label: try box.decode(String.self, forKey: .label), detail: try box.decode(String.self, forKey: .detail))
        case "swatch": return .swatch(name: try box.decode(String.self, forKey: .name), light: try box.decode(String.self, forKey: .light), dark: try box.decode(String.self, forKey: .dark))
        case "slot": return .slot(try box.decode(String.self, forKey: .key))
        case "brand": return .brand
        case "label": return .label(try box.decode(String.self, forKey: .label), symbol: try box.decode(String.self, forKey: .symbol))
        case "icon": return .icon(try box.decode(String.self, forKey: .symbol))
        case "link": return .link(label: try box.decode(String.self, forKey: .label), url: try secure(try box.decode(String.self, forKey: .url)))
        case "spacer": return .spacer
        case "divider": return .divider
        default: throw ProtocolError.unknownKind(kind)
        }
    }

    /// Only a secure web page with a host: a link must not open a local file, another app or a settings pane.
    private static func secure(_ address: String) throws -> URL {
        guard let url = URL(string: address), url.scheme == "https", url.host?.isEmpty == false else { throw ProtocolError.unknownValue("link", address) }
        return url
    }

    private static func splitter(_ box: KeyedDecodingContainer<Key>) throws -> Splitter {
        Splitter(width: try box.decode(Int.self, forKey: .width), least: try box.decode(Int.self, forKey: .least), most: try box.decode(Int.self, forKey: .most), keep: try box.decode(Int.self, forKey: .keep), event: try box.decode(Int.self, forKey: .event))
    }

    private static func editorFocusEpoch(_ box: KeyedDecodingContainer<Key>) throws -> Int {
        guard box.contains(.focusEpoch) else { return 0 }
        let value = try box.decode(Int.self, forKey: .focusEpoch)
        guard (0...9_007_199_254_740_991).contains(value) else {
            throw DecodingError.dataCorruptedError(forKey: .focusEpoch, in: box, debugDescription: "Editor focus epoch must be a nonnegative safe integer")
        }
        return value
    }

    // [UI-COMPONENTS-EDITOR] [UI-COMPONENTS-BOARD] [UI-COMPONENTS-MATH] and the media components.
    private static func rich(_ kind: String, _ box: KeyedDecodingContainer<Key>) throws -> Node {
        switch kind {
        case "editor": return .editor(label: try box.decode(String.self, forKey: .label), value: try box.decode(String.self, forKey: .value), inline: try box.decodeIfPresent(Bool.self, forKey: .inline) ?? false, cursor: try box.decode(Int.self, forKey: .cursor), focusEpoch: try editorFocusEpoch(box), marks: try box.decode([Mark].self, forKey: .marks), event: try box.decode(Int.self, forKey: .event), placeEvent: try box.decode(Int.self, forKey: .placeEvent))
        case "board": return .board(slice: try box.decode(String.self, forKey: .slice), count: try box.decode(Int.self, forKey: .count), lasso: try box.decode(Bool.self, forKey: .lasso), backdrop: try box.decodeIfPresent(String.self, forKey: .backdrop) ?? "", panning: try box.decodeIfPresent(Bool.self, forKey: .panning) ?? false, panEvent: try box.decodeIfPresent(Int.self, forKey: .panEvent), event: try box.decode(Int.self, forKey: .event), lassoEvent: try box.decode(Int.self, forKey: .lassoEvent))
        case "math": return .math(try box.decode(MathTree.self, forKey: .math), spoken: try box.decode(String.self, forKey: .spoken))
        case "image": return .image(source: try box.decode(String.self, forKey: .source), alt: try box.decode(String.self, forKey: .alt), width: try box.decode(Int.self, forKey: .width), height: try box.decode(Int.self, forKey: .height))
        case "video": return .video(source: try box.decode(String.self, forKey: .source), poster: try box.decode(String.self, forKey: .poster), captions: try box.decode(String.self, forKey: .captions), alt: try box.decode(String.self, forKey: .alt), seconds: try box.decode(Int.self, forKey: .seconds))
        default: return .contextMenu(body: try box.decode(Node.self, forKey: .body), items: try box.decode([Node].self, forKey: .items))
        }
    }

    private static func control(_ kind: String, _ box: KeyedDecodingContainer<Key>) throws -> Node {
        let event = try box.decode(Int.self, forKey: .event)
        switch kind {
        case "button": return .button(label: try box.decode(String.self, forKey: .label), symbol: try box.decode(String.self, forKey: .symbol), role: try box.decode(ButtonRole.self, forKey: .role), enabled: try box.decode(Bool.self, forKey: .enabled), event: event)
        case "field": return .field(label: try box.decode(String.self, forKey: .label), value: try box.decode(String.self, forKey: .value), prompt: try box.decode(String.self, forKey: .prompt), secure: try box.decode(Bool.self, forKey: .secure), multiline: try box.decode(Bool.self, forKey: .multiline), commit: try box.decode(Commit.self, forKey: .commit), event: event)
        case "toggle": return .toggle(label: try box.decode(String.self, forKey: .label), on: try box.decode(Bool.self, forKey: .on), event: event)
        case "picker": return .picker(label: try box.decode(String.self, forKey: .label), choices: try box.decode([Choice].self, forKey: .choices), selected: try box.decode(String.self, forKey: .selected), event: event)
        case "alert": return .alert(title: try box.decode(String.self, forKey: .title), message: try box.decode(String.self, forKey: .message), visible: try box.decode(Bool.self, forKey: .visible), event: event)
        case "disclosure": return .disclosure(title: try box.decode(String.self, forKey: .title), expanded: try box.decode(Bool.self, forKey: .expanded), event: event, try box.decode([Node].self, forKey: .children))
        case "tabs": return .tabs(try box.decode([TabEntry].self, forKey: .tabs), selected: try box.decode(String.self, forKey: .selected), event: event)
        default: return .rows(try box.decode([RowEntry].self, forKey: .rows), selected: try box.decode(String.self, forKey: .selected), event: event)
        }
    }
}

/// One editor annotation: a 1-based line, whether it is underlined, its tone, a short badge and the full note.
public struct Mark: Decodable, Sendable, Equatable {
    public let line: Int
    public let underlined: Bool
    public let tone: Tone
    public let badge: String
    public let note: String
}

/// A typeset formula as Osprey's layout tree ([UI-COMPONENTS-MATH]); unknown kinds are decoding errors.
public indirect enum MathTree: Sendable, Equatable {
    case sym(String)
    case num(String)
    case seq([MathTree])
    case sup(MathTree, MathTree)
    case sub(MathTree, MathTree)
    case frac(MathTree, MathTree)
    case root(MathTree)
    case binder(String, [MathTree], MathTree)
    case op(String, MathTree, MathTree)
    case paren(MathTree)
    case big(String, MathTree, MathTree, MathTree)
}

extension MathTree: Decodable {
    private enum Key: String, CodingKey { case kind, text, items, base, script, top, bottom, body, symbol, names, left, right, lower, upper }

    public init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: Key.self)
        let kind = try box.decode(String.self, forKey: .kind)
        func tree(_ key: Key) throws -> MathTree { try box.decode(MathTree.self, forKey: key) }
        func text(_ key: Key) throws -> String { try box.decode(String.self, forKey: key) }
        switch kind {
        case "sym": self = .sym(try text(.text))
        case "num": self = .num(try text(.text))
        case "seq": self = .seq(try box.decode([MathTree].self, forKey: .items))
        case "sup": self = .sup(try tree(.base), try tree(.script))
        case "sub": self = .sub(try tree(.base), try tree(.script))
        case "frac": self = .frac(try tree(.top), try tree(.bottom))
        case "root": self = .root(try tree(.body))
        case "binder": self = .binder(try text(.symbol), try box.decode([MathTree].self, forKey: .names), try tree(.body))
        case "op": self = .op(try text(.symbol), try tree(.left), try tree(.right))
        case "paren": self = .paren(try tree(.body))
        case "big": self = .big(try text(.symbol), try tree(.lower), try tree(.upper), try tree(.body))
        default: throw ProtocolError.unknownKind("math " + kind)
        }
    }
}

public struct Patch: Decodable, Sendable, Equatable {
    public let key: String
    public let revision: Int
    public let node: Node
}

public struct SliceText: Decodable, Sendable, Equatable {
    public let key: String
    public let text: String
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

/// A capability request as Osprey encoded it. Adapters read the members they need; the kind is exhaustive in `Capabilities`.
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

    /// Decodes one library result; a protocol version this backend does not speak is refused.
    public static func decode(_ text: String) throws -> Envelope {
        let envelope = try JSONDecoder().decode(Envelope.self, from: Data(text.utf8))
        guard envelope.version == Envelope.version else { throw ProtocolError.unsupportedVersion(envelope.version) }
        return envelope
    }
}
