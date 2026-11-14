import SwiftUI

// Implements [NATIVE-SHM-VIEW]: draws one checked frame by walking the controls from the root
// and drawing each control's children where its kind puts them. Every colour, size and space
// comes from the row; a hint names a press's or label's icon, a field's prompt or an option's
// key. Below the rows sits the host's own diagnostic strip.

/// The direction a container lays its children out in, which decides the way a child's flex
/// share stretches.
public enum SharedAxis {
    case vertical, horizontal, none
}

/// The host's view of one checked frame, with its diagnostic strip.
public struct SharedView: View {
    @ObservedObject private var store: SharedStore
    @FocusState private var focus: Int?

    public init(store: SharedStore) {
        self.store = store
    }

    public var body: some View {
        VStack(spacing: 0) {
            SharedPage(store: store, focus: $focus)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            SharedDiagnostics(store: store)
        }
    }
}

/// The rows with no parent, top to bottom.
@MainActor
private struct SharedPage: View {
    @ObservedObject var store: SharedStore
    @FocusState.Binding var focus: Int?

    var body: some View {
        if store.shown.roots.isEmpty {
            Color.clear
        } else {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(store.shown.roots, id: \.self) { root in
                    SharedRow(store: store, index: root, axis: .vertical, focus: $focus)
                }
            }
        }
    }
}

/// The host's diagnostic strip: why a frame or a capability was refused, newest last, only the
/// latest lines so a burst cannot push the application off screen.
@MainActor
struct SharedDiagnostics: View {
    @ObservedObject var store: SharedStore
    @State private var open = true

    var body: some View {
        let messages = store.diagnostics
        if !messages.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                Button(messages.count == 1 ? "Diagnostics (1)" : "Diagnostics (\(messages.count)) \(open ? "▾" : "▸")") {
                    withAnimation { open.toggle() }
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .font(.callout)
                if open {
                    ForEach(Array(messages.suffix(6).enumerated()), id: \.offset) { _, message in
                        Text(message).font(.callout).foregroundStyle(.red)
                    }
                }
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .top) { Rectangle().fill(.separator).frame(height: 1) }
        }
    }
}

/// One control and, recursively, the controls that name it as their parent.
@MainActor
struct SharedRow: View {
    @ObservedObject var store: SharedStore
    let index: Int
    let axis: SharedAxis
    @FocusState.Binding var focus: Int?

    var body: some View {
        if let control = store.control(index) {
            SharedLook(control).dressed(drawn(control), axis: axis)
        }
    }

    /// The control this row is. An unknown kind never reaches here: the checker refused it.
    private func drawn(_ control: RowControl) -> AnyView {
        switch control.kind {
        case .column: return column(control)
        case .line: return line(control)
        case .gap: return gap()
        case .scroll: return scroll(control)
        case .layer: return layer(control)
        case .rule: return SharedLeaf(store: store, index: index, focus: $focus).rule(control)
        case .canvas: return SharedCanvas(store: store, index: index, ordinal: control.action ?? -1).erase()
        case .menu: return menu(control)
        case .sheet: return sheet(control)
        default: return SharedLeaf(store: store, index: index, focus: $focus).draw(control)
        }
    }

    /// The controls that name this one as their parent, drawn in order.
    func kids(_ axis: SharedAxis) -> some View {
        ForEach(store.kids(index), id: \.self) { child in
            SharedRow(store: store, index: child, axis: axis, focus: $focus)
        }
    }

    private func column(_ control: RowControl) -> AnyView {
        AnyView(VStack(alignment: .leading, spacing: SharedLook.usable(control.look.spacing)) { kids(.vertical) })
    }

    private func line(_ control: RowControl) -> AnyView {
        AnyView(HStack(alignment: .center, spacing: SharedLook.usable(control.look.spacing)) { kids(.horizontal) })
    }

    /// A gap takes the free space along its parent's axis, as a share of one.
    private func gap() -> AnyView {
        AnyView(Spacer(minLength: 0).layoutPriority(1))
    }

    private func scroll(_ control: RowControl) -> AnyView {
        AnyView(ScrollView(.vertical) {
            VStack(alignment: .leading, spacing: SharedLook.usable(control.look.spacing)) { kids(.vertical) }
        })
    }

    /// A layer's children lie on each other, the last in front.
    private func layer(_ control: RowControl) -> AnyView {
        AnyView(ZStack(alignment: .topLeading) { kids(.none) })
    }

    /// A menu shows its first child and opens the rest as context-menu items.
    private func menu(_ control: RowControl) -> AnyView {
        let items = store.kids(index)
        guard let first = items.first else { return AnyView(EmptyView()) }
        return AnyView(SharedRow(store: store, index: first, axis: .none, focus: $focus)
            .contextMenu {
                ForEach(Array(items.dropFirst()), id: \.self) { child in
                    if let item = store.control(child) {
                        Button(item.words) { deliver(item) }
                    }
                }
            })
    }

    /// A sheet draws its children above its own dismiss press, so the view underneath stays
    /// visible; dismissing it delivers its action with no text.
    private func sheet(_ control: RowControl) -> AnyView {
        AnyView(VStack(alignment: .leading, spacing: SharedLook.usable(control.look.spacing)) {
            HStack {
                Spacer()
                Button { deliver(control) } label: { SharedIcons.symbol("dismiss") }
                    .buttonStyle(.plain)
            }
            kids(.vertical)
        })
    }

    private func deliver(_ control: RowControl) {
        store.send(Touch.control(control.action ?? -1, ""))
    }
}

extension View {
    /// One view as an `AnyView`.
    func erase() -> AnyView { AnyView(self) }
}
