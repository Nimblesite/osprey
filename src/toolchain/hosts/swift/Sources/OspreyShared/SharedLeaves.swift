import SwiftUI

// Implements [NATIVE-SHM-VIEW]'s leaves: the controls a person reads, taps and types into. Each
// one reads its words, binding and flags from its checked row and delivers exactly what the
// contract says — "" for a tap, the whole text for an edit, "true"/"false" for a switch, an
// option's key for a picker — and nothing else.

@MainActor
struct SharedLeaf {
    let store: SharedStore
    let index: Int
    @FocusState.Binding var focus: Int?

    /// Draws a control that holds no children of its own.
    func draw(_ control: RowControl) -> AnyView {
        switch control.kind {
        case .label: return label(control)
        case .press: return press(control)
        case .entry: return field(control)
        case .editor: return editor(control)
        case .toggle: return toggle(control)
        case .picker: return picker(control)
        case .image: return picture(control)
        // An option means nothing outside its picker, so it draws nothing.
        default: return EmptyView().erase()
        }
    }

    /// A label or press led by the icon its hint names.
    private func led(_ control: RowControl, _ body: some View) -> some View {
        HStack(spacing: 4) {
            if !control.hint.isEmpty {
                SharedIcons.symbol(control.hint)
                if !SharedIcons.known(control.hint) {
                    let _ = store.diagnoseOnce("no icon for symbol \(control.hint)")
                }
            }
            body
        }
    }

    private func label(_ control: RowControl) -> AnyView {
        let look = SharedLook(control)
        let text = Text(control.words).font(look.font).foregroundStyle(look.ink ?? .primary)
        let words = look.centred ? AnyView(text.frame(maxWidth: .infinity, alignment: .center)) : AnyView(text)
        return led(control, words).erase()
    }

    private func press(_ control: RowControl) -> AnyView {
        let look = SharedLook(control)
        return led(control, Text(control.words).font(look.font).foregroundStyle(look.ink ?? .primary))
            .contentShape(Rectangle())
            .onTapGesture { deliver(control, "") }
            .opacity(look.disabled ? 0.5 : 1)
            .disabled(look.disabled)
            .erase()
    }

    /// A single-line field: one row index, so the caret and the keyboard focus stay on it while
    /// the view is re-read after each edit. Its hint is its prompt.
    private func field(_ control: RowControl) -> AnyView {
        let look = SharedLook(control)
        let text = Binding(get: { store.words(at: index) },
                           set: { deliver(control, $0) })
        let base = control.fit.flags & SharedFlag.secret != 0
            ? AnyView(SecureField(control.hint, text: text))
            : AnyView(TextField(control.hint, text: text))
        return base
            .textFieldStyle(.plain)
            .font(look.font)
            .foregroundStyle(look.ink ?? .primary)
            .focused($focus, equals: index)
            .id(index)
            .disabled(look.disabled)
            .onSubmit { commit(control) }
            .erase()
    }

    /// A field of many lines; every edit delivers the whole text. A picture on the clipboard
    /// pastes as its saved marker, as the field's person expects on a Mac.
    private func editor(_ control: RowControl) -> AnyView {
        let look = SharedLook(control)
        let text = Binding(get: { store.words(at: index) },
                           set: { deliver(control, $0) })
        return SharedEditor(text: text, look: look, index: index, control: control, focus: $focus)
            .erase()
    }

    private func toggle(_ control: RowControl) -> AnyView {
        let look = SharedLook(control)
        let on = Binding(get: { (store.control(index)?.fit.flags ?? 0) & SharedFlag.chosen != 0 },
                         set: { deliver(control, $0 ? "true" : "false") })
        return Toggle(isOn: on) {
            Text(control.words).font(look.font).foregroundStyle(look.ink ?? .primary)
        }
        .toggleStyle(.switch)
        .disabled(look.disabled)
        .erase()
    }

    /// A picker: its words, then every option child as a press that delivers the option's key,
    /// the chosen one outlined in the picker's ink.
    private func picker(_ control: RowControl) -> AnyView {
        let look = SharedLook(control)
        let options = store.kids(index).compactMap { store.control($0) }
        return VStack(alignment: .leading, spacing: 4) {
            Text(control.words).font(look.font).foregroundStyle(look.ink ?? .primary)
            Flowing(spacing: 6) {
                ForEach(Array(options.enumerated()), id: \.offset) { _, option in
                    optionPress(control, option, look: look)
                }
            }
        }
        .disabled(look.disabled)
        .opacity(look.disabled ? 0.5 : 1)
        .erase()
    }

    private func optionPress(_ picker: RowControl, _ option: RowControl, look: SharedLook) -> some View {
        let chosen = option.fit.flags & SharedFlag.chosen != 0
        let optionLook = SharedLook(option)
        return Text(option.words)
            .font(optionLook.font)
            .foregroundStyle(optionLook.ink ?? .primary)
            .padding(optionLook.inset)
            .background(optionLook.fill.map { AnyView(Color.clear.background($0, in: RoundedRectangle(cornerRadius: optionLook.corner))) } ?? AnyView(Color.clear))
            .overlay(RoundedRectangle(cornerRadius: optionLook.corner)
                .strokeBorder(chosen ? (look.ink ?? .primary) : .clear, lineWidth: 1))
            .contentShape(Rectangle())
            .onTapGesture { deliver(picker, option.hint) }
    }

    /// The picture the row names, by URL or by path.
    private func picture(_ control: RowControl) -> AnyView {
        SharedPicture(source: control.words).erase()
    }

    /// A hairline in the row's ink, its thickness in points.
    func rule(_ control: RowControl) -> AnyView {
        guard let ink = SharedPaint.colour(control.look.ink) else { return EmptyView().erase() }
        return Rectangle().fill(ink)
            .frame(height: max(SharedLook.usable(control.look.size), 1))
            .frame(maxWidth: .infinity)
            .erase()
    }

    /// A field that carries the commit flag also delivers its text on Return.
    private func commit(_ control: RowControl) {
        guard control.fit.flags & SharedFlag.onCommit != 0 else { return }
        deliver(control, store.words(at: index))
    }

    private func deliver(_ control: RowControl, _ words: String) {
        store.send(Touch.control(control.action ?? -1, words))
    }
}

/// A wrapping row of options, as a picker's choices flow.
private struct Flowing: Layout {
    let spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var (x, y, row) = (0.0 as CGFloat, 0.0 as CGFloat, 0.0 as CGFloat)
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width { x = 0; y += row + spacing; row = 0 }
            x += size.width + spacing
            row = max(row, size.height)
        }
        return CGSize(width: min(proposal.width ?? x, x), height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var (x, y, row) = (bounds.minX, bounds.minY, 0.0 as CGFloat)
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX { x = bounds.minX; y += row + spacing; row = 0 }
            view.place(at: CGPoint(x: x, y: y), proposal: .unspecified)
            x += size.width + spacing
            row = max(row, size.height)
        }
    }
}
