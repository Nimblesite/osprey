import SwiftUI
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW]'s leaves: the controls a person reads, taps and
// types into. Each one reads its words, binding and flags from its row and
// delivers exactly what the contract says — "" for a tap, the whole text for an
// edit, "true"/"false" for a switch — and nothing else.

extension SharedRow {
    /// Draws a control that holds no children of its own.
    func leaf(_ row: OspNativeView) -> AnyView {
        switch SharedKind(rawValue: row.kind) {
        case .label: return label(row)
        case .press: return press(row)
        case .entry: return field(row)
        case .editor: return editor(row)
        case .toggle: return toggle(row)
        case .picker: return picker(row)
        case .image: return picture(row)
        // An option means nothing outside its picker's menu, so it draws nothing.
        default: return AnyView(EmptyView())
        }
    }

    private func label(_ row: OspNativeView) -> AnyView {
        let look = SharedLook(row)
        let text = Text(store.words(row)).font(look.font).foregroundStyle(look.ink ?? .primary)
        guard look.centred else { return AnyView(text) }
        return AnyView(text.frame(maxWidth: .infinity, alignment: .center))
    }

    private func press(_ row: OspNativeView) -> AnyView {
        let look = SharedLook(row)
        return AnyView(Button { store.event(action: row.action, text: "") } label: {
            Text(store.words(row)).font(look.font).foregroundStyle(look.ink ?? .primary)
        }
        .buttonStyle(.plain)
        .disabled(look.disabled))
    }

    /// A single-line field: one row index, so the caret and the keyboard focus
    /// stay on it while the view is re-read after each edit.
    private func field(_ row: OspNativeView) -> AnyView {
        let text = Binding(get: { store.words(at: index) },
                           set: { store.event(action: store.action(at: index), text: $0) })
        return AnyView((row.has(SharedFlag.secure)
            ? AnyView(SecureField(store.hint(row), text: text))
            : AnyView(TextField(store.hint(row), text: text)))
            .textFieldStyle(.plain)
            .focused($focus, equals: index)
            .id(index)
            .disabled(row.has(SharedFlag.disabled))
            .onSubmit { commit(row) })
    }

    /// A field of many lines; every edit delivers the whole text.
    private func editor(_ row: OspNativeView) -> AnyView {
        let text = Binding(get: { store.words(at: index) },
                           set: { store.event(action: store.action(at: index), text: $0) })
        return AnyView(TextEditor(text: text)
            .font(SharedLook(row).font)
            .focused($focus, equals: index)
            .id(index)
            .disabled(row.has(SharedFlag.disabled)))
    }

    private func toggle(_ row: OspNativeView) -> AnyView {
        let look = SharedLook(row)
        let on = Binding(get: { store.row(index)?.has(SharedFlag.chosen) ?? false },
                         set: { store.event(action: row.action, text: $0 ? "true" : "false") })
        return AnyView(Toggle(isOn: on) {
            Text(store.words(row)).font(look.font).foregroundStyle(look.ink ?? .primary)
        }
        .toggleStyle(.switch)
        .disabled(look.disabled))
    }

    /// A menu whose kind-10 children are the options; choosing one delivers the
    /// picker's action with that option's key, and the chosen one is marked.
    private func picker(_ row: OspNativeView) -> AnyView {
        let look = SharedLook(row)
        return AnyView(Menu {
            ForEach(store.kids(index), id: \.self) { child in
                if let option = store.row(child) {
                    Button { store.event(action: row.action, text: store.hint(option)) } label: {
                        choice(option)
                    }
                }
            }
        } label: {
            Text(store.words(row)).font(look.font).foregroundStyle(look.ink ?? .primary)
        }
        .disabled(look.disabled))
    }

    /// One option's label, marked with the system's checkmark when it is chosen.
    private func choice(_ option: OspNativeView) -> AnyView {
        guard option.has(SharedFlag.chosen) else { return AnyView(Text(store.words(option))) }
        return AnyView(Label(store.words(option), systemImage: "checkmark"))
    }

    /// The picture the row names, by URL or by path.
    private func picture(_ row: OspNativeView) -> AnyView {
        guard let url = SharedRow.source(store.words(row)) else { return AnyView(EmptyView()) }
        return AnyView(AsyncImage(url: url) { image in image.resizable() } placeholder: { Color.clear })
    }

    /// A hairline in the row's ink, its thickness in points.
    func rule(_ row: OspNativeView) -> AnyView {
        guard let ink = SharedPaint.colour(row.colour) else { return AnyView(EmptyView()) }
        return AnyView(Rectangle().fill(ink).frame(height: max(OspNativeView.usable(row.size), 1)))
    }

    /// A canvas reserves the fixed extent its row asks for; its shapes are not
    /// drawn yet, so it draws no content of its own.
    func canvas(_ row: OspNativeView) -> AnyView {
        AnyView(Color.clear.frame(width: OspNativeView.usable(row.width),
                                  height: OspNativeView.usable(row.height)))
    }

    /// A menu shows its first child and opens the rest as context-menu items.
    func menu(_ row: OspNativeView) -> AnyView {
        let items = store.kids(index)
        guard let first = items.first else { return AnyView(EmptyView()) }
        return AnyView(SharedRow(store: store, index: first, axis: .none, focus: $focus)
            .contextMenu {
                ForEach(Array(items.dropFirst()), id: \.self) { child in
                    if let item = store.row(child) {
                        Button(store.words(item)) { store.event(action: item.action, text: "") }
                    }
                }
            })
    }

    /// A sheet draws its children above its own dismiss area, so the view
    /// underneath stays visible. Tapping outside them dismisses it and delivers
    /// its action with no text.
    func sheet(_ row: OspNativeView) -> AnyView {
        AnyView(ZStack {
            Color.clear.contentShape(Rectangle())
                .onTapGesture { store.event(action: row.action, text: "") }
            VStack(alignment: .leading, spacing: OspNativeView.usable(row.spacing)) { kids(.vertical) }
        })
    }

    /// A field that carries the commit flag also delivers its text on Return.
    private func commit(_ row: OspNativeView) {
        guard row.has(SharedFlag.onCommit) else { return }
        store.event(action: store.action(at: index), text: store.words(at: index))
    }

    private static func source(_ words: String) -> URL? {
        guard !words.isEmpty else { return nil }
        if let url = URL(string: words), url.scheme != nil { return url }
        return URL(fileURLWithPath: words)
    }
}
