import SwiftUI

// Implements [UI-HOST] and [UI-COMPONENTS]: every node kind maps to a native
// control. The switch is exhaustive, so a new framework component cannot be
// drawn by accident as something else. Events carry only the region, the
// revision it was drawn with, the control's ordinal and a bounded value.

/// The region a node tree was drawn in; every control sends through it.
public struct Drawing {
    public let region: String
    public let revision: Int
    public let send: (Int, JSONValue) -> Void

    public init(region: String, revision: Int, send: @escaping (Int, JSONValue) -> Void) {
        self.region = region
        self.revision = revision
        self.send = send
    }
}

/// Where a node is drawn; native platforms present the same component differently in each.
/// A page lays out at natural height, a pane is a bounded column its node fills or scrolls within,
/// the sidebar is the navigation column and a row is a list row or card.
public enum Placement: Sendable {
    case page, pane, sidebar, row, toolbar
}

private struct PlacementKey: EnvironmentKey {
    static let defaultValue = Placement.page
}

extension EnvironmentValues {
    public var ospreyPlacement: Placement {
        get { self[PlacementKey.self] }
        set { self[PlacementKey.self] = newValue }
    }
}

extension Binding where Value: Equatable {
    /// A binding that sends only when the value actually changes. AppKit sets a source list's selection
    /// while SwiftUI is still updating views; sending then would publish inside that update, so the send
    /// waits for the main queue's next turn.
    static func sending(_ value: Value, _ send: @escaping (Value) -> Void) -> Binding<Value> {
        Binding(get: { value }, set: { next in if next != value { DispatchQueue.main.async { send(next) } } })
    }
}

extension Node {
    /// Lists, empty states, regions and asides take the remaining height of a page that fits its window.
    var fills: Bool {
        switch self {
        case .rows, .empty, .slot, .aside, .board, .editor: return true
        case let .layer(children): return children.contains(where: \.fillsPane)
        case let .themed(_, content): return content.fillsPane
        case .dialog: return true
        case let .framed(style, _): return style.grow && style.height == 0
        default: return false
        }
    }

    /// In a bounded column a stack holding a filling node fills too: its other children keep their natural height.
    var fillsPane: Bool {
        if case let .stack(.vertical, children) = self { return children.contains(where: \.fills) }
        return fills
    }

    var passive: Bool {
        switch self {
        case .text, .richText, .math, .image, .spacer, .divider, .badge, .label, .icon: return true
        case let .stack(_, children), let .layer(children): return children.allSatisfy(\.passive)
        case let .framed(_, content): return content.passive
        default: return false
        }
    }
}

public struct NodeView: View {
    public let node: Node
    public let drawing: Drawing
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyTypography) private var typography
    @Environment(\.ospreyInk) private var ink

    public init(node: Node, drawing: Drawing) {
        self.node = node
        self.drawing = drawing
    }

    private var dark: Bool { scheme == .dark }

    // Exhaustive over every kind: a new framework component cannot be drawn by accident as something else.
    public var body: some View {
        switch node {
        case .text, .richText, .evidence, .badge, .label, .icon, .link, .spacer, .divider, .segments, .empty, .progress, .swatch, .slot, .brand: content
        case .button, .field, .toggle, .picker, .alert, .disclosure: control
        case .stack, .layer, .framed, .themed, .dialog, .aside, .section, .screen, .split, .tabs, .rows: container
        case .editor, .board, .math, .image, .video, .contextMenu: RichView(node: node, drawing: drawing)
        }
    }

    // Each group's remaining kinds are unreachable: `body` routed them elsewhere.
    @ViewBuilder private var content: some View {
        switch node {
        case let .richText(runs, role): RunsView(runs: runs, role: role)
        case let .text(value, role, tone): TextView(value: value, role: role, tone: tone)
        case let .evidence(label, value): EvidenceLine(label: label, value: value)
        case let .badge(label, symbol, tone): BadgeView(label: label, symbol: symbol, color: theme.tone(tone, dark: dark))
        case let .label(label, symbol): Label(label, systemImage: symbol)
        case let .icon(symbol): NativeSymbol(name: symbol, size: typography.size > 0 ? CGFloat(typography.size) : 32).foregroundStyle(theme.color(ink.isEmpty ? "primary" : ink, dark: dark)).accessibilityHidden(true)
        case let .link(label, url): LinkView(label: label, url: url)
        case .spacer: Spacer(minLength: 0)
        case .divider: theme.color("divider", dark: dark).frame(height: 1).accessibilityHidden(true)
        case let .segments(tones, label): SegmentsView(colors: tones.map { theme.tone($0, dark: dark) }, label: label)
        case let .empty(symbol, title, detail): ContentUnavailableView(title, systemImage: symbol, description: Text(detail)).frame(maxWidth: .infinity, maxHeight: .infinity)
        case let .progress(label, detail): ProgressLine(label: label, detail: detail)
        case let .swatch(name, light, darkHex): SwatchView(name: name, light: light, dark: darkHex)
        case let .slot(key): RegionView(key: key)
        case .brand: BrandView()
        default: EmptyView()
        }
    }

    @ViewBuilder private var control: some View {
        switch node {
        case let .button(label, symbol, role, enabled, event): ButtonView(label: label, symbol: symbol, role: role, enabled: enabled) { drawing.send(event, .null) }
        case let .field(label, value, prompt, secure, multiline, commit, event): FieldView(label: label, value: value, prompt: prompt, secure: secure, multiline: multiline, commit: commit) { drawing.send(event, .string($0)) }
        case let .toggle(label, on, event): Toggle(label, isOn: Binding.sending(on) { drawing.send(event, .bool($0)) })
        case let .picker(label, choices, selected, event): PickerView(label: label, choices: choices, selected: selected) { drawing.send(event, .string($0)) }
        case let .alert(title, message, visible, event): AlertView(title: title, message: message, visible: visible) { drawing.send(event, .null) }
        case let .disclosure(title, expanded, event, children): DisclosureView(title: title, expanded: expanded, children: children, drawing: drawing) { drawing.send(event, .bool($0)) }
        default: EmptyView()
        }
    }

    @ViewBuilder private var container: some View {
        switch node {
        case let .stack(axis, children): StackView(axis: axis, children: children, drawing: drawing)
        case let .layer(children): LayerView(children: children, drawing: drawing)
        case let .themed(theme, content): NodeView(node: content, drawing: drawing).environment(\.ospreyTheme, ThemeResolver(theme: theme))
        case let .dialog(title, content, event): DialogView(title: title, content: content, drawing: drawing, dismiss: { drawing.send(event, .null) })
        case let .framed(style, content): FramedView(style: style, content: content, drawing: drawing)
        case let .aside(pane, side, aside, splitter): AsideView(pane: pane, side: side, aside: aside, splitter: splitter, drawing: drawing)
        case let .section(title, footer, children): SectionView(title: title, footer: footer, children: children, drawing: drawing)
        case let .screen(title, subtitle, toolbar, children): ScreenView(title: title, subtitle: subtitle, toolbar: toolbar, children: children, drawing: drawing)
        case let .split(panes): SplitView(panes: panes, drawing: drawing)
        case let .tabs(entries, selected, event): TabsView(entries: entries, selected: selected, drawing: drawing) { drawing.send(event, .string($0)) }
        case let .rows(entries, selected, event): RowsView(entries: entries, selected: selected, drawing: drawing) { drawing.send(event, .string($0)) }
        default: EmptyView()
        }
    }
}

struct Children: View {
    let children: [Node]
    let drawing: Drawing

    var body: some View {
        ForEach(Array(children.enumerated()), id: \.offset) { _, child in NodeView(node: child, drawing: drawing) }
    }
}

/// Children at their natural height, except those that fill: they share the rest of a bounded column.
struct FillingStack: View {
    let children: [Node]
    let spacing: CGFloat
    let drawing: Drawing

    var body: some View {
        VStack(alignment: .leading, spacing: spacing) {
            ForEach(Array(children.enumerated()), id: \.offset) { _, child in
                if child.fillsPane {
                    NodeView(node: child, drawing: drawing).environment(\.ospreyPlacement, .pane).frame(idealHeight: Page.fill, maxHeight: .infinity)
                } else {
                    NodeView(node: child, drawing: drawing).environment(\.ospreyPlacement, .page)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

/// A bounded column: a node that fills takes it whole; anything else scrolls within it.
struct PaneView: View {
    let node: Node
    let drawing: Drawing

    var body: some View {
        if node.fillsPane {
            NodeView(node: node, drawing: drawing).environment(\.ospreyPlacement, .pane)
        } else {
            ScrollView { NodeView(node: node, drawing: drawing).environment(\.ospreyPlacement, .page).padding(.vertical, 1) }
        }
    }
}

/// The first child sizes the layer; later children overlay it without changing its size.
struct LayerView: View {
    let children: [Node]
    let drawing: Drawing

    var body: some View {
        if let first = children.first {
            NodeView(node: first, drawing: drawing).overlay(alignment: .topLeading) {
                ForEach(Array(children.dropFirst().enumerated()), id: \.offset) { _, child in
                    NodeView(node: child, drawing: drawing).allowsHitTesting(!child.passive)
                }
            }
        }
    }
}

/// Stacks are tight: a heading sits on its eyebrow and a list row is compact.
struct StackView: View {
    let axis: Axis
    let children: [Node]
    let drawing: Drawing
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.ospreyPlacement) private var placement
    @Environment(\.ospreyGap) private var gap
    @Environment(\.ospreyStackAlignment) private var alignment

    private var rowAlignment: VerticalAlignment {
        switch alignment {
        case "start": return .top
        case "end": return .bottom
        case "baseline": return .firstTextBaseline
        default: return .center
        }
    }

    var body: some View {
        switch axis {
        case .vertical where placement == .pane: FillingStack(children: children, spacing: gap ?? theme.spacing(2), drawing: drawing)
        case .vertical where placement == .row: VStack(alignment: .leading, spacing: gap ?? 6) { Children(children: children, drawing: drawing) }
        case .vertical: VStack(alignment: .leading, spacing: gap ?? theme.spacing(1)) { Children(children: children, drawing: drawing) }
        case .horizontal: HStack(alignment: rowAlignment, spacing: gap ?? theme.spacing(1)) { Children(children: children, drawing: drawing) }
        }
    }
}

/// A slot shows another region's cell, so only that cell re-renders when its revision moves.
struct RegionView: View {
    let key: String
    @EnvironmentObject private var store: RegionStore

    var body: some View {
        if let cell = store.cell(key) { CellView(cell: cell, store: store) } else { EmptyView() }
    }
}

/// In a pane a region fills it or scrolls within it, as any pane's node does.
struct CellView: View {
    @ObservedObject var cell: RegionCell
    let store: RegionStore
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.ospreyPlacement) private var placement
    @Environment(\.accessibilityReduceMotion) private var reduced

    var body: some View {
        Group {
            if placement == .pane { PaneView(node: cell.node, drawing: drawing) } else { NodeView(node: cell.node, drawing: drawing) }
        }
        .animation(theme.settle(reduced: reduced), value: cell.revision)
    }

    private var drawing: Drawing {
        Drawing(region: cell.key, revision: cell.revision) { ordinal, value in store.send(region: cell.key, revision: cell.revision, ordinal: ordinal, value: value) }
    }
}
