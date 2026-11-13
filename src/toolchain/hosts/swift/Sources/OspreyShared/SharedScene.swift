import SwiftUI

// Implements [NATIVE-SHM-SCENE]'s input half, ported from the iOS JSON host's `SceneView.swift`
// to scene rows: pointer, key and view measurements in scene units over the canvas the scene's
// marker named, and the accessible elements a screen reader announces. An assistive action
// becomes the pointer press and release a mouse would send, so the application cannot tell a
// screen reader or an agent from a hand.

/// A canvas row with the scene its frame gave it. Without a scene it reserves its flexible
/// extent and draws nothing of its own.
@MainActor
struct SharedCanvas: View {
    let store: SharedStore
    let index: Int
    /// The control number the scene's input delivers.
    let ordinal: Int64

    var body: some View {
        Group {
            if let scene = store.scene(index) {
                drawn(scene)
            } else {
                Color.clear
            }
        }
        .frame(maxWidth: .infinity, minHeight: 120, maxHeight: .infinity)
    }

    /// The canvas, its input handlers and its accessible elements.
    private func drawn(_ scene: SharedScene) -> some View {
        GeometryReader { space in
                let transform = SharedTransform(size: space.size, scene: scene)
                ZStack(alignment: .topLeading) {
                    Canvas { context, _ in
                        SharedScenePainter(scene: scene, transform: transform).draw(in: context)
                    }
                    SharedSceneAccess(store: store, scene: scene, transform: transform, ordinal: ordinal)
                }
                .contentShape(Rectangle())
                .gesture(drag(transform))
                .onKeyPress(phases: .all) { keyed($0) }
                .focusable()
                .onAppear { viewed(scene, transform) }
                .onChange(of: space.size) { _, _ in viewed(scene, transform) }
            }
    }

    /// The pointer over the scene, down while the gesture holds, in scene units.
    private func drag(_ transform: SharedTransform) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { event in
                let at = transform.scene(event.location)
                store.send(Touch.pointer(ordinal, x: at.x, y: at.y, down: true, shift: false))
            }
            .onEnded { event in
                let at = transform.scene(event.location)
                store.send(Touch.pointer(ordinal, x: at.x, y: at.y, down: false, shift: false))
            }
    }

    /// Admits only the host-neutral key vocabulary the scene contract names, as the GPUI host's
    /// `key_name` does.
    private func keyed(_ press: KeyPress) -> KeyPress.Result {
        guard let name = SharedCanvas.keyName(press) else { return .ignored }
        store.send(Touch.key(ordinal, name, press.phase == .down))
        return .handled
    }

    private static func keyName(_ press: KeyPress) -> String? {
        let named: [String: String] = ["\u{F700}": "up", "\u{F701}": "down", "\u{F702}": "left", "\u{F703}": "right",
                                       " ": "space", "\r": "enter", "\u{1B}": "escape", "\t": "tab", "\u{7F}": "backspace"]
        let words = String(press.characters)
        if let name = named[words] { return name }
        guard words.count == 1, let first = words.utf8.first else { return nil }
        let lower = first >= UInt8(ascii: "a") && first <= UInt8(ascii: "z")
        let digit = first >= UInt8(ascii: "0") && first <= UInt8(ascii: "9")
        return lower || digit ? words : nil
    }

    /// Tells the application its scene's region is now so many scene units wide and tall, on
    /// the next runloop tick rather than from inside a layout pass. Only a sized scene has a
    /// scale to measure with.
    private func viewed(_ scene: SharedScene, _ transform: SharedTransform) {
        guard scene.width > 0, transform.scale > 0 else { return }
        let (width, height) = (transform.viewed.width, transform.viewed.height)
        Task { @MainActor in
            store.send(Touch.viewed(ordinal, width: width, height: height))
        }
    }
}

/// The scene's accessible elements: an invisible control per element, at its bounds in scene
/// units, acting as the pointer press and release a hand would send at the same place.
@MainActor
private struct SharedSceneAccess: View {
    let store: SharedStore
    let scene: SharedScene
    let transform: SharedTransform
    let ordinal: Int64

    var body: some View {
        ForEach(Array(scene.access.enumerated()), id: \.offset) { _, element in
            SharedSceneElement(store: store, element: element, transform: transform, ordinal: ordinal)
        }
    }
}

@MainActor
private struct SharedSceneElement: View {
    let store: SharedStore
    let element: SceneAccess
    let transform: SharedTransform
    let ordinal: Int64

    var body: some View {
        let frame = placed
        switch element.role {
        case .slider:
            Slider(value: Binding(get: { element.value }, set: { _ in }), in: element.min...max(element.max, element.min + 1),
                   onEditingChanged: { moving in if !moving { act(value: element.value) } })
                .frame(width: frame.width, height: frame.height)
                .position(x: frame.midX, y: frame.midY)
                .accessibilityLabel(element.label)
        default:
            Button { act(value: nil) } label: { Color.clear.contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .frame(width: frame.width, height: frame.height)
                .position(x: frame.midX, y: frame.midY)
                .accessibilityLabel(element.label)
                .accessibilityAddTraits(element.role == .menuItem && element.selected ? .isSelected : [])
        }
    }

    /// The element's box in window pixels, at least a fingertip wide and tall.
    private var placed: CGRect {
        CGRect(x: transform.point(element.x), y: transform.point(element.y),
               width: max(44, transform.point(element.w)), height: max(44, transform.point(element.h)))
    }

    /// A press and release at the element's centre, or at its value's place along a slider.
    private func act(value: Double?) {
        let fraction = value.map { min(1, max(0, ($0 - element.min) / max(0.000001, element.max - element.min))) } ?? 0.5
        let x = element.x + element.w * fraction
        let y = element.y + element.h / 2
        for down in [true, false] {
            store.send(Touch.pointer(ordinal, x: x, y: y, down: down, shift: false))
        }
    }
}
