import SwiftUI

// Implements [UI-HOST], [UI-COMPONENTS] and [DESIGN-FOUNDATIONS]: navigation
// and page structure. A split's first pane is the sidebar on the theme's inset
// colour; every other screen is a page on its canvas. A screen's toolbar joins
// the window toolbar and a toolbar field becomes the platform search field.

/// Page margins and widths from the design system's layout grammar.
enum Page {
    #if os(macOS)
    static let margin: CGFloat = 32
    #else
    static let margin: CGFloat = 20
    #endif
    static let width: CGFloat = 1240
    /// The least height a page's filling content keeps; with less room the whole page scrolls instead.
    static let fill: CGFloat = 280
}

struct ScreenView: View {
    let title: String
    let subtitle: String
    let toolbar: [Node]
    let children: [Node]
    let drawing: Drawing

    private var search: (value: String, prompt: String, commit: Commit, event: Ordinal)? {
        for node in toolbar { if case let .field(_, value, prompt, _, _, commit, event) = node { return (value, prompt, commit, event) } }
        return nil
    }

    private var tools: [Node] { toolbar.filter { if case .field = $0 { return false } else { return true } } }

    @Environment(\.ospreyTabSelected) private var selected

    // Tabs stay alive while hidden; only the selected tab's pages name the window and fill its toolbar.
    var body: some View {
        PageView(subtitle: subtitle, children: children, drawing: drawing)
            .modifier(TitleModifier(title: selected ? title : ""))
            .toolbar { if selected, !tools.isEmpty { ToolbarItemGroup { Children(children: tools, drawing: drawing).environment(\.ospreyPlacement, .toolbar) } } }
            .modifier(SearchModifier(search: selected ? search : nil, send: drawing.send))
    }
}

/// A page that holds a filling child fits the window and lets it fill while its other children leave the
/// filling content `Page.fill` points. Otherwise, and for any other page, the whole page scrolls, so no
/// content can make a page taller than its window ([UI-HOST]).
struct PageView: View {
    let subtitle: String
    let children: [Node]
    let drawing: Drawing
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyPlacement) private var placement

    private var dark: Bool { scheme == .dark }

    private var content: [Node] { subtitle.isEmpty ? children : [.text(subtitle, .lead, .secondary)] + children }

    var body: some View {
        if placement == .sidebar {
            sidebar
        } else {
            page.foregroundStyle(theme.tone(.primary, dark: dark)).background(theme.color("canvas", dark: dark))
        }
    }

    @ViewBuilder private var page: some View {
        if children.contains(where: \.fills) {
            ViewThatFits(in: .vertical) {
                FillingStack(children: content, spacing: theme.spacing(3), drawing: drawing)
                    .padding(Page.margin).frame(maxWidth: Page.width).frame(maxWidth: .infinity)
                scrolling
            }
        } else {
            scrolling
        }
    }

    private var scrolling: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: theme.spacing(4)) { Children(children: content, drawing: drawing) }
                .padding(Page.margin).frame(maxWidth: Page.width, alignment: .leading).frame(maxWidth: .infinity)
        }
    }

    // The brand, the navigation list filling the column, and a footer, on the inset colour.
    private var sidebar: some View {
        VStack(alignment: .leading, spacing: theme.spacing(2)) {
            ForEach(Array(children.enumerated()), id: \.offset) { _, child in
                if child.fills { NodeView(node: child, drawing: drawing) } else { NodeView(node: child, drawing: drawing).padding(.horizontal, theme.spacing(3)) }
            }
        }
        .padding(.vertical, theme.spacing(3))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(theme.color("inset", dark: dark))
    }
}

/// An untitled page leaves the window title to the column that names it.
struct TitleModifier: ViewModifier {
    let title: String

    func body(content: Content) -> some View {
        if title.isEmpty { content } else { content.navigationTitle(title) }
    }
}

/// The platform search field. Only the text being composed stays here.
struct SearchModifier: ViewModifier {
    let search: (value: String, prompt: String, commit: Commit, event: Ordinal)?
    let send: (Int, JSONValue) -> Void
    @State private var draft = ""

    func body(content: Content) -> some View {
        if let search {
            content.searchable(text: $draft, prompt: search.prompt)
                .onAppear { draft = search.value }
                .onChange(of: search.value) { _, next in if next != draft { draft = next } }
                .onChange(of: draft) { _, next in if search.commit.edited(next, from: search.value) { send(search.event, .string(next)) } }
                .onSubmit(of: .search) { if search.commit.submitted(draft, from: search.value) { send(search.event, .string(draft)) } }
        } else {
            content
        }
    }
}

/// Navigation columns: the first is the sidebar.
struct SplitView: View {
    let panes: [Node]
    let drawing: Drawing

    var body: some View {
        #if os(macOS)
        switch panes.count {
        case 0: EmptyView()
        case 1: pane(0)
        case 2: NavigationSplitView { sidebar } detail: { pane(1) }
        default: NavigationSplitView { sidebar } content: { pane(1).navigationSplitViewColumnWidth(min: 300, ideal: 430) } detail: { rest }
        }
        #else
        VStack(spacing: 0) { Children(children: panes, drawing: drawing) }
        #endif
    }

    private func pane(_ index: Int) -> some View { NodeView(node: panes[index], drawing: drawing) }

    private var sidebar: some View { pane(0).environment(\.ospreyPlacement, .sidebar).navigationSplitViewColumnWidth(min: 210, ideal: 230, max: 270) }

    private var rest: some View { VStack(spacing: 0) { Children(children: Array(panes.dropFirst(2)), drawing: drawing) } }
}

/// A pane with a column beside it that a splitter resizes ([UI-COMPONENTS-SPLITTER]); on a phone the column
/// follows the pane. Only a drag in progress lives here: its end sends the width, and Osprey's answer replaces it.
struct AsideView: View {
    let pane: Node
    let side: Side
    let aside: Node
    let splitter: Splitter
    let drawing: Drawing
    @State private var dragging: CGFloat?
    @State private var total: CGFloat = 0
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyPlacement) private var placement

    var body: some View {
        #if os(macOS)
        HStack(alignment: .top, spacing: 0) {
            if side == .leading { asideColumn }
            column(pane)
            if side == .trailing { asideColumn }
        }
        .overlay { divider }
        .onGeometryChange(for: CGFloat.self, of: \.size.width) { total = $0 }
        .onChange(of: splitter.width) { dragging = nil }
        #else
        VStack(alignment: .leading, spacing: theme.spacing(3)) { column(pane); column(aside) }
        #endif
    }

    #if os(macOS)
    private var asideColumn: some View {
        column(aside).padding(side == .trailing ? .leading : .trailing, 1)
            .frame(width: dragging ?? CGFloat(splitter.width))
    }

    // The dividing line follows the full row height, including a taller document.
    private var divider: some View {
        GeometryReader { geometry in
            let width = dragging ?? CGFloat(splitter.width)
            let edge = side == .leading ? width : geometry.size.width - width
            SplitterHandle(splitter: splitter, side: side, total: total, dragging: $dragging, commit: commit)
                .frame(height: geometry.size.height).position(x: edge + 0.5, y: geometry.size.height / 2)
        }
    }

    // An unchanged width draws nothing new, so the preview ends here instead of when Osprey's answer arrives.
    private func commit(_ width: CGFloat) {
        let points = Int(width.rounded())
        if points == splitter.width { dragging = nil } else { drawing.send(splitter.event, .number(Double(points))) }
    }
    #endif

    @ViewBuilder private func column(_ node: Node) -> some View {
        if placement == .pane {
            PaneView(node: node, drawing: drawing).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        } else {
            // On a scrolling page each column keeps its natural height; the row would stretch the shorter one.
            NodeView(node: node, drawing: drawing).fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity, alignment: .topLeading)
        }
    }
}

private struct TabSelectedKey: EnvironmentKey {
    static let defaultValue = true
}

extension EnvironmentValues {
    /// False inside a tab that is not selected; a page outside tabs is always selected.
    var ospreyTabSelected: Bool {
        get { self[TabSelectedKey.self] }
        set { self[TabSelectedKey.self] = newValue }
    }
}

struct TabsView: View {
    let entries: [TabEntry]
    let selected: String
    let drawing: Drawing
    let choose: (String) -> Void

    var body: some View {
        TabView(selection: Binding.sending(selected) { choose($0) }) {
            ForEach(entries, id: \.key) { entry in
                NodeView(node: entry.child, drawing: drawing).environment(\.ospreyTabSelected, entry.key == selected).tabItem { Label(entry.label, systemImage: entry.symbol) }.tag(entry.key)
            }
        }
    }
}

/// [DESIGN-COMPONENTS] surface: an eyebrow, the content and an explanatory footer on a bordered surface.
struct SectionView: View {
    let title: String
    let footer: String
    let children: [Node]
    let drawing: Drawing
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(alignment: .leading, spacing: theme.spacing(3)) {
            if !title.isEmpty { TextView(value: title, role: .eyebrow, tone: .secondary) }
            Children(children: children, drawing: drawing)
            if !footer.isEmpty { TextView(value: footer, role: .callout, tone: .secondary) }
        }
        .padding(theme.spacing(3))
        .frame(maxWidth: .infinity, alignment: .leading)
        .surface(theme.color("surface", dark: scheme == .dark), border: theme.color("line", dark: scheme == .dark), radius: 16)
    }
}

extension View {
    /// An opaque rounded fill with a thin border ([DESIGN-FOUNDATIONS]: borders, not shadows, group content).
    func surface(_ fill: Color, border: Color, radius: CGFloat, width: CGFloat = 0.5) -> some View {
        background(fill, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(border, lineWidth: width))
    }
}

struct DisclosureView: View {
    let title: String
    let expanded: Bool
    let children: [Node]
    let drawing: Drawing
    let toggle: (Bool) -> Void

    var body: some View {
        DisclosureGroup(title, isExpanded: Binding.sending(expanded) { toggle($0) }) {
            VStack(alignment: .leading, spacing: 4) { Children(children: children, drawing: drawing) }.frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.caption)
    }
}
