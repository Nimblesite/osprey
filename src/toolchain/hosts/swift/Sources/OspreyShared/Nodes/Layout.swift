import SwiftUI

// Implements [NATIVE-HOSTS-CONTRACT] and [UI-COMPONENTS-SPLITTER]: navigation and page
// structure. A split's first pane is the system's vibrant sidebar.

/// Navigation columns: the first pane stands on the system's sidebar material, under the window's title
/// bar, and the rest fill the window. It is laid out directly, so every pixel comes from the application's frames.
struct SplitView: View {
    let node: Tree
    let drawing: Drawing

    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    private var panes: [Tree] { node.list("children") }

    /// The theme's `sidebar` token over the material, so the window behind shows through only a little.
    @ViewBuilder private var tint: some View {
        if let colour = theme.color("sidebar", dark: scheme == .dark) { colour.opacity(0.9) }
    }

    /// The sidebar's width in points.
    private static let sidebar: CGFloat = 264

    var body: some View {
        switch panes.count {
        case 0: EmptyView()
        case 1: NodeView(node: panes[0], drawing: drawing)
        default:
            HStack(spacing: 0) {
                NodeView(node: panes[0], drawing: drawing)
                    .environment(\.ospreyPlacement, .sidebar)
                    .frame(width: Self.sidebar)
                    .frame(maxHeight: .infinity)
                    .background { ZStack { SidebarMaterial(); tint } }
                VStack(spacing: 0) { Children(nodes: Array(panes.dropFirst()), drawing: drawing) }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .ignoresSafeArea(.container, edges: .top)
        }
    }
}

/// The system's vibrant sidebar material, blending with what is behind the window.
#if os(macOS)
struct SidebarMaterial: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .underWindowBackground
        view.blendingMode = .behindWindow
        view.state = .followsWindowActiveState
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {}
}

#else
struct SidebarMaterial: View {
    var body: some View { Rectangle().fill(.regularMaterial) }
}
#endif

/// A pane with a column beside it that a splitter resizes; only a drag in progress lives here.
struct AsideView: View {
    let node: Tree
    let drawing: Drawing
    @State private var dragging: CGFloat?
    @State private var start: CGFloat = 0

    private var width: CGFloat { dragging ?? CGFloat(node.count("width")) }
    private var trailing: Bool { node.text("side") == "trailing" }

    var body: some View {
        HStack(spacing: 0) {
            if !trailing { column; handle }
            NodeView(node: node["pane"] ?? .null, drawing: drawing).frame(maxWidth: .infinity, maxHeight: .infinity)
            if trailing { handle; column }
        }
    }

    private var column: some View {
        NodeView(node: node["aside"] ?? .null, drawing: drawing).frame(width: width).frame(maxHeight: .infinity)
    }

    private var handle: some View {
        Rectangle().fill(.clear).frame(width: 6)
            .padding(.horizontal, -3)
            .contentShape(Rectangle())
            #if os(macOS)
            .onHover { inside in if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() } }
            #endif
            .gesture(DragGesture(coordinateSpace: .global).onChanged { drag in
                if dragging == nil { start = CGFloat(node.count("width")) }
                let moved = trailing ? -drag.translation.width : drag.translation.width
                dragging = min(max(start + moved, CGFloat(node.count("least"))), CGFloat(node.count("most")))
            }.onEnded { _ in
                if let dragging { drawing.send(node.count("event"), .number(Double(Int(dragging.rounded())))) }
                dragging = nil
            })
            .zIndex(1)
    }
}

struct ScreenView: View {
    let node: Tree
    let drawing: Drawing

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    if !node.text("title").isEmpty { Text(node.text("title")).font(.title2.weight(.semibold)) }
                    if !node.text("subtitle").isEmpty { Text(node.text("subtitle")).foregroundStyle(.secondary) }
                }
                Spacer(minLength: 0)
                HStack(spacing: 6) { Children(nodes: node.list("toolbar"), drawing: drawing) }
            }
            Children(nodes: node.list("children"), drawing: drawing)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct SectionView: View {
    let node: Tree
    let drawing: Drawing

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !node.text("title").isEmpty { Text(node.text("title")).font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary) }
            Children(nodes: node.list("children"), drawing: drawing)
            if !node.text("footer").isEmpty { Text(node.text("footer")).font(.caption).foregroundStyle(.secondary) }
        }
    }
}

struct TabsView: View {
    let node: Tree
    let drawing: Drawing

    var body: some View {
        TabView(selection: Binding(get: { node.text("selected") }, set: { drawing.send(node.count("event"), .string($0)) })) {
            ForEach(node.list("tabs"), id: \.keyOfRow) { tab in
                NodeView(node: tab["child"] ?? .null, drawing: drawing)
                    .tabItem { Label(tab.text("label"), systemImage: Symbols.name(tab.text("symbol"))) }
                    .tag(tab.text("key"))
            }
        }
    }
}

struct DialogView: View {
    let node: Tree
    let drawing: Drawing

    var body: some View {
        Color.clear.frame(width: 0, height: 0)
            .sheet(isPresented: Binding(get: { true }, set: { if !$0 { drawing.send(node.count("event"), .null) } })) {
                NodeView(node: node["body"] ?? .null, drawing: drawing)
                    .accessibilityLabel(node.text("title"))
            }
    }
}
