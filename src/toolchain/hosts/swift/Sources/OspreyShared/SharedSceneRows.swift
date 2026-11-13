import Foundation
import OspreyViewApp

// Implements [NATIVE-SHM-SCENE] reading, as the GPUI host's `rows.rs` does: scene rows in order,
// with the next one taken on demand; every count is checked against what is left, every code
// against what the host knows, and every float for finiteness. The codes are `native.td`'s.

/// A canvas's scene: its extent, zoom, atlas, shapes and accessibility tree.
public struct SharedScene: Sendable {
    public let width: Double
    public let height: Double
    /// Window pixels per scene unit; 0 fits the scene to its region whole.
    public let zoom: Double
    /// The sprite images the scene names, by atlas index.
    public let atlas: [String]
    public let shapes: [SceneShape]
    public let access: [SceneAccess]

    init(width: Double, height: Double, zoom: Double, atlas: [String], shapes: [SceneShape], access: [SceneAccess]) {
        self.width = width
        self.height = height
        self.zoom = zoom
        self.atlas = atlas
        self.shapes = shapes
        self.access = access
    }
}

/// The roles an accessible element may play, by its row's `index`.
public enum SharedSceneRole: Int64, Sendable {
    case button = 0
    case slider = 1
    case popup = 2
    case menuItem = 3
    /// An element a screen reader announces but a person cannot act on.
    case none = 4
}

/// One element of a scene's accessibility tree, bounds in scene units.
public struct SceneAccess: Sendable {
    public let role: SharedSceneRole
    public let label: String
    public let x: Double, y: Double, w: Double, h: Double
    public let value: Double, min: Double, max: Double
    public let selected: Bool
}

/// A primitive in scene units; the painter applies the shared transform.
public enum SceneShape: Sendable {
    case rect(x: Double, y: Double, w: Double, h: Double, angle: Double, colour: UInt32, depth: Double)
    case disc(x: Double, y: Double, radius: Double, colour: UInt32, depth: Double)
    case strip(points: [(Double, Double)], colour: UInt32, stroke: Double, depth: Double)
    case sprite(atlas: Int, x: Double, y: Double, w: Double, h: Double, angle: Double, tint: UInt32, depth: Double)
    case glyphs(text: String, x: Double, y: Double, size: Double, colour: UInt32, depth: Double)
    case path(steps: [SceneStep], outline: SceneOutline, paint: ScenePaint, moved: (Double, Double, Double)?, depth: Double)
    /// A text run at a weight, anchored at its left (0), centre (1) or right (2).
    case lettering(text: String, x: Double, y: Double, size: Double, colour: UInt32, depth: Double, anchor: UInt8, weight: Double)
}

/// One path-builder command in scene units; curves name their end point first.
public enum SceneStep: Sendable {
    case move(Double, Double)
    case line(Double, Double)
    case curve(to: (Double, Double), control: (Double, Double))
    case cubic(to: (Double, Double), first: (Double, Double), second: (Double, Double))
    case arc(radii: (Double, Double), rotation: Double, large: Bool, sweep: Bool, to: (Double, Double))
    case polygon([(Double, Double)], closed: Bool)
    case close
}

/// Fill, or stroke with a width, bevel or miter joins and an optional dash pattern.
public enum SceneOutline: Sendable {
    case fill
    case stroke(width: Double, bevel: Bool, dashes: [Double])
}

/// A float colour in the space the application gave it.
public enum SceneTint: Sendable {
    case rgb(Double, Double, Double, Double)
    case hsl(Double, Double, Double, Double)
}

/// A solid tint or a two-stop linear gradient, angle in degrees.
public enum ScenePaint: Sendable {
    case solid(SceneTint)
    case linear(angle: Double, from: (SceneTint, Double), to: (SceneTint, Double), oklab: Bool)
}

/// The scene row codes `native.td` declares.
enum SceneCode {
    static let point: Int64 = 7
    static let pathStep: Int64 = 8
    static let outlined: Int64 = 9
    static let painted: Int64 = 10
    static let tinted: Int64 = 11
    static let scene: Int64 = 20
    static let atlasEntry: Int64 = 21
    static let element: Int64 = 22
}

/// SharedScene rows in order, with the next one taken on demand.
final class SceneRows {
    private let rows: UnsafeBufferPointer<OspNativeShape>
    private let run: UnsafeRawBufferPointer
    var next = 0
    /// The canvas row the scene being read belongs to.
    var current: Int?

    init(_ rows: UnsafeBufferPointer<OspNativeShape>, _ run: UnsafeRawBufferPointer) {
        self.rows = rows
        self.run = run
    }

    var left: Int { rows.count - next }

    /// The next row, which must have `code`; its values must be finite.
    func take(_ code: Int64) throws -> OspNativeShape {
        guard next < rows.count, rows[next].code == code else {
            throw SharedRefusal("scene row \(next) should have code \(code)")
        }
        let row = rows[next]
        try SharedRows.finite(row.values)
        next += 1
        return row
    }

    /// A count that must stay inside the rows that follow.
    func count(_ value: Int64) throws -> Int {
        guard value >= 0, value <= Int64(left) else { throw SharedRefusal("a scene count that runs past its rows") }
        return Int(value)
    }

    /// `many` point rows, as pairs.
    func points(_ many: Int64) throws -> [(Double, Double)] {
        let total = try count(many)
        return try (0..<total).map { _ in
            let row = try take(SceneCode.point)
            return (row.values.0, row.values.1)
        }
    }

    /// The text a row's offsets name.
    func text(_ row: OspNativeShape) throws -> String {
        try SharedRows.words(run, row.text_offset, row.text_length)
    }

    private func value(_ row: OspNativeShape, _ at: Int) -> Double {
        [row.values.0, row.values.1, row.values.2, row.values.3, row.values.4][at]
    }

    /// One tint row, whose `index` says RGB or HSL.
    func tint() throws -> SceneTint {
        let row = try take(SceneCode.tinted)
        switch row.index {
        case 0: return .rgb(value(row, 0), value(row, 1), value(row, 2), value(row, 3))
        case 1: return .hsl(value(row, 0), value(row, 1), value(row, 2), value(row, 3))
        default: throw SharedRefusal("an unknown tint")
        }
    }

    /// One path-step row, whose `index` says which step it is.
    func step() throws -> SceneStep {
        let row = try take(SceneCode.pathStep)
        let at = { self.value(row, $0) }
        switch row.index {
        case 0: return .move(at(0), at(1))
        case 1: return .line(at(0), at(1))
        case 2: return .curve(to: (at(0), at(1)), control: (at(2), at(3)))
        case 3:
            let last = try take(SceneCode.point)
            return .cubic(to: (at(0), at(1)), first: (at(2), at(3)), second: (at(4), last.values.0))
        case 4:
            return .arc(radii: (at(0), at(1)), rotation: at(2), large: row.colour & 1 != 0,
                        sweep: row.colour & 2 != 0, to: (at(3), at(4)))
        case 5: return .polygon(try points(row.depth), closed: row.colour & 1 != 0)
        case 6: return .close
        default: throw SharedRefusal("an unknown path step")
        }
    }

    /// One outline row, with its dash points.
    func outline() throws -> SceneOutline {
        let row = try take(SceneCode.outlined)
        switch row.index {
        case 0: return .fill
        case 1: return .stroke(width: value(row, 0), bevel: row.colour & 1 != 0,
                               dashes: try points(row.depth).map(\.0))
        default: throw SharedRefusal("an unknown outline")
        }
    }

    /// One paint row, solid or linear, with the tints that follow it.
    func paint() throws -> ScenePaint {
        let row = try take(SceneCode.painted)
        switch row.index {
        case 0: return .solid(try tint())
        case 1: return .linear(angle: value(row, 0), from: (try tint(), value(row, 1)),
                               to: (try tint(), value(row, 2)), oklab: row.colour & 1 != 0)
        default: throw SharedRefusal("an unknown paint")
        }
    }

    /// A path head row's steps, then its outline and paint.
    private func path(_ head: OspNativeShape) throws -> SceneShape {
        let total = try count(head.index)
        let steps = try (0..<total).map { _ in try step() }
        let moved = (value(head, 0), value(head, 1), value(head, 2))
        return .path(steps: steps, outline: try outline(), paint: try paint(),
                     moved: (moved.0 != 0 || moved.1 != 0 || moved.2 != 1) ? moved : nil,
                     depth: Double(head.depth))
    }

    /// One shape head row, with whatever rows follow it.
    func shape(_ head: OspNativeShape) throws -> SceneShape {
        let depth = Double(head.depth)
        switch head.code {
        case 0: return .rect(x: value(head, 0), y: value(head, 1), w: value(head, 2), h: value(head, 3),
                             angle: value(head, 4), colour: try SharedRows.colour(head.colour), depth: depth)
        case 1: return .disc(x: value(head, 0), y: value(head, 1), radius: value(head, 2),
                             colour: try SharedRows.colour(head.colour), depth: depth)
        case 2: return .strip(points: try points(head.index), colour: try SharedRows.colour(head.colour),
                              stroke: value(head, 2), depth: depth)
        case 3:
            guard head.index >= 0 else { throw SharedRefusal("a sprite index below zero") }
            return .sprite(atlas: Int(head.index), x: value(head, 0), y: value(head, 1), w: value(head, 2),
                           h: value(head, 3), angle: value(head, 4), tint: try SharedRows.colour(head.colour), depth: depth)
        case 4: return .glyphs(text: try text(head), x: value(head, 0), y: value(head, 1), size: value(head, 2),
                               colour: try SharedRows.colour(head.colour), depth: depth)
        case 5: return try path(head)
        case 6:
            guard head.index >= 0, head.index <= 2 else { throw SharedRefusal("an unknown lettering anchor") }
            return .lettering(text: try text(head), x: value(head, 0), y: value(head, 1), size: value(head, 2),
                              colour: try SharedRows.colour(head.colour), depth: depth,
                              anchor: UInt8(head.index), weight: value(head, 3))
        default: throw SharedRefusal("an unknown scene code \(head.code)")
        }
    }

    /// One accessible element, with its range row; role 4 announces only, so it draws nothing.
    func element(_ head: OspNativeShape) throws -> [SceneAccess] {
        let range = try take(SceneCode.point)
        guard let role = SharedSceneRole(rawValue: head.index) else {
            throw SharedRefusal("an unknown accessible role")
        }
        guard role != .none else { return [] }
        return [SceneAccess(role: role, label: try text(head),
                            x: value(head, 0), y: value(head, 1), w: value(head, 2), h: value(head, 3),
                            value: value(head, 4), min: range.values.0, max: range.values.1,
                            selected: head.colour & 1 != 0)]
    }
}
