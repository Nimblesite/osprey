#if os(macOS)
import AppKit
#endif
import SwiftUI

// Implements [UI-COMPONENTS-BOARD]: a writable board whose strokes the host holds under the
// slices `<slice>:<n>`, told as `slice`, `count`, `lasso`, `backdrop`, `panning`, `event` (a
// finished stroke), `lassoEvent` (the strokes a lasso enclosed) and, when the camera is bound,
// `panEvent`. Only a drag in progress lives here: each point of the stroke, the finished stroke,
// each enclosed index and the lasso's close are typed events, and a camera drag reports its
// final absolute offsets as `x,y`.

struct Stroke: Decodable {
    let points: [[Int]]
    let width: Int
    let at: Int
}

struct BoardFrame: Decodable {
    struct Size: Decodable { let width: Int; let height: Int }
    struct Camera: Decodable { let x: Int; let y: Int; let scale: Int }
    let size: Size
    let view: Camera
}

/// Intersects the backdrop's horizontal and vertical edge fades while keeping its broad opaque
/// area, so a reference picture trails into the board without a hard edge.
struct BackdropFade: ViewModifier {
    let enabled: Bool
    private static let horizontal = Gradient(stops: [.init(color: .clear, location: 0), .init(color: .black, location: 0.05), .init(color: .black, location: 0.6), .init(color: .clear, location: 1)])
    private static let vertical = Gradient(stops: [.init(color: .clear, location: 0), .init(color: .black, location: 0.05), .init(color: .black, location: 0.86), .init(color: .clear, location: 1)])

    @ViewBuilder func body(content: Content) -> some View {
        if enabled {
            content.mask(LinearGradient(gradient: Self.horizontal, startPoint: .leading, endPoint: .trailing))
                .mask(LinearGradient(gradient: Self.vertical, startPoint: .top, endPoint: .bottom))
                .blendMode(.lighten)
        } else { content }
    }
}

/// The board: the strokes its slices hold, a drag drawing one more, and with Shift held a lasso
/// that reads the strokes it encloses; a panning camera drag reports its final offsets.
struct BoardLeaf: View {
    let node: Tree
    @EnvironmentObject private var store: RegionStore
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyInherited) private var inherited
    @State private var live: [CGPoint] = []
    @State private var selecting = false
    @State private var cameraTranslation = CGSize.zero

    private var count: Int { node.count("count") }
    private var lassoing: Bool { node.flag("lasso") }
    private var panning: Bool { node.flag("panning") }

    /// Shift is the lasso's modifier, read through the platform's keyboard state.
    private var shiftHeld: Bool {
        #if os(macOS)
        NSEvent.modifierFlags.contains(.shift)
        #else
        false
        #endif
    }

    private var strokes: [Stroke] {
        (0..<max(count, 0)).compactMap { try? JSONDecoder().decode(Stroke.self, from: Data(SliceStore.shared.text("\(node.text("slice")):\($0)").utf8)) }
    }

    private var metadata: BoardFrame? {
        try? JSONDecoder().decode(BoardFrame.self, from: Data(SliceStore.shared.text(node.text("slice")).utf8))
    }

    private var frame: CGSize {
        CGSize(width: metadata?.size.width ?? 1536, height: metadata?.size.height ?? 1024)
    }

    private var chalk: Color { theme.color("chalk", dark: scheme == .dark) ?? .primary }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / frame.width, proxy.size.height / frame.height) * CGFloat(metadata?.view.scale ?? 100) / 100
            let offset = CGPoint(x: (proxy.size.width - frame.width * scale) / 2 + CGFloat(metadata?.view.x ?? 0) * scale + cameraTranslation.width,
                                 y: (proxy.size.height - frame.height * scale) / 2 + CGFloat(metadata?.view.y ?? 0) * scale + cameraTranslation.height)
            Canvas { context, _ in
                for item in strokes {
                    context.stroke(path(item.points.map { CGPoint(x: CGFloat($0[0]), y: CGFloat($0[1])) }, scale, offset), with: .color(chalk), style: StrokeStyle(lineWidth: CGFloat(item.width) * scale, lineCap: .round, lineJoin: .round))
                }
                let ink = selecting ? theme.color("accent", dark: scheme == .dark) ?? .accentColor : chalk
                context.stroke(path(live, scale, offset), with: .color(ink), style: StrokeStyle(lineWidth: selecting ? 1.5 : 3 * scale, lineCap: .round, lineJoin: .round, dash: selecting ? [6, 4] : []))
            }
            .background { backdrop(at: offset, scale: scale) }
            .clipped()
            .gesture(DragGesture(minimumDistance: 0).onChanged { change in
                if panning { cameraTranslation = change.translation; return }
                if live.isEmpty { selecting = lassoing && shiftHeld }
                let point = CGPoint(x: (change.location.x - offset.x) / scale, y: (change.location.y - offset.y) / scale)
                live.append(point)
                // Each point of the stroke arrives as its own typed event, as the application accumulates.
                if !selecting { store.shared.send(Touch(kind: TouchKind.point, x: point.x.rounded(), y: point.y.rounded())) }
            }.onEnded { _ in
                if panning { finishCamera(scale); return }
                finish()
            })
            .accessibilityLabel("Drawing board, \(count) strokes")
        }
        .frame(minHeight: 280)
    }

    @ViewBuilder private func backdrop(at offset: CGPoint, scale: CGFloat) -> some View {
        if let picture = Bundled.image(node.text("backdrop")) {
            picture.resizable().frame(width: frame.width * scale, height: frame.height * scale)
                .modifier(BackdropFade(enabled: inherited.fadeBackdrop))
                .position(x: offset.x + frame.width * scale / 2, y: offset.y + frame.height * scale / 2)
        } else {
            theme.color("board", dark: scheme == .dark) ?? theme.color("canvas", dark: scheme == .dark) ?? Color.clear
        }
    }

    private func path(_ points: [CGPoint], _ scale: CGFloat, _ offset: CGPoint) -> Path {
        var path = Path()
        guard let first = points.first else { return path }
        path.move(to: CGPoint(x: first.x * scale + offset.x, y: first.y * scale + offset.y))
        points.dropFirst().forEach { path.addLine(to: CGPoint(x: $0.x * scale + offset.x, y: $0.y * scale + offset.y)) }
        if points.count == 1 { path.addLine(to: CGPoint(x: first.x * scale + offset.x + 0.1, y: first.y * scale + offset.y)) }
        return path
    }

    /// The stroke's points have arrived one by one; its finish carries its width and the clock.
    private func finish() {
        let points = live
        live = []
        guard !points.isEmpty else { return }
        if selecting {
            selecting = false
            return enclosed(by: points).forEach { store.shared.send(Touch(kind: TouchKind.enclose, ordinal: Int64($0))) }
        }
        store.shared.send(Touch(kind: TouchKind.stroke, ordinal: Int64(node.count("event")),
                                x: 3, y: Double(Int(Date().timeIntervalSince1970 * 1000))))
    }

    /// A camera drag reports its final absolute offsets, leaving the application to draw the next.
    private func finishCamera(_ scale: CGFloat) {
        let x = (metadata?.view.x ?? 0) + Int((cameraTranslation.width / scale).rounded())
        let y = (metadata?.view.y ?? 0) + Int((cameraTranslation.height / scale).rounded())
        cameraTranslation = .zero
        store.shared.send(Touch.control(Int64(node.count("panEvent")), "\(x),\(y)"))
    }

    /// The indices of the strokes with any point inside the lasso.
    private func enclosed(by loop: [CGPoint]) -> [Int] {
        var outline = Path()
        outline.addLines(loop)
        outline.closeSubpath()
        return strokes.enumerated().compactMap { index, item in
            item.points.contains { outline.contains(CGPoint(x: CGFloat($0[0]), y: CGFloat($0[1]))) } ? index : nil
        }
    }
}
