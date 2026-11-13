import Foundation
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW] and [NATIVE-SHM-ASK] checking, as the GPUI host's `rows.rs` does:
// nothing from the application is trusted. Every kind and code must be known, every parent must
// be an earlier row, every text range must lie inside the text run on UTF-8 boundaries, every
// float must be finite and every count must stay inside the rows that follow it. A frame that
// fails any check is refused whole and the last good view stays.

/// What a row draws, by its `kind` ([NATIVE-SHM-VIEW]).
public enum SharedKind: Int64, CaseIterable, Sendable {
    case column = 0, line, label, press, entry, gap, scroll, layer, toggle
    case picker, choice, rule, image, canvas, editor, menu, sheet
    /// [NATIVE-SHM-NODES] One told value, a record of members and a list of items.
    case field = 33, record, list

    /// Whether the row tells a component rather than draws a control.
    public var told: Bool { self == .field || self == .record || self == .list }
}

/// The `flags` bits a row's fit adds up ([NATIVE-SHM-VIEW]).
public enum SharedFlag: Sendable {
    public static let disabled: Int64 = 1
    public static let chosen: Int64 = 2
    public static let secret: Int64 = 4
    public static let fixedWidth: Int64 = 8
    public static let slanted: Int64 = 16
    public static let centred: Int64 = 32
    public static let onCommit: Int64 = 64
    /// Every flag a row may carry; anything outside it refuses the frame.
    public static let all: Int64 = 127
}

/// How one control looks, every value decided in Osprey: `0xRRGGBBAA` ink and fill, sizes in points.
public struct Look: Equatable, Sendable {
    public let ink: UInt32
    public let fill: UInt32
    public let size: Double
    public let padding: Double
    public let spacing: Double
    public let radius: Double
    public let weight: Int64
}

/// How one control sits: fixed extents (0 = natural), its share of free space, flags and border.
public struct Fit: Equatable, Sendable {
    public let width: Double
    public let height: Double
    public let grow: Double
    public let flags: Int64
    public let border: UInt32
}

/// One checked control and the rows drawn inside it.
public struct RowControl: Equatable, Sendable {
    public let kind: SharedKind
    /// The row this control sits in, or -1 when it has none.
    public let parent: Int64
    /// The control number a host event names, when the control delivers one.
    public let action: Int64?
    public let words: String
    /// A field's prompt, an option's or tab's key, or the icon a label or press is led by.
    public let hint: String
    public let look: Look
    public let fit: Fit
    /// A told field's whole number or flag; 0 for every other row.
    public let value: Int64
    public var children: [Int]
}

/// One field of an ask: its argument position, number and text.
public struct AskField: Equatable, Sendable {
    public let field: Int64
    public let number: Int64
    public let text: String

    public init(field: Int64, number: Int64, text: String) {
        self.field = field
        self.number = number
        self.text = text
    }
}

/// The capability codes that are not capabilities: a slice write and a refused event's diagnostic.
public let sharedSliceWrite: Int64 = 100
public let sharedDiagnostic: Int64 = 101

/// A capability request or a slice write: its code, its ticket (a slice write's key) and its fields.
public struct Ask: Equatable, Sendable {
    public let code: Int64
    public let ticket: String
    /// The ask's arguments by position; built while its rows are read.
    public var fields: [AskField]

    public init(code: Int64, ticket: String, fields: [AskField]) {
        self.code = code
        self.ticket = ticket
        self.fields = fields
    }

    /// The capability's name, or empty for a code this host does not know.
    public var kind: String {
        let at = Int(code - 1)
        return sharedCapabilities.indices.contains(at) ? sharedCapabilities[at] : ""
    }

    /// Every text at argument `field`, in order: a list's items or a pair's halves.
    public func texts(_ field: Int64) -> [String] {
        fields.filter { $0.field == field }.map(\.text)
    }

    /// The first text at argument `field`, or empty.
    public func text(_ field: Int64) -> String {
        texts(field).first ?? ""
    }

    /// The whole number at argument `field`, or 0.
    public func number(_ field: Int64) -> Int64 {
        fields.first { $0.field == field }?.number ?? 0
    }

    /// The flag at argument `field`.
    public func flag(_ field: Int64) -> Bool {
        number(field) != 0
    }

    /// The name/value pairs at argument `field` (headers or environment variables).
    public func pairs(_ field: Int64) -> [(String, String)] {
        let items = texts(field)
        return stride(from: 0, to: items.count, by: 2).map { at in
            (items[at], at + 1 < items.count ? items[at + 1] : "")
        }
    }
}

/// The capability names, by code from 1, as the application's `Requests::kind` spells them.
public let sharedCapabilities = [
    "run", "list-directories", "read-plists", "read-texts", "fingerprint", "store-read",
    "store-write", "store-list", "store-delete", "http", "host-call", "running-applications",
    "choose-file", "choose-folder", "save-file", "measure-folders", "reveal", "open-settings",
    "cancel", "recognise", "animate", "process-start", "process-write", "process-read", "process-stop",
]

/// A checked frame: its controls (a row's index is its position), the rows with no parent,
/// each canvas's scene by its row, and its asks.
public struct Shown: Sendable {
    public let controls: [RowControl]
    public let roots: [Int]
    public let scenes: [Int: SharedScene]
    public let asks: [Ask]

    /// The capability requests to answer, in order.
    public var requests: [Ask] { asks.filter { (1...25).contains($0.code) } }

    /// The slice writes, as key and text.
    public var slices: [(key: String, text: String)] {
        asks.filter { $0.code == sharedSliceWrite }.map { ($0.ticket, $0.fields.first?.text ?? "") }
    }

    /// Why the application refused the event this frame answers, if it did.
    public var diagnostics: [String] {
        asks.filter { $0.code == sharedDiagnostic }.map(\.ticket)
    }

    public static let empty = Shown(controls: [], roots: [], scenes: [:], asks: [])
}

/// Why a frame was refused: one fact that did not hold, in the words a person reads.
public struct SharedRefusal: Error, CustomStringConvertible {
    public let why: String

    public init(_ why: String) { self.why = why }

    public var description: String { why }
}

/// Checks and reads one sealed frame's runs.
public enum SharedRows {
    /// Text bytes `offset..offset + length` of the run, checked as UTF-8.
    static func words(_ run: UnsafeRawBufferPointer, _ offset: Int64, _ length: Int64) throws -> String {
        guard offset >= 0, length >= 0 else { throw SharedRefusal("a text range below zero") }
        let start = Int(offset)
        let count = Int(length)
        guard start <= run.count, count <= run.count - start else {
            throw SharedRefusal("a text range outside the frame")
        }
        guard let text = String(bytes: UnsafeRawBufferPointer(rebasing: run[start..<start + count]), encoding: .utf8) else {
            throw SharedRefusal("text that is not UTF-8")
        }
        return text
    }

    /// A size the host can lay out: finite and never negative.
    static func extent(_ value: Double) throws -> Double {
        guard value.isFinite, value >= 0 else { throw SharedRefusal("an extent that is negative or not finite") }
        return value
    }

    /// A packed `0xRRGGBBAA` colour.
    static func colour(_ value: Int64) throws -> UInt32 {
        guard value >= 0, value <= 0xFFFF_FFFF else { throw SharedRefusal("a colour outside 0xRRGGBBAA") }
        return UInt32(value)
    }

    /// One control, checked. Unknown kinds, unknown flags, a weight outside 100 to 900, a
    /// negative extent or a non-finite number each refuse the frame whole.
    private static func control(_ row: OspNativeView, _ index: Int, _ run: UnsafeRawBufferPointer) throws -> RowControl {
        guard let kind = SharedKind(rawValue: row.kind) else {
            throw SharedRefusal("row \(index) has an unknown kind \(row.kind)")
        }
        if kind.told { return try told(kind, row, run) }
        guard row.flags >= 0, row.flags & ~SharedFlag.all == 0 else {
            throw SharedRefusal("row \(index) has unknown flags \(row.flags)")
        }
        guard (100...900).contains(row.weight) else {
            throw SharedRefusal("row \(index) has a font weight outside 100 to 900")
        }
        return RowControl(kind: kind,
                          parent: row.parent,
                          action: row.action >= 0 ? row.action : nil,
                          words: try words(run, row.text_offset, row.text_length),
                          hint: try words(run, row.hint_offset, row.hint_length),
                          look: Look(ink: try colour(row.colour), fill: try colour(row.background),
                                     size: try extent(row.size), padding: try extent(row.padding),
                                     spacing: try extent(row.spacing), radius: try extent(row.radius),
                                     weight: row.weight),
                          fit: Fit(width: try extent(row.width), height: try extent(row.height),
                                   grow: try extent(row.grow), flags: row.flags,
                                   border: try colour(row.border)),
                          value: 0,
                          children: [])
    }

    /// One told row ([NATIVE-SHM-NODES]): its name is its hint, a field's value is its words, its
    /// whole number or its real, and its weight says which (0 to 4). Anything else refuses the frame.
    private static func told(_ kind: SharedKind, _ row: OspNativeView, _ run: UnsafeRawBufferPointer) throws -> RowControl {
        guard (0...4).contains(row.weight) else { throw SharedRefusal("a told value of an unknown type \(row.weight)") }
        return RowControl(kind: kind, parent: row.parent, action: nil,
                          words: try words(run, row.text_offset, row.text_length),
                          hint: try words(run, row.hint_offset, row.hint_length),
                          look: Look(ink: 0, fill: 0, size: try extent(row.size), padding: 0, spacing: 0, radius: 0, weight: row.weight),
                          fit: Fit(width: 0, height: 0, grow: 0, flags: 0, border: 0),
                          value: row.colour,
                          children: [])
    }

    /// The controls, each child listed under its parent, and the rows with no parent.
    private static func controls(_ rows: UnsafeBufferPointer<OspNativeView>, _ run: UnsafeRawBufferPointer) throws -> ([RowControl], [Int]) {
        var all: [RowControl] = []
        var roots: [Int] = []
        for index in rows.indices {
            let row = rows[index]
            if row.parent == -1 {
                roots.append(index)
            } else {
                guard row.parent >= 0, Int(row.parent) < index else {
                    throw SharedRefusal("row \(index) names a parent that is not an earlier row")
                }
                all[Int(row.parent)].children.append(index)
            }
            all.append(try control(row, index, run))
        }
        return (all, roots)
    }

    /// The asks: a field-0 row starts one, and the rows after it are its fields.
    private static func asks(_ rows: UnsafeBufferPointer<OspNativeAsk>, _ run: UnsafeRawBufferPointer) throws -> [Ask] {
        var all: [Ask] = []
        for row in rows {
            let text = try words(run, row.text_offset, row.text_length)
            if row.field == 0 {
                all.append(Ask(code: row.number, ticket: text, fields: []))
            } else {
                guard row.field > 0, var last = all.popLast() else {
                    throw SharedRefusal("an ask field before any ask, or below zero")
                }
                last.fields.append(AskField(field: row.field, number: row.number, text: text))
                all.append(last)
            }
        }
        guard all.allSatisfy({ (1...25).contains($0.code) || $0.code == sharedSliceWrite || $0.code == sharedDiagnostic }) else {
            throw SharedRefusal("an ask with an unknown capability code")
        }
        return all
    }

    /// Checks and reads one sealed frame.
    public static func read(shapes: UnsafeBufferPointer<OspNativeShape>,
                            views: UnsafeBufferPointer<OspNativeView>,
                            text run: UnsafeRawBufferPointer,
                            asks asked: UnsafeBufferPointer<OspNativeAsk>) throws -> Shown {
        let (controls, roots) = try controls(views, run)
        let canvases = controls.indices.filter { controls[$0].kind == .canvas || controls[$0].kind == .record }
        return Shown(controls: controls, roots: roots,
                     scenes: try scenes(shapes, run, canvases),
                     asks: try asks(asked, run))
    }

    /// Every scene, by the canvas row its marker names ([NATIVE-SHM-SCENE]). A marker's `index`
    /// must name a canvas row no scene has claimed; the rows after it belong to that canvas.
    private static func scenes(_ rows: UnsafeBufferPointer<OspNativeShape>,
                               _ run: UnsafeRawBufferPointer,
                               _ canvases: [Int]) throws -> [Int: SharedScene] {
        var all: [Int: SceneBuilder] = [:]
        let at = SceneRows(rows, run)
        while at.next < rows.count {
            let head = rows[at.next]
            try finite(head.values)
            at.next += 1
            if head.code == 20 {
                let row = Int(head.index)
                guard head.index >= 0, canvases.contains(row), all[row] == nil else {
                    throw SharedRefusal("a scene that names no canvas, or one twice")
                }
                all[row] = SceneBuilder(width: try extent(head.values.0), height: try extent(head.values.1),
                                        zoom: try extent(head.values.2))
                at.current = row
                continue
            }
            guard let current = at.current else { throw SharedRefusal("a scene row before any scene") }
            switch head.code {
            case 21: all[current]?.atlas.append(try at.text(head))
            case 22: all[current]?.access.append(contentsOf: try at.element(head))
            default: all[current]?.shapes.append(try at.shape(head))
            }
        }
        return all.mapValues { $0.built }
    }

    /// One scene while its rows are still arriving.
    private struct SceneBuilder {
        let width: Double, height: Double, zoom: Double
        var atlas: [String] = []
        var shapes: [SceneShape] = []
        var access: [SceneAccess] = []

        var built: SharedScene {
            SharedScene(width: width, height: height, zoom: zoom, atlas: atlas, shapes: shapes, access: access)
        }
    }

    /// Every value a row carries must be finite, whatever it means.
    static func finite(_ values: (Double, Double, Double, Double, Double)) throws {
        guard values.0.isFinite, values.1.isFinite, values.2.isFinite, values.3.isFinite, values.4.isFinite else {
            throw SharedRefusal("a number that is not finite")
        }
    }
}
