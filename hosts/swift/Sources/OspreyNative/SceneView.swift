#if os(iOS)
import SwiftUI
import UIKit

// [UI-SCENE-STREAM] The C boundary stages streamed primitives during each call.
@MainActor
public final class SceneStream: ObservableObject {
    public static let shared = SceneStream()
    @Published var shapes: [JSONValue] = []
    private var pending: [JSONValue] = []
    private let images = NSCache<NSString, UIImage>()
    public func begin() { pending.removeAll(keepingCapacity: true) }
    public func append(_ shape: JSONValue) { pending.append(shape) }
    public func commit() { if !pending.isEmpty { shapes = pending } }
    func image(_ path: String) -> UIImage? {
        if let cached = images.object(forKey: path as NSString) { return cached }
        guard let image = UIImage(contentsOfFile: path) else { return nil }
        images.setObject(image, forKey: path as NSString)
        return image
    }
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
            .compactMap(\.array).sorted { depth($0) < depth($1) }
    }

    private func depth(_ shape: [JSONValue]) -> Double {
        let index = shape.first?.int == 6 ? 6 : shape.count - 1
        return shape.indices.contains(index) ? shape[index].scalar : 0
    }

    var body: some View {
        if width <= 0 || height <= 0 || shapes.contains(where: { ![0, 1, 2, 3, 4, 5, 6].contains($0.first?.int ?? -1) }) {
            Refusal(kind: "scene with unsupported dimensions or primitive")
        } else {
            GeometryReader { geometry in
                ZStack(alignment: .topLeading) {
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
                .accessibilityHidden(true)
                ForEach(Array(node.list("access").enumerated()), id: \.offset) { _, item in
                    accessibleControl(item)
                        .frame(width: max(44, (item["w"]?.scalar ?? 0) * geometry.size.width / width),
                               height: max(44, (item["h"]?.scalar ?? 0) * geometry.size.height / height))
                        .position(x: ((item["x"]?.scalar ?? 0) + (item["w"]?.scalar ?? 0) / 2) * geometry.size.width / width,
                                  y: ((item["y"]?.scalar ?? 0) + (item["h"]?.scalar ?? 0) / 2) * geometry.size.height / height)
                }
                }
            }
            .aspectRatio(width / height, contentMode: .fit)
        }
    }

    private func pointer(x: Double, y: Double, down: Bool) {
        drawing.point(node.count("event"), .object(["x": .number(x), "y": .number(y), "down": .bool(down), "shift": .bool(false)]))
    }

    private func act(_ item: JSONValue, value: Double? = nil) {
        let left = item["x"]?.scalar ?? 0, top = item["y"]?.scalar ?? 0
        let w = item["w"]?.scalar ?? 0, h = item["h"]?.scalar ?? 0
        let least = item["min"]?.scalar ?? 0, most = item["max"]?.scalar ?? 1
        let fraction = value.map { min(1, max(0, ($0 - least) / max(0.000001, most - least))) } ?? 0.5
        for down in [true, false] { pointer(x: left + w * fraction, y: top + h / 2, down: down) }
    }

    @ViewBuilder private func accessibleControl(_ item: JSONValue) -> some View {
            if item.text("role") == "slider" {
                SceneSlider(item: item) { act(item, value: $0) }
            } else if ["button", "popup", "menuitem"].contains(item.text("role")) {
                Button { act(item) } label: { Color.clear.contentShape(Rectangle()) }
                    .buttonStyle(.plain).accessibilityLabel(item.text("label"))
            } else { Color.clear.accessibilityLabel(item.text("label")) }
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
        case 5: vector(shape, context: context)
        case 3 where shape.count == 9:
            let atlas = node.list("atlas"), index = Int(n(1))
            if atlas.indices.contains(index), let path = atlas[index].string, let image = stream.image(path) {
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

    private func vector(_ shape: [JSONValue], context original: GraphicsContext) {
        guard shape.count == 6, let steps = shape[1].array, let outline = shape[2].array,
              let paint = shape[3].array, paint.count == 2, let transform = shape[4].array else { return }
        var path = Path(), context = original
        for value in steps {
            guard let step = value.array, let code = step.first?.int else { return }
            func point(_ index: Int) -> CGPoint { CGPoint(x: step[index].scalar, y: step[index + 1].scalar) }
            switch code {
            case 0 where step.count == 3: path.move(to: point(1))
            case 1 where step.count == 3: path.addLine(to: point(1))
            case 2 where step.count == 5: path.addQuadCurve(to: point(1), control: point(3))
            case 3 where step.count == 7: path.addCurve(to: point(1), control1: point(3), control2: point(5))
            case 5 where step.count >= 4:
                path.move(to: point(2))
                for index in stride(from: 4, to: step.count - 1, by: 2) { path.addLine(to: point(index)) }
                if step[1].int == 1 { path.closeSubpath() }
            case 6: path.closeSubpath()
            default: return
            }
        }
        if transform.count == 3 {
            context.translateBy(x: transform[0].scalar, y: transform[1].scalar)
            context.scaleBy(x: transform[2].scalar, y: transform[2].scalar)
        }
        guard let channels = paint[1].array, channels.count == 5 else { return }
        let ink = GraphicsContext.Shading.color(pathColor(channels))
        if outline.first?.int == 0 { context.fill(path, with: ink) }
        else if outline.count >= 3 { context.stroke(path, with: ink, lineWidth: outline[1].scalar) }
    }

    private func pathColor(_ tint: [JSONValue]) -> Color {
        let a = tint[4].scalar
        if tint[0].int == 0 { return Color(.sRGB, red: tint[1].scalar, green: tint[2].scalar, blue: tint[3].scalar, opacity: a) }
        let hue = tint[1].scalar - floor(tint[1].scalar), s = tint[2].scalar, l = tint[3].scalar
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

private struct SceneSlider: UIViewRepresentable {
    let item: JSONValue
    let changed: (Double) -> Void
    final class Coordinator: NSObject {
        var parent: SceneSlider
        init(_ parent: SceneSlider) { self.parent = parent }
        @objc func moved(_ slider: SceneUISlider) { parent.changed(Double(slider.takeValue())) }
    }
    func makeCoordinator() -> Coordinator { Coordinator(self) }
    // [CAST-DESIGN-NATIVE] UIKit's intrinsic slider height must not shrink the touch frame.
    func sizeThatFits(_ proposal: ProposedViewSize, uiView: SceneUISlider, context: Context) -> CGSize? {
        guard let width = proposal.width, let height = proposal.height else { return nil }
        return CGSize(width: width, height: height)
    }
    func makeUIView(context: Context) -> SceneUISlider {
        let slider = SceneUISlider()
        slider.isContinuous = false
        slider.isAccessibilityElement = true
        slider.accessibilityTraits = .adjustable
        slider.minimumTrackTintColor = .clear
        slider.maximumTrackTintColor = .clear
        let clear = UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1)).image { _ in }
        slider.setThumbImage(clear, for: .normal)
        if #available(iOS 26, *) { slider.sliderStyle = .thumbless }
        slider.addTarget(context.coordinator, action: #selector(Coordinator.moved(_:)), for: .valueChanged)
        return slider
    }
    func updateUIView(_ slider: SceneUISlider, context: Context) {
        context.coordinator.parent = self
        guard !slider.isTracking else { return }
        slider.accessibilityLabel = item.text("label")
        slider.accessibilityIdentifier = item.text("label")
        slider.minimumValue = Float(item["min"]?.scalar ?? 0)
        slider.maximumValue = Float(item["max"]?.scalar ?? 1)
        slider.value = Float(item["value"]?.scalar ?? 0)
    }
}

// [UI-SCENE-STREAM] Preserve the actual endpoint across UIKit's delayed change
// event; non-touch accessibility adjustments keep the native slider's value.
private final class SceneUISlider: UISlider {
    private var touchValue: Float?

    private func remember(_ touch: UITouch) {
        guard bounds.width > 0, maximumValue > minimumValue else { touchValue = nil; return }
        let fraction = min(1, max(0, touch.location(in: self).x / bounds.width))
        touchValue = minimumValue + Float(fraction) * (maximumValue - minimumValue)
    }

    func takeValue() -> Float {
        defer { touchValue = nil }
        return touchValue ?? value
    }

    override func beginTracking(_ touch: UITouch, with event: UIEvent?) -> Bool {
        touchValue = nil
        let accepted = super.beginTracking(touch, with: event)
        if accepted { remember(touch) }
        return accepted
    }
    override func continueTracking(_ touch: UITouch, with event: UIEvent?) -> Bool {
        remember(touch)
        return super.continueTracking(touch, with: event)
    }
    override func endTracking(_ touch: UITouch?, with event: UIEvent?) {
        if let touch { remember(touch) }
        super.endTracking(touch, with: event)
    }
    override func cancelTracking(with event: UIEvent?) {
        touchValue = nil
        super.cancelTracking(with: event)
    }
}
#endif
