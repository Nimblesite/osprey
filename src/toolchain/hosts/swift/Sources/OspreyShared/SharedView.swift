import SwiftUI
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW]: draws one leased view by walking the rows from
// the root and drawing each row's children where its kind puts them. A row is
// read in place, so nothing here copies or decodes it; the store keeps the lease
// alive for as long as this view can read it.

/// The direction a container lays its children out in, which decides the way a
/// child's flex share stretches.
public enum SharedAxis {
    case vertical, horizontal, none
}

/// The host's view of one leased frame.
public struct SharedView: View {
    @ObservedObject private var store: SharedStore
    @FocusState private var focus: Int?

    public init(store: SharedStore) {
        self.store = store
    }

    public var body: some View {
        if let root = store.row(0), root.parent == -1 {
            SharedRow(store: store, index: 0, axis: .vertical, focus: $focus)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else {
            Color.clear
        }
    }
}

/// One row and, recursively, the rows that name it as their parent.
struct SharedRow: View {
    let store: SharedStore
    let index: Int
    let axis: SharedAxis
    @FocusState.Binding var focus: Int?

    var body: some View {
        if let row = store.row(index) {
            SharedLook(row).dressed(drawn(row), axis: axis)
        }
    }

    /// The control this row is. An unknown kind is not a control, so it draws
    /// nothing rather than a guess.
    private func drawn(_ row: OspNativeView) -> AnyView {
        guard let kind = SharedKind(rawValue: row.kind) else { return AnyView(EmptyView()) }
        switch kind {
        case .column: return column(row)
        case .line: return line(row)
        case .gap: return gap()
        case .scroll: return scroll(row)
        case .layer: return layer(row)
        case .rule: return rule(row)
        case .canvas: return canvas(row)
        case .menu: return menu(row)
        case .sheet: return sheet(row)
        default: return leaf(row)
        }
    }

    /// The rows that name this one as their parent, drawn in order.
    func kids(_ axis: SharedAxis) -> some View {
        ForEach(store.kids(index), id: \.self) { child in
            SharedRow(store: store, index: child, axis: axis, focus: $focus)
        }
    }

    private func column(_ row: OspNativeView) -> AnyView {
        AnyView(VStack(alignment: .leading, spacing: OspNativeView.usable(row.spacing)) { kids(.vertical) })
    }

    private func line(_ row: OspNativeView) -> AnyView {
        AnyView(HStack(alignment: .center, spacing: OspNativeView.usable(row.spacing)) { kids(.horizontal) })
    }

    /// A gap takes the free space along its parent's axis, as a share of one.
    private func gap() -> AnyView {
        AnyView(Spacer(minLength: 0).layoutPriority(1))
    }

    private func scroll(_ row: OspNativeView) -> AnyView {
        AnyView(ScrollView(.vertical) {
            VStack(alignment: .leading, spacing: OspNativeView.usable(row.spacing)) { kids(.vertical) }
        })
    }

    /// A layer's children lie on each other, the last in front.
    private func layer(_ row: OspNativeView) -> AnyView {
        AnyView(ZStack(alignment: .topLeading) { kids(.none) })
    }
}
