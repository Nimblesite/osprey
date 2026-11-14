import SwiftUI

// Implements [NATIVE-HOSTS-CONTRACT] and [UI-COMPONENTS]: each node kind has exactly one
// SwiftUI mapping. A kind without one is a visible refusal and a diagnostic,
// never a fallback ([NATIVE-HOSTS-CONTRACT]). Events carry only the region, the
// revision it was drawn with, the control's ordinal and a bounded value.

/// The region a node tree was drawn in; every control sends through it.
public struct Drawing {
    let region: String
    let revision: Int
    let send: (Int, Tree) -> Void
    let point: (Int, Tree) -> Void
    let act: (Tree, Tree) -> Void
}

extension Tree {
    public subscript(_ key: String) -> Tree? { object?[key] }
    public func text(_ key: String) -> String { self[key]?.string ?? "" }
    public func flag(_ key: String) -> Bool { self[key]?.bool ?? false }
    public func count(_ key: String) -> Int { self[key]?.int ?? 0 }
    public func list(_ key: String) -> [Tree] { self[key]?.array ?? [] }
}

public struct NodeView: View {
    let node: Tree
    let drawing: Drawing

    public var body: some View {
        switch node.text("kind") {
        case "scene": ToldScene(node: node)
        case "text", "rich-text", "label", "icon", "badge", "pair", "evidence", "link", "spacer", "divider", "progress", "meter", "empty", "segments", "swatch": LeafView(node: node)
        case "button", "field", "toggle", "picker", "disclosure", "alert": ControlView(node: node, drawing: drawing)
        case "rows": RowsView(node: node, drawing: drawing)
        case "math": MathLeaf(node: node)
        case "image": StillLeaf(node: node)
        case "video": FilmLeaf(node: node)
        case "editor": EditorLeaf(node: node, drawing: drawing)
        case "board": BoardLeaf(node: node)
        case "stack", "framed", "themed", "slot", "layer", "section", "screen", "split", "aside", "tabs", "dialog", "context-menu": ContainerView(node: node, drawing: drawing)
        default: Refusal(kind: node.text("kind"))
        }
    }
}

/// [NATIVE-HOSTS-CONTRACT] A kind this host does not draw says so where it would have been.
struct Refusal: View {
    let kind: String
    var body: some View {
        Text("This host does not draw \(kind) nodes").font(.caption).foregroundStyle(.red).padding(6).overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(.red))
    }
}

struct Children: View {
    let nodes: [Tree]
    let drawing: Drawing

    var body: some View {
        ForEach(Array(nodes.enumerated()), id: \.offset) { _, child in NodeView(node: child, drawing: drawing) }
    }
}

struct ContainerView: View {
    let node: Tree
    let drawing: Drawing
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        switch node.text("kind") {
        case "stack": stack
        case "framed": FramedView(node: node, drawing: drawing)
        case "themed": themed
        case "slot": RegionView(key: node.text("key"))
        case "layer": ZStack(alignment: .topLeading) { Children(nodes: node.list("children"), drawing: drawing) }
        case "section": SectionView(node: node, drawing: drawing)
        case "screen": ScreenView(node: node, drawing: drawing)
        case "split": SplitView(node: node, drawing: drawing)
        case "aside": AsideView(node: node, drawing: drawing)
        case "tabs": TabsView(node: node, drawing: drawing)
        case "dialog": DialogView(node: node, drawing: drawing)
        default: contextMenu
        }
    }

    @ViewBuilder private var stack: some View {
        let gap = inherited.gap ?? theme.spacing(1)
        if node.text("axis") == "horizontal" {
            HStack(alignment: inherited.rowsFromTop ? .top : .center, spacing: gap) { Children(nodes: node.list("children"), drawing: drawing).environment(\.ospreyInRow, true) }
        } else {
            // The frame's authored cross alignment places the children, as the browser does
            // (`align-items`): start, center or end.
            let cross: HorizontalAlignment = inherited.cross == "center" ? .center : inherited.cross == "end" ? .trailing : .leading
            VStack(alignment: cross, spacing: gap) { Children(nodes: node.list("children"), drawing: drawing).environment(\.ospreyInRow, false) }
        }
    }

    @ViewBuilder private var themed: some View {
        let resolver = ThemeResolver(theme: decodedTheme)
        NodeView(node: node["body"] ?? .null, drawing: drawing)
            .environment(\.ospreyTheme, resolver)
            .foregroundStyle(resolver.color("text", dark: scheme == .dark) ?? .primary)
            .tint(resolver.color("accent", dark: scheme == .dark) ?? .accentColor)
            .padding(resolver.spacing(1) * 3)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(resolver.color("background", dark: scheme == .dark) ?? .clear)
    }

    private var decodedTheme: Theme? {
        Theme(node["theme"]) ?? theme.theme
    }

    private var contextMenu: some View {
        NodeView(node: node["body"] ?? .null, drawing: drawing).contextMenu { Children(nodes: node.list("items"), drawing: drawing) }
    }
}

/// [UI-COMPONENTS-FRAMED] Application-authored insets, dimensions and surfaces, drawn mechanically.
struct FramedView: View {
    let node: Tree
    let drawing: Drawing
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyInRow) private var inRow
    #if os(macOS)
    @Environment(\.ospreySelectionGroup) private var outerGroup
    @State private var selection = SelectionGroup()
    #endif

    private var style: Tree { node["style"] ?? .null }
    private var dark: Bool { scheme == .dark }
    private var radius: CGFloat { CGFloat(style.count("radius")) }
    private var insets: EdgeInsets {
        EdgeInsets(top: CGFloat(style.count("top")) + lined("top"), leading: CGFloat(style.count("left")) + lined("left"),
                   bottom: CGFloat(style.count("bottom")) + lined("bottom"), trailing: CGFloat(style.count("right")) + lined("right"))
    }

    /// With exact metrics a border occupies its point of layout on each edge it draws, as the browser's does.
    private func lined(_ edge: String) -> CGFloat {
        let drawn = style.text("lineEdge")
        return FontRegistration.exactLines && !style.text("line").isEmpty && (drawn.isEmpty || drawn == "all" || drawn == edge) ? 1 : 0
    }
    private var width: CGFloat? { style.count("width") > 0 ? CGFloat(style.count("width")) : nil }
    private var height: CGFloat? { style.count("height") > 0 ? CGFloat(style.count("height")) : nil }
    private var grows: Bool { style.flag("grow") }
    private var hugs: Bool { style.flag("hug") }

    /// How the body sits on the cross and main axes of the frame.
    private var alignment: Alignment {
        let horizontal: HorizontalAlignment = style.text("cross") == "center" ? .center : style.text("cross") == "end" ? .trailing : .leading
        let vertical: VerticalAlignment = style.text("align") == "center" ? .center : style.text("align") == "end" ? .bottom : .top
        return Alignment(horizontal: horizontal, vertical: vertical)
    }

    private var content: some View {
        let leaf = NodeView(node: node["body"] ?? .null, drawing: drawing)
            .environment(\.ospreyInherited, inherited.merging(style.object ?? [:]))
            .environment(\.ospreyInRow, false)
        #if os(macOS)
        return leaf.environment(\.ospreySelectionGroup, style.flag("selectionGroup") ? selection : outerGroup)
        #else
        return leaf
        #endif
    }

    var body: some View {
        let dressed = sized
            .background { surface }
            .overlay { outline }
            .modifier(Lifted(on: style.text("shadow") == "floating", radius: radius))
            .foregroundStyle(ink)
        #if os(macOS)
        return dressed.modifier(FrameInk(colour: inkColour))
        #else
        return dressed
        #endif
    }

    #if os(macOS)
    /// The frame's ink as AppKit ink, for the leaves a selection group draws.
    private var inkColour: NSColor {
        token("ink").map(NSColor.init) ?? theme.color("text", dark: dark).map(NSColor.init) ?? .labelColor
    }
    #endif

    private func revealing(_ proxy: ScrollViewProxy) -> (@MainActor (UUID) -> Void)? {
        guard style.text("align") == "end" else { return nil }
        return { id in proxy.scrollTo(id, anchor: .top) }
    }

    @ViewBuilder private var inner: some View {
        if style.flag("scrollY") {
            // A column that lines up from its end follows new content, as a conversation does, so what
            // opens inside it would grow upward past its own title; it asks to be shown instead.
            ScrollViewReader { proxy in
                ScrollView {
                    content.padding(insets).frame(maxWidth: .infinity, alignment: alignment)
                        .environment(\.ospreyReveal, revealing(proxy))
                }
                .defaultScrollAnchor(style.text("align") == "end" ? .bottom : .top, for: .sizeChanges)
                .defaultScrollAnchor(style.text("align") == "end" ? .bottom : .top, for: .initialOffset)
                .scrollIndicators(.automatic)
            }
        } else if FontRegistration.exactLines, grows, !(node["body"] ?? .null).fillsPane {
            // A growing frame keeps natural content at its size and scrolls content taller than its pane.
            ViewThatFits(in: .vertical) {
                content.padding(insets).fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: alignment)
                ScrollView(.vertical) { content.padding(insets).frame(maxWidth: .infinity, alignment: alignment) }
            }
        } else {
            content.padding(insets)
        }
    }

    @ViewBuilder private var sized: some View {
        let fillsWidth = width == nil && !hugs && (!inRow || grows)
        inner
            .frame(minWidth: inRow ? width : nil, idealWidth: width, maxWidth: fillsWidth ? .infinity : width, alignment: alignment)
            .frame(minHeight: height, idealHeight: height, maxHeight: grows && !inRow ? .infinity : height, alignment: alignment)
    }

    @ViewBuilder private var surface: some View {
        if let fill = token("fill") { RoundedRectangle(cornerRadius: radius, style: .continuous).fill(fill) }
    }

    @ViewBuilder private var outline: some View {
        if let line = token("line") {
            switch style.text("lineEdge") {
            case "top": VStack { line.frame(height: 1); Spacer(minLength: 0) }.allowsHitTesting(false)
            case "bottom": VStack { Spacer(minLength: 0); line.frame(height: 1) }.allowsHitTesting(false)
            case "left": HStack { line.frame(width: 1); Spacer(minLength: 0) }.allowsHitTesting(false)
            case "right": HStack { Spacer(minLength: 0); line.frame(width: 1) }.allowsHitTesting(false)
            default: RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(line, lineWidth: 1).allowsHitTesting(false)
            }
        }
    }

    private var ink: AnyShapeStyle {
        if let colour = token("ink") { return AnyShapeStyle(colour) }
        return theme.foreground(dark: dark)
    }

    private func token(_ key: String) -> Color? {
        let name = style.text(key)
        return name.isEmpty ? nil : theme.color(name, dark: dark)
    }
}

/// The floating shadow a frame asks for, cast by its surface alone.
struct Lifted: ViewModifier {
    let on: Bool
    let radius: CGFloat

    func body(content: Content) -> some View {
        if on { content.compositingGroup().shadow(color: .black.opacity(0.28), radius: 18, y: 8) } else { content }
    }
}

/// A slot shows another region's cell, so only that cell re-renders when its revision moves.
struct RegionView: View {
    let key: String
    @EnvironmentObject private var store: RegionStore

    var body: some View {
        if let cell = store.cell(key) { CellView(cell: cell, store: store) } else { Refusal(kind: "missing region \(key)") }
    }
}

struct CellView: View {
    @ObservedObject var cell: RegionCell
    let store: RegionStore

    var body: some View {
        NodeView(node: cell.node, drawing: drawing)
    }

    private var drawing: Drawing {
        let region = cell.key, revision = cell.revision
        return Drawing(region: region, revision: revision,
                       send: { ordinal, value in store.send(region: region, revision: revision, ordinal: ordinal, value: value) },
                       point: { ordinal, value in store.send(region: region, revision: cell.revision, ordinal: ordinal, value: value) },
                       act: { node, value in
                           if let current = cell.controlRevision(for: node, value: value) {
                               store.send(region: region, revision: current, ordinal: node.count("event"), value: value)
                           }
                       })
    }
}

public struct RootView: View {
    @ObservedObject var store: RegionStore
    @Environment(\.colorScheme) private var scheme

    public init(store: RegionStore) {
        self.store = store
    }

    public var body: some View {
        VStack(spacing: 0) {
            if let failure = store.failure {
                ContentUnavailableView("The application could not start", systemImage: "exclamationmark.triangle", description: Text(failure))
            } else if let root = store.root {
                CellView(cell: root, store: store)
            } else if !store.told {
                SharedView(store: store.shared)
            } else {
                ProgressView()
            }
            if let last = store.diagnostics.last, store.failure == nil {
                Text(last).font(.caption).foregroundStyle(.secondary).padding(6).frame(maxWidth: .infinity, alignment: .leading).background(.bar)
            }
        }
        .tint(ThemeResolver(theme: store.theme).color("accent", dark: scheme == .dark))
        .environmentObject(store)
        .environment(\.ospreyTheme, ThemeResolver(theme: store.theme))
        .task { store.start(dark: scheme == .dark) }
        .onChange(of: scheme) { _, now in store.shared.appear(dark: now == .dark) }
    }
}

/// [NATIVE-SHM-SCENE] A scene component: the canvas the frame's scene rows name, with its input.
struct ToldScene: View {
    let node: Tree
    @EnvironmentObject private var store: RegionStore

    var body: some View {
        SharedCanvas(store: store.shared, index: node.count("sceneRow"), ordinal: Int64(node.count("event")))
    }
}

extension Tree {
    /// Whether this content fills the pane it is given rather than keeping a natural height: an
    /// editor, board or other pane-sized component, or a column holding one.
    var fillsPane: Bool {
        switch text("kind") {
        case "editor", "board", "dialog", "split", "aside", "tabs", "screen", "scene": return true
        case "stack": return list("children").contains { $0.fillsPane }
        case "layer": return list("children").contains { $0.fillsPane }
        case "themed": return (self["body"] ?? .null).fillsPane
        case "framed": return (self["style"] ?? .null).flag("grow") && (self["style"] ?? .null).count("height") == 0
        default: return false
        }
    }
}
