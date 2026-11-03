#if os(iOS)
import SwiftUI
import UIKit

// [UI-SCENE-STREAM] The C boundary stages streamed primitives during each call.
@MainActor
public final class SceneStream: ObservableObject {
    public static let shared = SceneStream()
    @Published var shapes: [JSONValue] = []
    private var pending: [JSONValue] = []
    public func begin() { pending.removeAll(keepingCapacity: true) }
    public func append(_ shape: JSONValue) { pending.append(shape) }
    public func commit() { if !pending.isEmpty { shapes = pending } }
}

private extension JSONValue {
    var scalar: Double { if case let .number(value) = self { return value }; return 0 }
}

// [CAST-MOBILE-DELIVERY] The same artboard, pointer coordinates and accessible
// controls used on desktop, fitted into the phone's safe content area.
struct SceneView: View {
    let node: JSONValue
    let drawing: Drawing
    @ObservedObject private var stream = SceneStream.shared
    private var width: Double { node["width"]?.scalar ?? 0 }
    private var height: Double { node["height"]?.scalar ?? 0 }
    private var shapes: [[JSONValue]] {
        (node.list("shapes") + (node.count("streamed") > 0 ? stream.shapes : []))
            .compactMap(\.array).sorted { ($0.last?.scalar ?? 0) < ($1.last?.scalar ?? 0) }
    }

    var body: some View {
        if width <= 0 || height <= 0 || shapes.contains(where: { ![0, 1, 2, 3, 4, 6].contains($0.first?.int ?? -1) }) {
            Refusal(kind: "scene with unsupported dimensions or primitive")
        } else {
            GeometryReader { geometry in
                Canvas { context, size in
                    context.scaleBy(x: size.width / width, y: size.height / height)
                    for shape in shapes { paint(shape, context: context) }
                }
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0).onChanged { event in
                    pointer(x: event.location.x * width / geometry.size.width,
                            y: event.location.y * height / geometry.size.height, down: true)
                }.onEnded { event in
                    pointer(x: event.location.x * width / geometry.size.width,
                            y: event.location.y * height / geometry.size.height, down: false)
                })
                .accessibilityRepresentation { accessibility }
            }
            .aspectRatio(width / height, contentMode: .fit)
        }
    }

    private func pointer(x: Double, y: Double, down: Bool) {
        drawing.send(node.count("event"), .object(["x": .number(x), "y": .number(y), "down": .bool(down), "shift": .bool(false)]))
    }

    private func act(_ item: JSONValue, value: Double? = nil) {
        let left = item["x"]?.scalar ?? 0, top = item["y"]?.scalar ?? 0
        let w = item["w"]?.scalar ?? 0, h = item["h"]?.scalar ?? 0
        let least = item["min"]?.scalar ?? 0, most = item["max"]?.scalar ?? 1
        let fraction = value.map { min(1, max(0, ($0 - least) / max(0.000001, most - least))) } ?? 0.5
        for down in [true, false] { pointer(x: left + w * fraction, y: top + h / 2, down: down) }
    }

    @ViewBuilder private var accessibility: some View {
        ForEach(Array(node.list("access").enumerated()), id: \.offset) { _, item in
            if item.text("role") == "slider", item.flag("movable") {
                Slider(value: Binding(get: { item["value"]?.scalar ?? 0 }, set: { act(item, value: $0) }),
                       in: (item["min"]?.scalar ?? 0)...max(item["min"]?.scalar ?? 0, item["max"]?.scalar ?? 1)) {
                    Text(item.text("label"))
                }
            } else if item.text("role") == "button" || item.text("role") == "popup" {
                Button(item.text("label")) { act(item) }
            } else { Text(item.text("label")) }
        }
    }

    private func color(_ value: Double) -> Color {
        let bits = UInt32(clamping: Int64(value))
        return Color(.sRGB, red: Double((bits >> 24) & 255) / 255,
                     green: Double((bits >> 16) & 255) / 255,
                     blue: Double((bits >> 8) & 255) / 255, opacity: Double(bits & 255) / 255)
    }

    private func paint(_ shape: [JSONValue], context original: GraphicsContext) {
        guard let code = shape.first?.int else { return }
        func n(_ i: Int) -> Double { shape.indices.contains(i) ? shape[i].scalar : 0 }
        var context = original
        switch code {
        case 0 where shape.count == 8:
            context.translateBy(x: n(1), y: n(2)); context.rotate(by: .radians(n(5)))
            context.fill(Path(CGRect(x: -n(3) / 2, y: -n(4) / 2, width: n(3), height: n(4))), with: .color(color(n(6))))
        case 1 where shape.count == 6:
            context.fill(Path(ellipseIn: CGRect(x: n(1) - n(3), y: n(2) - n(3), width: n(3) * 2, height: n(3) * 2)), with: .color(color(n(4))))
        case 2: strip(shape, context: context)
        case 3 where shape.count == 9:
            let atlas = node.list("atlas"), index = Int(n(1))
            if atlas.indices.contains(index), let path = atlas[index].string, let image = UIImage(contentsOfFile: path) {
                context.translateBy(x: n(2), y: n(3)); context.rotate(by: .radians(n(6)))
                context.opacity = Double(UInt32(clamping: Int64(n(7))) & 255) / 255
                context.draw(Image(uiImage: image), in: CGRect(x: -n(4) / 2, y: -n(5) / 2, width: n(4), height: n(5)))
            }
        case 4, 6:
            if shape.count >= 7, let text = shape[1].string {
                let anchor: UnitPoint = code == 6 && n(7) == 1 ? .top : code == 6 && n(7) == 2 ? .topTrailing : .topLeading
                let weight: Font.Weight = shape.count > 8 && n(8) >= 600 ? .semibold : .regular
                context.draw(Text(text).font(.system(size: n(4), weight: weight)).foregroundColor(color(n(5))), at: CGPoint(x: n(2), y: n(3)), anchor: anchor)
            }
        default: break
        }
    }

    private func strip(_ shape: [JSONValue], context: GraphicsContext) {
        guard shape.count >= 5, let count = shape[1].int, count > 0, shape.count == count * 2 + 5 else { return }
        var path = Path()
        for index in 0..<count {
            let point = CGPoint(x: shape[2 + index * 2].scalar, y: shape[3 + index * 2].scalar)
            if index == 0 { path.move(to: point) } else { path.addLine(to: point) }
        }
        context.stroke(path, with: .color(color(shape[count * 2 + 2].scalar)), lineWidth: shape[count * 2 + 3].scalar)
    }
}
#endif
