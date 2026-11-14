import SwiftUI

#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

// Implements [NATIVE-SHM-SCENE] painting, ported from the iOS JSON host's `SceneView.swift` to
// scene rows: rects, discs, strips, sprites from the atlas, glyphs, lettering and paths, every
// colour, size and width from the row. Shapes draw back to front by depth, equal depths in
// drawing order; the painter applies one shared transform — the scene's zoom, or a fit of the
// scene whole — and each path's own move and scale before it.

/// The scene's zoom and top-left origin, shared by painting and pointer input.
struct SharedTransform {
    let scale: CGFloat
    /// The region's size in scene units, which the application is told when it changes.
    let viewed: (width: Double, height: Double)

    /// Places the scene at its fixed `zoom`, or with zoom 0 as large as fits the region whole;
    /// a fixed zoom never magnifies the scene past what its region fits whole either, so a
    /// narrow window sees the whole artboard instead of clipping it. A zero-sized scene is in
    /// window pixels.
    init(size: CGSize, scene: SharedScene) {
        guard scene.width > 0, scene.height > 0 else {
            scale = 1
            viewed = (size.width, size.height)
            return
        }
        let fit = min(size.width / scene.width, size.height / scene.height)
        let chosen = scene.zoom == 0 ? fit : min(scene.zoom, fit)
        scale = CGFloat(chosen)
        viewed = (size.width / chosen, size.height / chosen)
    }

    /// One scene-space length in window pixels.
    func point(_ value: Double) -> CGFloat { CGFloat(value) * scale }

    /// One window-space position in scene units.
    func scene(_ position: CGPoint) -> (x: Double, y: Double) {
        (Double(position.x) / Double(scale), Double(position.y) / Double(scale))
    }
}

/// A scene's sprite images, decoded once, by the atlas path that named them.
final class SharedSprites: @unchecked Sendable {
    static let shared = SharedSprites()
    private let lock = NSLock()
    private var images: [String: PlatformImage?] = [:]

    func image(_ path: String) -> PlatformImage? {
        lock.lock()
        defer { lock.unlock() }
        if let held = images[path] { return held }
        #if os(macOS)
        let loaded = NSImage(contentsOfFile: path)
        #else
        let loaded = UIImage(contentsOfFile: path)
        #endif
        images[path] = loaded
        return loaded
    }
}

#if os(macOS)
typealias PlatformImage = NSImage
#else
typealias PlatformImage = UIImage
#endif

extension GraphicsContext {
    /// One scene's checked shapes, back to front by depth, equal depths in drawing order.
    static func ordered(_ scene: SharedScene) -> [SceneShape] {
        scene.shapes.enumerated().sorted { left, right in
            depth(left.element) != depth(right.element) ? depth(left.element) < depth(right.element) : left.offset < right.offset
        }.map(\.element)
    }

    private static func depth(_ shape: SceneShape) -> Double {
        switch shape {
        case let .rect(_, _, _, _, _, _, depth): return depth
        case let .disc(_, _, _, _, depth): return depth
        case let .strip(_, _, _, depth): return depth
        case let .sprite(_, _, _, _, _, _, _, depth): return depth
        case let .glyphs(_, _, _, _, _, depth): return depth
        case let .path(_, _, _, _, depth): return depth
        case let .lettering(_, _, _, _, _, _, _, depth): return depth
        }
    }
}

/// The scene painter: draws one checked scene at one transform. Nothing here reads the
/// application again; every value came from the checked rows.
struct SharedScenePainter {
    let scene: SharedScene
    let transform: SharedTransform

    /// The scene's shapes, stably depth-sorted once.
    private let shapes: [SceneShape]

    init(scene: SharedScene, transform: SharedTransform) {
        self.scene = scene
        self.transform = transform
        shapes = GraphicsContext.ordered(scene)
    }

    func draw(in context: GraphicsContext) {
        var painter = context
        painter.scaleBy(x: transform.scale, y: transform.scale)
        shapes.forEach { paint($0, into: &painter) }
    }

    private func paint(_ shape: SceneShape, into context: inout GraphicsContext) {
        switch shape {
        case let .rect(x, y, w, h, angle, colour, _):
            var box = context
            box.translateBy(x: transform.point(x), y: transform.point(y))
            box.rotate(by: .radians(angle))
            box.fill(Path(CGRect(x: -transform.point(w) / 2, y: -transform.point(h) / 2,
                                 width: transform.point(w), height: transform.point(h))),
                     with: .color(SharedPaint.colour(colour) ?? .clear))
        case let .disc(x, y, radius, colour, _):
            let side = transform.point(radius * 2)
            context.fill(Path(ellipseIn: CGRect(x: transform.point(x - radius), y: transform.point(y - radius),
                                                width: side, height: side)),
                         with: .color(SharedPaint.colour(colour) ?? .clear))
        case let .strip(points, colour, stroke, _):
            guard points.count >= 2 else { return }
            var path = Path()
            path.move(to: CGPoint(x: transform.point(points[0].0), y: transform.point(points[0].1)))
            points.dropFirst().forEach { path.addLine(to: CGPoint(x: transform.point($0.0), y: transform.point($0.1))) }
            context.stroke(path, with: .color(SharedPaint.colour(colour) ?? .clear),
                           style: StrokeStyle(lineWidth: transform.point(stroke), lineCap: .round, lineJoin: .round))
        case let .sprite(atlas, x, y, w, h, angle, tint, _):
            sprite(atlas, x, y, w, h, angle, tint, into: &context)
        case let .glyphs(text, x, y, size, colour, _):
            context.draw(Text(text)
                .font(.system(size: transform.point(size)))
                .foregroundColor(SharedPaint.colour(colour) ?? .clear),
                at: CGPoint(x: transform.point(x), y: transform.point(y)), anchor: .topLeading)
        case let .path(steps, outline, paint, moved, _):
            drawPath(steps, outline, paint, moved, into: &context)
        case let .lettering(text, x, y, size, colour, _, anchor, weight):
            context.draw(Text(text)
                .font(.system(size: transform.point(size), weight: SharedPaint.weight(Int64(weight))))
                .foregroundColor(SharedPaint.colour(colour) ?? .clear),
                at: CGPoint(x: transform.point(x), y: transform.point(y)),
                anchor: anchor == 1 ? .top : anchor == 2 ? .topTrailing : .topLeading)
        }
    }

    /// A sprite from the scene's atlas, turned by its angle and worn to its tint's opacity; a
    /// missing sprite draws its placeholder quad, tinted so atlas indexes stay distinguishable.
    private func sprite(_ atlas: Int, _ x: Double, _ y: Double, _ w: Double, _ h: Double,
                        _ angle: Double, _ tint: UInt32, into context: inout GraphicsContext) {
        guard scene.atlas.indices.contains(atlas), let image = SharedSprites.shared.image(scene.atlas[atlas]) else {
            let side = CGRect(x: transform.point(x - w / 2), y: transform.point(y - h / 2),
                              width: transform.point(w), height: transform.point(h))
            context.fill(Path(side), with: .color(placeholder(atlas, tint)))
            return
        }
        var box = context
        box.translateBy(x: transform.point(x), y: transform.point(y))
        box.rotate(by: .radians(angle))
        box.opacity = Double(tint & 0xFF) / 255
        #if os(macOS)
        let drawn = Image(nsImage: image)
        #else
        let drawn = Image(uiImage: image)
        #endif
        box.draw(drawn, in: CGRect(x: -transform.point(w) / 2, y: -transform.point(h) / 2,
                                   width: transform.point(w), height: transform.point(h)))
    }

    /// Makes an untextured opaque-white sprite distinguishable by its atlas index.
    private func placeholder(_ atlas: Int, _ tint: UInt32) -> Color {
        if tint != 0xFFFF_FFFF { return SharedPaint.colour(tint) ?? .clear }
        return [Color(.sRGB, red: 0.53, green: 0.81, blue: 0.92),
                Color(.sRGB, red: 1, green: 0.65, blue: 0),
                Color(.sRGB, red: 0.33, green: 0.67, blue: 0.33)][atlas % 3]
    }

    /// Replays a path's steps, then its own transform and its outline over its paint.
    private func drawPath(_ steps: [SceneStep], _ outline: SceneOutline, _ paint: ScenePaint,
                          _ moved: (Double, Double, Double)?, into context: inout GraphicsContext) {
        var path = Path()
        var at: (Double, Double)?
        steps.forEach { step in
            (path, at) = SharedPath.add(step, to: path, from: at)
        }
        guard let moved else {
            paintPath(path, outline, paint, into: &context)
            return
        }
        var box = context
        box.translateBy(x: transform.point(moved.0), y: transform.point(moved.1))
        box.scaleBy(x: CGFloat(moved.2), y: CGFloat(moved.2))
        paintPath(path, outline, paint, into: &box)
    }

    private func paintPath(_ path: Path, _ outline: SceneOutline, _ paint: ScenePaint, into context: inout GraphicsContext) {
        let shading = self.shading(paint)
        switch outline {
        case .fill:
            context.fill(path, with: shading)
        case let .stroke(width, bevel, dashes):
            var style = StrokeStyle(lineWidth: transform.point(width), lineJoin: bevel ? .bevel : .miter)
            if !dashes.isEmpty {
                style.dash = dashes.map { transform.point($0) }
                style.dashPhase = 0
            }
            context.stroke(path, with: shading, style: style)
        }
    }

    /// A solid tint, or a two-stop linear gradient across the scene, along the angle the row named.
    private func shading(_ paint: ScenePaint) -> GraphicsContext.Shading {
        switch paint {
        case let .solid(tint):
            return .color(SharedTints.colour(tint))
        case let .linear(angle, from, to, _):
            let radians = angle * .pi / 180
            let (dx, dy) = (cos(radians) / 2, sin(radians) / 2)
            let (width, height) = (max(scene.width, 1), max(scene.height, 1))
            return .linearGradient(Gradient(colors: [SharedTints.colour(from.0), SharedTints.colour(to.0)]),
                                   startPoint: CGPoint(x: width * (0.5 - dx), y: height * (0.5 - dy)),
                                   endPoint: CGPoint(x: width * (0.5 + dx), y: height * (0.5 + dy)))
        }
    }
}

/// SharedScene tints, in the float space the row gave them.
enum SharedTints {
    static func colour(_ tint: SceneTint) -> Color {
        switch tint {
        case let .rgb(r, g, b, a):
            return Color(.sRGB, red: r, green: g, blue: b, opacity: a)
        case let .hsl(h, s, l, a):
            return hsl(h - floor(h), s, l, a)
        }
    }

    /// HSL as the sRGB the painter draws, as the iOS host spelled it.
    private static func hsl(_ hue: Double, _ s: Double, _ l: Double, _ a: Double) -> Color {
        let c = (1 - abs(2 * l - 1)) * s, m = l - c / 2
        let x = c * (1 - abs((hue * 6).truncatingRemainder(dividingBy: 2) - 1))
        let rgb: (Double, Double, Double)
        switch Int(hue * 6) {
        case 0: rgb = (c, x, 0)
        case 1: rgb = (x, c, 0)
        case 2: rgb = (0, c, x)
        case 3: rgb = (0, x, c)
        case 4: rgb = (x, 0, c)
        default: rgb = (c, 0, x)
        }
        return Color(.sRGB, red: rgb.0 + m, green: rgb.1 + m, blue: rgb.2 + m, opacity: a)
    }
}

/// One path step replayed onto a SwiftUI path; an arc lands as the cubics that spell it.
enum SharedPath {
    static func add(_ step: SceneStep, to path: Path, from at: (Double, Double)?) -> (Path, (Double, Double)?) {
        var path = path
        var here = at ?? (0, 0)
        switch step {
        case let .move(x, y):
            path.move(to: CGPoint(x: x, y: y))
            here = (x, y)
        case let .line(x, y):
            if path.isEmpty { path.move(to: CGPoint(x: here.0, y: here.1)) }
            path.addLine(to: CGPoint(x: x, y: y))
            here = (x, y)
        case let .curve(to, control):
            if path.isEmpty { path.move(to: CGPoint(x: here.0, y: here.1)) }
            path.addQuadCurve(to: CGPoint(x: to.0, y: to.1), control: CGPoint(x: control.0, y: control.1))
            here = to
        case let .cubic(to, first, second):
            if path.isEmpty { path.move(to: CGPoint(x: here.0, y: here.1)) }
            path.addCurve(to: CGPoint(x: to.0, y: to.1),
                          control1: CGPoint(x: first.0, y: first.1),
                          control2: CGPoint(x: second.0, y: second.1))
            here = to
        case let .arc(radii, rotation, large, sweep, to):
            (path, here) = SharedPath.arc(path, from: here, radii: radii, rotation: rotation,
                                          large: large, sweep: sweep, to: to)
        case let .polygon(corners, closed):
            guard let first = corners.first else { break }
            path.move(to: CGPoint(x: first.0, y: first.1))
            corners.dropFirst().forEach { path.addLine(to: CGPoint(x: $0.0, y: $0.1)) }
            if closed { path.closeSubpath() }
            here = corners.last ?? first
        case .close:
            path.closeSubpath()
        }
        return (path, here)
    }

    /// One SVG arc as cubic curves, by the specification's endpoint parameterization: each
    /// segment of at most a quarter turn, its controls on the arc's own tangents.
    private static func arc(_ path: Path, from: (Double, Double), radii: (Double, Double), rotation: Double,
                            large: Bool, sweep: Bool, to: (Double, Double)) -> (Path, (Double, Double)) {
        var path = path
        guard radii.0 > 0, radii.1 > 0, from.0 != to.0 || from.1 != to.1 else {
            if path.isEmpty { path.move(to: CGPoint(x: from.0, y: from.1)) }
            path.addLine(to: CGPoint(x: to.0, y: to.1))
            return (path, to)
        }
        let centred = SharedPath.centred(from, to, radii, rotation, large, sweep)
        let segments = max(1, Int(ceil(abs(centred.sweep) / (.pi / 2))))
        let handle = 4.0 / 3.0 * tan(centred.sweep / (4 * Double(segments)))
        if path.isEmpty { path.move(to: CGPoint(x: from.0, y: from.1)) }
        var here = from
        for step in 1...segments {
            let (start, end) = (centred.start + centred.sweep * Double(step - 1) / Double(segments),
                                centred.start + centred.sweep * Double(step) / Double(segments))
            let (p1, t1) = (SharedPath.placed(SharedPath.point(start, on: centred), centred),
                             SharedPath.tangent(start, on: centred))
            let (p2, t2) = (SharedPath.placed(SharedPath.point(end, on: centred), centred),
                             SharedPath.tangent(end, on: centred))
            path.addCurve(to: CGPoint(x: p2.0, y: p2.1),
                          control1: CGPoint(x: p1.0 + handle * t1.0, y: p1.1 + handle * t1.1),
                          control2: CGPoint(x: p2.0 - handle * t2.0, y: p2.1 - handle * t2.1))
            here = p2
        }
        return (path, here)
    }

    /// A point on the centred arc's ellipse, in its own local frame.
    private static func point(_ angle: Double, on arc: CentredArc) -> (Double, Double) {
        (arc.rx * cos(angle), arc.ry * sin(angle))
    }

    /// The arc's tangent direction at `angle`, in its own local frame.
    private static func tangent(_ angle: Double, on arc: CentredArc) -> (Double, Double) {
        (-arc.rx * sin(angle), arc.ry * cos(angle))
    }

    /// One ellipse-local point placed in scene units, by the arc's centre and rotation.
    private static func placed(_ local: (Double, Double), _ arc: CentredArc) -> (Double, Double) {
        (arc.centre.0 + local.0 * arc.cosPhi - local.1 * arc.sinPhi,
         arc.centre.1 + local.0 * arc.sinPhi + local.1 * arc.cosPhi)
    }

    /// The centred form of an arc: centre, radii, start angle and signed sweep, as the SVG
    /// specification's endpoint-to-centre conversion spells them.
    private static func centred(_ from: (Double, Double), _ to: (Double, Double), _ radii: (Double, Double),
                                _ rotation: Double, _ large: Bool, _ sweep: Bool) -> CentredArc {
        let phi = rotation * .pi / 180
        let (cosPhi, sinPhi) = (cos(phi), sin(phi))
        let (mx, my) = ((from.0 + to.0) / 2, (from.1 + to.1) / 2)
        let (dx, dy) = (cosPhi * (from.0 - mx) / 2 + sinPhi * (from.1 - my) / 2,
                        -sinPhi * (from.0 - mx) / 2 + cosPhi * (from.1 - my) / 2)
        let grown = sqrt(max(0, (dx * dx) / (radii.0 * radii.0) + (dy * dy) / (radii.1 * radii.1)))
        let (rx, ry) = (radii.0 * max(1, grown), radii.1 * max(1, grown))
        let sign: Double = large != sweep ? 1 : -1
        let denominator = rx * rx * dy * dy + ry * ry * dx * dx
        let coefficient = denominator > 0 ? sign * sqrt(max(0, (rx * rx * ry * ry - denominator) / denominator)) : 0
        let (cxPrime, cyPrime) = (coefficient * rx * dy / ry, -coefficient * ry * dx / rx)
        let centre = (cosPhi * cxPrime - sinPhi * cyPrime + mx, sinPhi * cxPrime + cosPhi * cyPrime + my)
        let angle = { (ux: Double, uy: Double, vx: Double, vy: Double) in
            let dot = ux * vx + uy * vy
            let length = sqrt((ux * ux + uy * uy) * (vx * vx + vy * vy))
            var theta = acos(min(1, max(-1, dot / max(length, .leastNonzeroMagnitude))))
            if ux * vy - uy * vx < 0 { theta = -theta }
            return theta
        }
        let start = angle(1, 0, (dx - cxPrime) / rx, (dy - cyPrime) / ry)
        var sweepAngle = angle((dx - cxPrime) / rx, (dy - cyPrime) / ry, (-dx - cxPrime) / rx, (-dy - cyPrime) / ry)
        if !sweep && sweepAngle > 0 { sweepAngle -= 2 * .pi }
        if sweep && sweepAngle < 0 { sweepAngle += 2 * .pi }
        return CentredArc(centre: centre, rx: rx, ry: ry, cosPhi: cosPhi, sinPhi: sinPhi, start: start, sweep: sweepAngle)
    }

    /// An arc's centred form, in radians and scene units.
    private struct CentredArc {
        let centre: (Double, Double)
        let rx: Double, ry: Double
        let cosPhi: Double, sinPhi: Double
        let start: Double, sweep: Double
    }
}
