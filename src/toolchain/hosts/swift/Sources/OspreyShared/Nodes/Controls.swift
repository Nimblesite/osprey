import Observation
import SwiftUI

// Implements [UI-COMPONENTS] and [UI-EVENTS]: controls. Only text being composed
// lives here; every committed value goes to Osprey and comes back as the node.

struct ControlView: View {
    let node: Tree
    let drawing: Drawing

    var body: some View {
        switch node.text("kind") {
        case "button": ButtonView(node: node) { drawing.act(node, .null) }
        case "field": FieldView(node: node, revision: drawing.revision) { drawing.send(node.count("event"), .string($0)) }
        case "toggle": Toggle(node.text("label"), isOn: Binding(get: { node.flag("on") }, set: { drawing.send(node.count("event"), .bool($0)) })).toggleStyle(.switch).controlSize(.small)
        case "picker": PickerView(node: node) { drawing.act(node, .string($0)) }.equatable()
        case "disclosure": DisclosureView(node: node, drawing: drawing)
        default: AlertView(node: node) { drawing.send(node.count("event"), .null) }
        }
    }
}

struct ButtonView: View {
    let node: Tree
    let tap: () -> Void
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @FocusState private var focused: Bool

    private var symbol: String { node.text("symbol") }
    private var iconOnly: Bool { inherited.iconOnly && !symbol.isEmpty }

    var body: some View {
        Button(action: tap) { face }
            .buttonStyle(Surface(role: node.text("role"), quiet: inherited.quiet, round: iconOnly, height: inherited.controlHeight, padding: iconOnly ? 0 : inherited.controlPadding, theme: theme, dark: scheme == .dark, focused: focused))
            .focused($focused)
            .disabled(!node.flag("enabled"))
            .help(node.text("label"))
            .accessibilityLabel(node.text("label"))
            .background { if ButtonFrames.recording { ButtonFrames.Recorder(name: node.text("label")) } }
    }

    @ViewBuilder private var face: some View {
        let size = inherited.symbolSize > 0 ? inherited.symbolSize : iconOnly ? 18 : 12
        let glyph = NativeSymbol(name: symbol, size: size, weight: .medium)
        if iconOnly {
            glyph
        } else {
            HStack(spacing: 6) {
                if !symbol.isEmpty && !inherited.trailingSymbols { glyph }
                if !node.text("label").isEmpty { Text(node.text("label")).font(Type.font("body", inherited)).lineLimit(1) }
                if !symbol.isEmpty && inherited.trailingSymbols { glyph }
            }
        }
    }
}

/// Where each button is drawn, kept only during a self-test so its `press` step can click there.
@MainActor enum ButtonFrames {
    @Observable fileprivate final class Recording {
        var enabled = false
    }

    private static let state = Recording()
    static var recording: Bool {
        get { state.enabled }
        set { state.enabled = newValue }
    }
    static var frames: [String: CGRect] = [:]

    struct Recorder: View {
        let name: String

        var body: some View {
            GeometryReader { geometry in
                Color.clear
                    .onAppear { frames[name] = geometry.frame(in: .global) }
                    .onChange(of: geometry.frame(in: .global)) { _, next in frames[name] = next }
            }
        }
    }
}

/// A button's surface by role: quiet buttons show only on hover, as a toolbar's do.
struct Surface: ButtonStyle {
    let role: String
    let quiet: Bool
    let round: Bool
    let height: CGFloat
    let padding: CGFloat
    let theme: ThemeResolver
    let dark: Bool
    var focused = false

    func makeBody(configuration: Configuration) -> some View {
        SurfaceFace(configuration: configuration, style: self)
    }

    private struct SurfaceFace: View {
        let configuration: Configuration
        let style: Surface
        @State private var hovering = false
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            configuration.label
                .foregroundStyle(ink)
                .padding(.horizontal, style.padding >= 0 ? style.padding : 12)
                .frame(minWidth: minimumWidth, minHeight: max(minimumTouch, style.height > 0 ? style.height : 28))
                .background(fill, in: RoundedRectangle(cornerRadius: style.round ? max(style.height, 28) / 2 : 8, style: .continuous))
                .overlay { focusRing }
                .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .opacity(enabled ? 1 : 0.45)
                .scaleEffect(configuration.isPressed ? 0.97 : 1)
                .animation(.easeOut(duration: 0.12), value: hovering)
                .animation(.easeOut(duration: 0.08), value: configuration.isPressed)
                .onHover { hovering = $0 }
        }

        /// Keyboard focus paints a two-point accent outline three points outside the button,
        /// on the theme's accent, as the application authored it.
        @ViewBuilder private var focusRing: some View {
            if style.focused {
                RoundedRectangle(cornerRadius: style.round ? max(style.height, 28) / 2 : 8, style: .continuous)
                    .strokeBorder(accent, lineWidth: 2).padding(-3)
            }
        }

        private var accent: Color { style.theme.color("accent", dark: style.dark) ?? .accentColor }

        // [CAST-DESIGN-NATIVE] Desktop metrics remain native; phone controls keep a touch-sized target.
        private var minimumTouch: CGFloat {
            #if os(iOS)
            return 44
            #else
            return 0
            #endif
        }

        private var minimumWidth: CGFloat? {
            if minimumTouch > 0 { return max(minimumTouch, style.round ? style.height : 0) }
            return style.round && style.height > 0 ? style.height : nil
        }

        private var ink: AnyShapeStyle {
            switch style.role {
            case "filled", "prominent": return AnyShapeStyle(style.theme.color("onAccent", dark: style.dark) ?? .white)
            case "destructive": return AnyShapeStyle(style.theme.color("danger", dark: style.dark) ?? .red)
            default: return style.theme.foreground(dark: style.dark)
            }
        }

        private var fill: AnyShapeStyle {
            switch style.role {
            case "filled", "prominent": return AnyShapeStyle(accent.opacity(hovering && enabled ? 0.88 : 1))
            default:
                if style.quiet { return AnyShapeStyle(Color.primary.opacity(hovering && enabled ? 0.09 : 0)) }
                return AnyShapeStyle(Color.primary.opacity(hovering && enabled ? 0.12 : 0.07))
            }
        }
    }
}

struct FieldView: View {
    let node: Tree
    let revision: Int
    let send: (String) -> Void
    @State private var draft: String
    @FocusState private var focused: Bool
    @State private var pasteMonitor: Any?
    @State private var pasteID = UUID()
    @Environment(\.ospreyInherited) private var inherited
    private static let singleLineHeight: CGFloat = 25

    init(node: Tree, revision: Int, send: @escaping (String) -> Void) {
        self.node = node
        self.revision = revision
        self.send = send
        _draft = State(initialValue: node.text("value"))
    }

    private var value: String { node.text("value") }
    private var edits: Bool { node.text("commit") == "edit" }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if !node.text("label").isEmpty && !node.flag("multiline") { Text(node.text("label")).font(Type.font("caption", inherited)).foregroundStyle(.secondary) }
            input
                .focused($focused)
                .font(Type.font("body", inherited))
                .autocorrectionDisabled()
                .accessibilityLabel(node.text("label"))
        }
        .onAppear { if node.flag("multiline") { DispatchQueue.main.async { focused = true } } }
        #if os(macOS)
        .onAppear { if node.flag("multiline") && pasteMonitor == nil { pasteMonitor = monitorPaste() } }
        .onChange(of: focused) { _, now in if now && pasteMonitor != nil { PastedImage.receivers.insert(pasteID) } else { PastedImage.receivers.remove(pasteID) } }
        .onDisappear { PastedImage.receivers.remove(pasteID); if let pasteMonitor { NSEvent.removeMonitor(pasteMonitor) }; pasteMonitor = nil }
        #endif
        .onChange(of: draft) { _, next in if edits && next != value { send(next) } }
        .onChange(of: value) { _, next in if next != draft { draft = next } }
        .onChange(of: revision) { _, _ in if !edits && value != draft { draft = value } }
    }

    @ViewBuilder private var input: some View {
        if node.flag("secure") {
            SecureField("", text: $draft, prompt: Text(node.text("prompt"))).onSubmit(submit).textFieldStyle(.roundedBorder)
        } else if node.flag("multiline") {
            // Return is a newline at the end of the text, on every host; the application decides what it means.
            TextField("", text: $draft, prompt: Text(node.text("prompt")), axis: .vertical).lineLimit(1...12).textFieldStyle(.plain).onSubmit(returned)
        } else {
            // A fixed height: left to fit, a field in a growing frame settles at 22 or 25 points from one layout to the next.
            TextField("", text: $draft, prompt: Text(node.text("prompt"))).onSubmit(submit).textFieldStyle(inherited.quiet ? AnyTextFieldStyle(.plain) : AnyTextFieldStyle(.roundedBorder)).frame(height: Self.singleLineHeight)
        }
    }

    private func submit() { if !edits { send(draft) } }

    #if os(macOS)
    /// Cmd-V with a picture on the clipboard: a text field takes only text, so the picture is
    /// saved and its marker joins the words. Anything else is left to the field.
    private func monitorPaste() -> Any? {
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard focused, event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command,
                  event.charactersIgnoringModifiers == "v", let marker = PastedImage.take() else { return event }
            draft = draft.isEmpty ? marker : "\(draft) \(marker)"
            return nil
        }
    }
    #endif

    private func returned() {
        if edits { draft += "\n" } else { send(draft) }
    }
}

/// A type-erased text field style, so one field can be quiet or bordered.
struct AnyTextFieldStyle: TextFieldStyle {
    private let plain: Bool
    init(_ style: PlainTextFieldStyle) { plain = true }
    init(_ style: RoundedBorderTextFieldStyle) { plain = false }

    func _body(configuration: TextField<_Label>) -> some View {
        if plain { configuration.textFieldStyle(.plain) } else { configuration.textFieldStyle(.roundedBorder) }
    }
}

struct PickerView: View, Equatable {
    let node: Tree
    let choose: (String) -> Void
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    // Frame-only revisions must not replace an open native menu and restart its animation.
    static func == (left: Self, right: Self) -> Bool { left.node == right.node }

    private var selected: String {
        node.list("choices").first { $0.text("key") == node.text("selected") }?.text("label") ?? node.text("selected")
    }

    var body: some View {
        Menu {
            ForEach(node.list("choices"), id: \.keyOfRow) { choice in
                Button(choice.text("label")) { choose(choice.text("key")) }
            }
        } label: {
            HStack(spacing: 4) {
                Text(selected).font(Type.font("body", inherited))
                Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold))
            }
            .foregroundStyle(theme.color("secondary", dark: scheme == .dark) ?? .secondary)
        }
        .menuStyle(.button)
        .buttonStyle(Surface(role: "plain", quiet: true, round: false, height: inherited.controlHeight, padding: 8, theme: theme, dark: scheme == .dark))
        .menuIndicator(.hidden)
        .fixedSize(horizontal: false, vertical: true)
        .lineLimit(1)
        .accessibilityLabel(node.text("label"))
        .accessibilityValue(selected)
    }
}

struct DisclosureView: View {
    let node: Tree
    let drawing: Drawing
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.ospreyReveal) private var reveal
    @State private var hovering = false
    @State private var id = UUID()

    var body: some View {
        let open = node.flag("expanded")
        VStack(alignment: .leading, spacing: 6) {
            Button { drawing.send(node.count("event"), .bool(!open)) } label: {
                HStack(spacing: 5) {
                    Text(node.text("title")).font(Type.font("body", inherited))
                    Image(systemName: "chevron.right").font(.system(size: 8, weight: .bold)).rotationEffect(.degrees(open ? 90 : 0)).opacity(hovering || open ? 1 : 0.5)
                }
                .foregroundStyle(hovering ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { hovering = $0 }
            if open {
                VStack(alignment: .leading, spacing: inherited.gap ?? theme.spacing(1)) { Children(nodes: node.list("children"), drawing: drawing) }
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .id(id)
        .animation(.snappy(duration: 0.2), value: open)
        .onChange(of: open) { _, opened in
            // After the column has laid out what opened.
            if opened { DispatchQueue.main.async { reveal?(id) } }
        }
    }
}

struct AlertView: View {
    let node: Tree
    let dismiss: () -> Void

    var body: some View {
        Color.clear.frame(width: 0, height: 0)
            .alert(node.text("title"), isPresented: Binding(get: { node.flag("visible") }, set: { if !$0 { dismiss() } })) {
                Button("OK", action: dismiss)
            } message: { Text(node.text("message")) }
    }
}

/// [UI-COMPONENTS] Selectable rows: the native source list in the sidebar, a flat list elsewhere,
/// or the card list a `flatRows` frame authors.
struct RowsView: View {
    let node: Tree
    let drawing: Drawing
    @Environment(\.ospreyPlacement) private var placement
    @Environment(\.ospreyInherited) private var inherited

    private var selected: String { node.text("selected") }
    private var rows: [Tree] { node.list("rows") }

    private var groups: [(title: String, rows: [Tree])] {
        rows.reduce(into: []) { groups, row in
            if groups.last?.title == row.text("group") { groups[groups.count - 1].rows.append(row) } else { groups.append((row.text("group"), [row])) }
        }
    }

    private func choose(_ key: String) { if key != selected { DispatchQueue.main.async { drawing.send(node.count("event"), .string(key)) } } }

    var body: some View {
        if placement == .sidebar { source } else if inherited.flatRows { authored } else { flat }
    }

    private var source: some View {
        List(selection: Binding(get: { selected.isEmpty ? nil : selected }, set: { if let key = $0 { choose(key) } })) {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                if group.title.isEmpty { items(group.rows) } else {
                    Section(group.title) { items(group.rows) }
                    #if os(macOS)
                        .collapsible(false)
                    #endif
                }
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .frame(maxHeight: .infinity)
    }

    private func items(_ rows: [Tree]) -> some View {
        ForEach(rows, id: \.keyOfRow) { row in
            HStack(spacing: 8) {
                if !row.text("symbol").isEmpty { NativeSymbol(name: row.text("symbol"), size: 12).foregroundStyle(.secondary) }
                NodeView(node: row["child"] ?? .null, drawing: drawing)
            }
            .tag(row.text("key"))
        }
    }

    private var flat: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(rows, id: \.keyOfRow) { row in
                FlatRow(row: row, selected: row.text("key") == selected, drawing: drawing) { choose(row.text("key")) }
            }
        }
    }

    /// The list an application authored: consecutive rows that name one group under its heading,
    /// each row a card whose flat surface shows selection as a tint, never colour alone.
    private var authored: some View {
        LazyVStack(alignment: .leading, spacing: inherited.gap ?? theme.spacing(2)) {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                if !group.title.isEmpty {
                    Text(group.title).font(Type.font("eyebrow", inherited)).foregroundStyle(.secondary).padding(.top, theme.spacing(1))
                }
                ForEach(group.rows, id: \.keyOfRow) { row in
                    AuthoredRow(row: row, selected: row.text("key") == selected, drawing: drawing) { choose(row.text("key")) }
                }
            }
        }
    }

    @Environment(\.ospreyTheme) private var theme
}

extension Tree {
    var keyOfRow: String { text("key") }
}

struct FlatRow: View {
    let row: Tree
    let selected: Bool
    let drawing: Drawing
    let tap: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: tap) {
            HStack(spacing: 8) {
                if !row.text("symbol").isEmpty { NativeSymbol(name: row.text("symbol"), size: 12).foregroundStyle(.secondary) }
                NodeView(node: row["child"] ?? .null, drawing: drawing)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8).padding(.vertical, 6)
            .background(Color.primary.opacity(selected ? 0.1 : hovering ? 0.05 : 0), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// One authored row: symbol, content and a disclosure chevron on the card surface a `flatRows`
/// frame flattens to a tinted full-row selection.
struct AuthoredRow: View {
    let row: Tree
    let selected: Bool
    let drawing: Drawing
    let tap: () -> Void
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        Button(action: tap) {
            HStack(alignment: .top, spacing: 14) {
                if !row.text("symbol").isEmpty { NativeSymbol(name: row.text("symbol"), size: 22).frame(width: 30).padding(.top, 3) }
                NodeView(node: row["child"] ?? .null, drawing: drawing)
                Spacer(minLength: 0)
            }
            .padding(0)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(theme.color(selected ? "selection" : "", dark: scheme == .dark) ?? .clear, in: RoundedRectangle(cornerRadius: 3, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 3, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
