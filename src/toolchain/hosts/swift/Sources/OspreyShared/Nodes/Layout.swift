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
            .underTitleBar()
        }
    }
}

private extension View {
    /// A split runs under the Mac's hidden title bar; on iOS it stays inside the safe area, clear of the status bar.
    @ViewBuilder func underTitleBar() -> some View {
        #if os(macOS)
        ignoresSafeArea(.container, edges: .top)
        #else
        self
        #endif
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
/// Osprey owns the width and its limits; while a drag is in progress the host previews the width
/// it would send, and the drag's end delivers that width to the splitter's binding.
struct AsideView: View {
    let node: Tree
    let drawing: Drawing
    @State private var dragging: CGFloat?

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

    /// The width once the splitter moves `offset` points from `start`, within the limits Osprey sent.
    private func preview(from start: CGFloat, dragged offset: CGFloat) -> CGFloat {
        let wanted = trailing ? start - offset : start + offset
        return min(max(wanted, CGFloat(node.count("least"))), CGFloat(node.count("most")))
    }

    private var handle: some View {
        SplitterHandle(width: node.count("width"), trailing: trailing, preview: preview,
                       dragging: $dragging,
                       sent: { next in drawing.send(node.count("event"), .number(Double(Int(next.rounded())))) })
            .zIndex(1)
    }
}

/// The draggable splitter: a line on the theme's `divider` colour, the accent while hovered or
/// dragged, as wide as the gap between the columns for its hit area.
struct SplitterHandle: View {
    let width: Int
    let trailing: Bool
    let preview: (CGFloat, CGFloat) -> CGFloat
    @Binding var dragging: CGFloat?
    let sent: (CGFloat) -> Void
    @State private var start: CGFloat?
    @State private var hovering = false
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    private var active: Bool { hovering || start != nil }

    /// The splitter's line: the accent while hovered or dragged, else the theme's divider. No
    /// divider token draws no line, so a theme without one keeps the seam invisible.
    private var ink: Color? {
        if active { return theme.color("accent", dark: scheme == .dark) ?? .accentColor }
        return theme.color("divider", dark: scheme == .dark)
    }

    var body: some View {
        // The clear strip is net zero wide: the pane and the column keep the widths Osprey sent.
        Rectangle().fill(.clear).frame(width: 6)
            .padding(.horizontal, -3)
            .overlay { if let ink { Rectangle().fill(ink).frame(width: active ? 2 : 1) } }
            .contentShape(Rectangle().inset(by: -4))
            #if os(macOS)
            .onContinuousHover { phase in
                hovering = phase != .ended
                (hovering ? NSCursor.resizeLeftRight : NSCursor.arrow).set()
            }
            #endif
            .gesture(drag)
            .accessibilityElement()
            .accessibilityLabel("Column splitter")
            .accessibilityValue("\(dragging.map { Int($0) } ?? width) points")
            .accessibilityAdjustableAction(adjust)
    }

    // Translation is global so the splitter's own movement does not feed back into the drag.
    private var drag: some Gesture {
        DragGesture(minimumDistance: 1, coordinateSpace: .global)
            .onChanged { change in
                let base = start ?? CGFloat(width)
                start = base
                dragging = preview(base, change.translation.width)
            }
            .onEnded { _ in
                start = nil
                if let dragging { sent(dragging) }
                dragging = nil
            }
    }

    /// Points a keyboard or assistive adjustment moves the splitter.
    private func adjust(_ direction: AccessibilityAdjustmentDirection) {
        let step: CGFloat = 20
        let offset = direction == .increment ? step : -step
        sent(preview(CGFloat(width), trailing ? -offset : offset))
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
    #if os(macOS)
    @State private var boundary = ModalFocusBoundary()
    @State private var escapeMonitor: Any?
    #endif

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.clear.contentShape(Rectangle()).onTapGesture(perform: dismiss)
            content
        }
        .accessibilityLabel(node.text("title"))
        #if os(macOS)
        .onAppear(perform: appear)
        .onDisappear(perform: leave)
        .onExitCommand(perform: dismiss)
        #endif
    }

    /// A dialog shows over its region, as every host draws it ([NATIVE-HOSTS-CONTRACT]): Escape
    /// delivers its dismissal exactly once, and while it shows only its own controls take the
    /// keyboard ([UI-COMPONENTS-EDITOR]) — the boundary restores the responder it replaced.
    #if os(macOS)
    private var content: some View {
        NodeView(node: node["body"] ?? .null, drawing: drawing)
            .environment(\.ospreyModalFocusBoundary, boundary)
            .background(DialogWindow { window in if let window { boundary.attach(to: window) } })
    }

    private func appear() {
        escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard event.keyCode == 53 else { return event }
            dismiss()
            return nil
        }
    }

    private func leave() {
        boundary.detach()
        if let escapeMonitor { NSEvent.removeMonitor(escapeMonitor) }
        escapeMonitor = nil
    }
    #else
    private var content: some View {
        NodeView(node: node["body"] ?? .null, drawing: drawing)
    }
    #endif

    private func dismiss() {
        drawing.send(node.count("event"), .null)
    }
}

#if os(macOS)
/// The window a dialog is drawn in, so its focus boundary guards exactly that window.
private struct DialogWindow: NSViewRepresentable {
    let found: (NSWindow?) -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { self.found(view.window) }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async { self.found(view.window) }
    }
}
#endif
