import SwiftUI

// Implements [MOBILE-REACTIVE-UI]. All labels, layout and events come from
// Osprey; this renderer only maps each node to the platform's own control, on
// iPhone and on the Mac.
struct NativeNode: Decodable {
    let kind: String
    var id: String?
    var text: String?
    var style: String?
    var value: String?
    var placeholder: String?
    var url: String?
    var submit: Bool?
    var event: [String: String]?
    var children: [NativeNode]?
    var spans: [NativeSpan]?
    var symbol: String?
    var tint: String?
    var accent: String?
    var detail: String?
    var caption: String?
    var key: String?
    var toolbar: [NativeNode]?
    var menu: [NativeNode]?
}

// One run of a `rich` node: Osprey chose the style and link. [MOBILE-MARKDOWN]
struct NativeSpan: Decodable {
    let text: String
    let style: String
    var url: String?
}

struct NativeRenderer: View {
    let node: NativeNode
    let send: ([String: Any]) -> Void

    var body: some View {
        rendered.modifier(NodeStyle(style: node.style ?? ""))
            .accessibilityIdentifier(node.id ?? "")
            .contextMenu {
                // A node's `menu` is its context menu; with none, nothing shows.
                ForEach(Array((node.menu ?? []).enumerated()), id: \.offset) { _, item in
                    Button { send(item.event ?? [:]) } label: { Label(item.text ?? "", systemImage: item.symbol ?? "circle") }
                }
            }
    }

    private var children: some View {
        ForEach(Array((node.children ?? []).enumerated()), id: \.offset) { _, child in
            NativeRenderer(node: child, send: send)
        }
    }

    private var rendered: AnyView {
        switch node.kind {
        case "column": return AnyView(VStack(alignment: .leading, spacing: 14) { children })
        case "row": return AnyView(HStack(alignment: node.style == "item" ? .top : .center,
                                          spacing: node.style == "item" ? 8 : 14) { children })
        case "scroll": return AnyView(ScrollView { children })
        case "text": return AnyView(Text(node.text ?? "").fixedSize(horizontal: false, vertical: true))
        case "rich": return AnyView(Text(richText).fixedSize(horizontal: false, vertical: true))
        case "button": return AnyView(Button(node.text ?? "") { send(node.event ?? [:]) })
        case "input": return AnyView(NativeInput(node: node, send: send))
        case "link": return link
        case "divider": return AnyView(Divider())
        case "navigation": return AnyView(NativeNavigation(node: node, send: send))
        case "entry": return AnyView(NativeEntry(node: node, send: send))
        case "gauge": return AnyView(NativeGauge(node: node))
        case "stack": return AnyView(NativeStack(node: node))
        case "pathbar": return AnyView(NativePathBar(node: node, send: send))
        case "progress": return AnyView(NativeProgress(node: node))
        case "empty": return AnyView(NativeEmpty(node: node))
        default: return AnyView(Text("Unsupported native view: \(node.kind)").foregroundColor(.red))
        }
    }

    // Implements [MOBILE-MARKDOWN]: spans carry Osprey's emphasis, code and
    // HTTPS link decisions; SwiftUI only applies the platform presentation.
    private var richText: AttributedString {
        (node.spans ?? []).reduce(into: AttributedString()) { result, span in
            var run = AttributedString(span.text)
            switch span.style {
            case "bold": run.inlinePresentationIntent = .stronglyEmphasized
            case "italic": run.inlinePresentationIntent = .emphasized
            case "code": run.inlinePresentationIntent = .code
            default: break
            }
            if let raw = span.url, let url = URL(string: raw), url.scheme == "https" { run.link = url }
            result.append(run)
        }
    }

    private var link: AnyView {
        guard let raw = node.url, let url = URL(string: raw), url.scheme == "https" else {
            return AnyView(Text("Invalid link").foregroundColor(.red))
        }
        return AnyView(Link(node.text ?? raw, destination: url))
    }
}

private struct NativeInput: View {
    let node: NativeNode
    let send: ([String: Any]) -> Void
    @State private var value: String
    @FocusState private var focused: Bool

    init(node: NativeNode, send: @escaping ([String: Any]) -> Void) {
        self.node = node
        self.send = send
        _value = State(initialValue: node.value ?? "")
    }

    var body: some View {
        TextField(node.placeholder ?? "", text: $value)
            .modifier(LiteralTyping())
            .submitLabel(node.submit == true ? .go : .search).focused($focused)
            .padding(13).background(Palette.field, in: RoundedRectangle(cornerRadius: 12))
            .onChange(of: value) { _ in if node.submit != true { dispatch() } }
            .onChange(of: node.value) { next in if !focused { value = next ?? "" } }
            .onSubmit { dispatch(); focused = false }
    }

    private func dispatch() {
        var event: [String: Any] = node.event ?? [:]
        event["value"] = value
        send(event)
    }
}

private struct NodeStyle: ViewModifier {
    let style: String
    @Environment(\.colorScheme) private var scheme
    private let fill = Color(red: 0.29, green: 0.25, blue: 0.81)
    private var accent: Color {
        scheme == .dark ? Color(red: 0.68, green: 0.65, blue: 1) : fill
    }

    @ViewBuilder func body(content: Content) -> some View {
        switch style {
        case "screen": content.background(Palette.grouped).tint(accent)
        case "page": content.padding(20).frame(maxWidth: 720)
        case "header": content.padding(.vertical, 14).frame(maxWidth: .infinity, alignment: .leading)
        case "card": content.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.card, in: RoundedRectangle(cornerRadius: 20))
        case "hero": content.font(.system(size: 36, weight: .bold, design: .rounded))
        case "title": content.font(.headline)
        case "caption": content.font(.caption).foregroundColor(.secondary)
        case "accent": content.font(.caption.weight(.bold)).foregroundColor(accent)
        case "error": content.font(.callout).foregroundColor(.red)
        case "heading": content.font(.title3.weight(.semibold))
        case "subheading": content.font(.headline)
        case "code": content.font(.system(.footnote, design: .monospaced)).padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.field, in: RoundedRectangle(cornerRadius: 10))
        case "quote": content.padding(.leading, 12).foregroundColor(.secondary)
                .overlay(alignment: .leading) { RoundedRectangle(cornerRadius: 2).fill(accent.opacity(0.6)).frame(width: 3) }
        case "bullet": content.foregroundColor(.secondary)
        case "item": content.frame(maxWidth: .infinity, alignment: .leading)
        case "primary": content.font(.subheadline.weight(.semibold)).padding(.horizontal, 16).padding(.vertical, 10)
                .foregroundColor(.white).background(fill, in: Capsule())
        case "secondary": content.font(.subheadline.weight(.medium)).padding(.horizontal, 12).padding(.vertical, 10)
                .foregroundColor(accent).background(accent.opacity(0.09), in: Capsule())
        default: content
        }
    }
}

// The few colours and text-entry traits that differ between UIKit and AppKit.
#if os(iOS)
private enum Palette {
    static let grouped = Color(.systemGroupedBackground)
    static let card = Color(.secondarySystemGroupedBackground)
    static let field = Color(.tertiarySystemGroupedBackground)
}

private struct LiteralTyping: ViewModifier {
    func body(content: Content) -> some View { content.textInputAutocapitalization(.never).autocorrectionDisabled() }
}
#else
private enum Palette {
    static let grouped = Color(nsColor: .windowBackgroundColor)
    static let card = Color(nsColor: .controlBackgroundColor)
    static let field = Color(nsColor: .textBackgroundColor)
}

private struct LiteralTyping: ViewModifier {
    func body(content: Content) -> some View { content.autocorrectionDisabled() }
}
#endif

/// Osprey names a system colour; the platform supplies its light and dark shades.
private func systemColor(_ name: String?) -> Color {
    switch name {
    case "blue": return .blue
    case "purple": return .purple
    case "orange": return .orange
    case "pink": return .pink
    case "teal": return .teal
    case "indigo": return .indigo
    case "brown": return .brown
    case "green": return .green
    case "red": return .red
    default: return .gray
    }
}

private func shade(_ color: Color) -> LinearGradient {
    LinearGradient(colors: [color.opacity(0.72), color], startPoint: .top, endPoint: .bottom)
}

/// A node's `value` in thousandths, as Osprey computed it.
private func fraction(_ node: NativeNode) -> Double {
    min(max(Double(node.value ?? "") ?? 0, 0), 1000) / 1000
}

private func indexed(_ nodes: [NativeNode]?) -> [(offset: Int, element: NativeNode)] {
    Array((nodes ?? []).enumerated())
}

// Implements [MOBILE-REACTIVE-UI]: a titled screen of grouped sections with a
// toolbar, as Settings draws one on the Mac and on iPhone.
private struct NativeNavigation: View {
    let node: NativeNode
    let send: ([String: Any]) -> Void

    var body: some View {
        if #available(iOS 16, macOS 13, *) {
            NavigationStack { screen }
        } else {
            NavigationView { screen }
        }
    }

    private var screen: some View {
        Form {
            ForEach(indexed(node.children), id: \.offset) { _, section in
                Section {
                    ForEach(indexed(section.children), id: \.offset) { _, row in NativeRenderer(node: row, send: send) }
                } header: {
                    if let text = section.text, !text.isEmpty { Text(text) }
                } footer: {
                    if let text = section.detail, !text.isEmpty { Text(text) }
                }
            }
        }
        .modifier(GroupedForm())
        .id(node.id ?? "")
        .navigationTitle(node.text ?? "")
        .modifier(Subtitle(text: node.detail ?? ""))
        .toolbar {
            ToolbarItemGroup(placement: .navigation) { items(leading: true) }
            ToolbarItemGroup(placement: .primaryAction) { items(leading: false) }
        }
    }

    private func items(leading: Bool) -> some View {
        ForEach(indexed(node.toolbar).filter { ($0.element.style == "navigation") == leading }, id: \.offset) { _, item in
            NativeToolbarItem(node: item, send: send)
        }
    }
}

private struct GroupedForm: ViewModifier {
    func body(content: Content) -> some View {
        #if os(macOS)
        if #available(macOS 13, *) { content.formStyle(.grouped) } else { content }
        #else
        content
        #endif
    }
}

private struct Subtitle: ViewModifier {
    let text: String

    func body(content: Content) -> some View {
        #if os(macOS)
        content.navigationSubtitle(text)
        #else
        content
        #endif
    }
}

private struct NativeToolbarItem: View {
    let node: NativeNode
    let send: ([String: Any]) -> Void

    var body: some View {
        control.help(node.text ?? "").modifier(Shortcut(key: node.key))
    }

    @ViewBuilder private var control: some View {
        if node.kind == "toggle" {
            Toggle(isOn: Binding(get: { node.value == "true" }, set: { on in
                var event: [String: Any] = node.event ?? [:]
                event["value"] = on ? "true" : "false"
                send(event)
            })) { Label(node.text ?? "", systemImage: node.symbol ?? "circle") }
                .toggleStyle(.button)
        } else {
            Button { send(node.event ?? [:]) } label: { Label(node.text ?? "", systemImage: node.symbol ?? "circle") }
        }
    }
}

/// `command+shift+.` style key equivalents chosen by Osprey.
private struct Shortcut: ViewModifier {
    let key: String?

    func body(content: Content) -> some View {
        if let (equivalent, modifiers) = key.flatMap(Self.parse) {
            content.keyboardShortcut(equivalent, modifiers: modifiers)
        } else {
            content
        }
    }

    private static func parse(_ text: String) -> (KeyEquivalent, EventModifiers)? {
        var parts = text.split(separator: "+").map(String.init)
        guard let last = parts.popLast() else { return nil }
        let names: [String: EventModifiers] = ["command": .command, "shift": .shift, "option": .option, "control": .control]
        let modifiers = parts.reduce(into: EventModifiers()) { result, part in result.formUnion(names[part] ?? []) }
        switch last {
        case "up": return (.upArrow, modifiers)
        case "down": return (.downArrow, modifiers)
        default: return last.count == 1 ? (KeyEquivalent(Character(last)), modifiers) : nil
        }
    }
}

private struct UsageBar: View {
    let fraction: Double
    let color: Color
    let height: CGFloat

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.primary.opacity(0.08))
                Capsule().fill(LinearGradient(colors: [color.opacity(0.7), color], startPoint: .leading, endPoint: .trailing))
                    .frame(width: fraction > 0 ? max(geometry.size.width * fraction, height) : 0)
            }
        }
        .frame(height: height)
    }
}

/// One file or folder: its symbol, name, size, share and a bar against the largest.
private struct NativeEntry: View {
    let node: NativeNode
    let send: ([String: Any]) -> Void

    var body: some View {
        if let event = node.event {
            Button { send(event) } label: { row.contentShape(Rectangle()) }.buttonStyle(.plain)
        } else {
            row
        }
    }

    private var row: some View {
        HStack(spacing: 12) {
            Image(systemName: node.symbol ?? "doc.fill").font(.title3)
                .foregroundStyle(shade(systemColor(node.tint))).frame(width: 30)
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline) {
                    Text(node.text ?? "").lineLimit(1).truncationMode(.middle)
                        .foregroundStyle(node.style == "hidden" ? .secondary : .primary)
                    Spacer(minLength: 12)
                    Text(node.detail ?? "").font(.callout.monospacedDigit()).foregroundStyle(.secondary)
                }
                HStack(spacing: 8) {
                    UsageBar(fraction: fraction(node), color: systemColor(node.accent), height: 5)
                    Text(node.caption ?? "").font(.caption2.monospacedDigit()).foregroundStyle(.secondary)
                        .frame(width: 40, alignment: .trailing)
                }
            }
            if node.event != nil {
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 3)
    }
}

/// A volume's capacity, as the Storage pane shows it.
private struct NativeGauge: View {
    let node: NativeNode

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 14) {
                Image(systemName: node.symbol ?? "internaldrive.fill").font(.system(size: 34))
                    .foregroundStyle(shade(systemColor(node.tint)))
                VStack(alignment: .leading, spacing: 2) {
                    Text(node.text ?? "").font(.title3.weight(.semibold))
                    Text(node.detail ?? "").font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
                }
            }
            UsageBar(fraction: fraction(node), color: systemColor(node.tint), height: 10)
            Text(node.caption ?? "").font(.caption).foregroundStyle(.secondary)
        }
        .padding(.vertical, 6)
    }
}

/// A folder's composition: one coloured run per segment, with a legend.
private struct NativeStack: View {
    let node: NativeNode
    private var segments: [NativeNode] { (node.children ?? []).filter { fraction($0) > 0 } }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            GeometryReader { geometry in
                HStack(spacing: 2) {
                    ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
                        Rectangle().fill(shade(systemColor(segment.tint)))
                            .frame(width: max(3, (geometry.size.width - CGFloat(segments.count - 1) * 2) * fraction(segment)))
                    }
                }
            }
            .frame(height: 16)
            .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), alignment: .leading)], alignment: .leading, spacing: 8) {
                ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
                    HStack(spacing: 6) {
                        Circle().fill(systemColor(segment.tint)).frame(width: 8, height: 8)
                        Text(segment.text ?? "").font(.caption).lineLimit(1).truncationMode(.middle)
                        Text(segment.detail ?? "").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .padding(.vertical, 6)
    }
}

/// The folders from the measured root to the open one, each a way back.
private struct NativePathBar: View {
    let node: NativeNode
    let send: ([String: Any]) -> Void

    var body: some View {
        let crumbs = node.children ?? []
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Array(crumbs.enumerated()), id: \.offset) { index, crumb in
                    if index > 0 {
                        Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(.tertiary)
                    }
                    Button { send(crumb.event ?? [:]) } label: {
                        HStack(spacing: 4) {
                            Image(systemName: crumb.symbol ?? "folder.fill").foregroundStyle(Color.blue)
                            Text(crumb.text ?? "").fontWeight(index == crumbs.count - 1 ? .semibold : .regular)
                                .foregroundStyle(index == crumbs.count - 1 ? .primary : .secondary)
                        }
                        .font(.callout)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}

private struct NativeProgress: View {
    let node: NativeNode

    var body: some View {
        HStack(spacing: 14) {
            ProgressView()
            VStack(alignment: .leading, spacing: 3) {
                Text(node.text ?? "").font(.headline)
                Text(node.detail ?? "").font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 8)
    }
}

private struct NativeEmpty: View {
    let node: NativeNode

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: node.symbol ?? "tray").font(.system(size: 40, weight: .light))
                .foregroundStyle(shade(systemColor(node.tint)))
            Text(node.text ?? "").font(.headline)
            Text(node.detail ?? "").font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }
}
