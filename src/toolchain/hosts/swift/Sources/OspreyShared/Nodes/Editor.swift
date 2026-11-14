import SwiftUI

// Implements [UI-COMPONENTS-EDITOR]: a text editor with per-line marks, told as `label`, `value`,
// `inline`, `cursor` (a UTF-8 byte offset), `focusEpoch`, `marks`, `event` (each edit delivers the
// whole text) and `placeEvent` (each caret move delivers its offset). Typing reports the whole
// text; moving the caret reports its offset; nothing else is decided here.

/// The editor: its label, its text in a mathematics-friendly face, and a line of notes per mark.
struct EditorLeaf: View {
    let node: Tree
    let drawing: Drawing
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    private var step: Bool { node.flag("inline") }
    private var size: CGFloat { inherited.size > 0 ? inherited.size : 20 }
    private var dark: Bool { scheme == .dark }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !step && !node.text("label").isEmpty {
                Text(node.text("label")).font(Type.font("body", inherited)).foregroundStyle(.secondary)
            }
            text
                .accessibilityLabel(node.text("label"))
                .frame(minHeight: step ? 0 : 180, maxHeight: .infinity)
            ForEach(Array(node.list("marks").enumerated()), id: \.offset) { _, mark in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text("Line \(mark.count("line"))").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    LeafView(node: .object(["kind": .string("badge"), "label": .string(mark.text("badge")), "symbol": .string(markSymbol(mark)), "tone": .string(mark.text("tone"))]))
                    Text(mark.text("note")).font(.callout).textSelection(.enabled)
                }
            }
        }
    }

    /// The symbol that reads as this mark's tone.
    private func markSymbol(_ mark: Tree) -> String {
        switch mark.text("tone") {
        case "good": return "checkmark.seal"
        case "danger": return "xmark.octagon"
        default: return "exclamationmark.circle"
        }
    }

    @ViewBuilder private var text: some View {
        #if os(macOS)
        CodeText(value: node.text("value"), cursor: node.count("cursor"), focusEpoch: node["focusEpoch"]?.whole ?? 0,
                 underlines: underlines, textSize: size, inline: step,
                 edit: { drawing.send(node.count("event"), .string($0)) },
                 place: { drawing.send(node.count("placeEvent"), .number(Double($0))) })
            .overlay(alignment: .topLeading) {
                if node.text("value").isEmpty {
                    Text(node.text("label")).font(.custom(FontRegistration.serifName, size: size))
                        .foregroundStyle(.secondary)
                        .padding(.top, step ? 12 : 18).padding(.leading, step ? 0 : 16)
                        .allowsHitTesting(false)
                }
            }
        #else
        Refusal(kind: "editor")
        #endif
    }

    private var underlines: [(Int, Color)] {
        node.list("marks").filter { $0.flag("underlined") }.map { ($0.count("line"), tone($0)) }
    }

    private func tone(_ mark: Tree) -> Color {
        switch mark.text("tone") {
        case "good": return theme.color("good", dark: dark) ?? .green
        case "danger": return theme.color("danger", dark: dark) ?? .red
        default: return theme.color("caution", dark: dark) ?? .orange
        }
    }
}

#if os(macOS)
import AppKit

/// A plain-text editor in a mathematics-friendly face, as AppKit draws it.
struct CodeText: NSViewRepresentable {
    @Environment(\.ospreyModalFocusBoundary) private var modalFocusBoundary
    let value: String
    let cursor: Int
    let focusEpoch: Int
    let underlines: [(Int, Color)]
    let textSize: CGFloat
    let inline: Bool
    let edit: (String) -> Void
    let place: (Int) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        guard let text = scroll.documentView as? NSTextView else { return scroll }
        text.isRichText = false
        text.allowsUndo = true
        text.font = face
        text.drawsBackground = false
        scroll.drawsBackground = false
        text.isAutomaticQuoteSubstitutionEnabled = false
        text.isAutomaticDashSubstitutionEnabled = false
        text.isAutomaticSpellingCorrectionEnabled = false
        text.textContainerInset = NSSize(width: inline ? 0 : 16, height: inline ? 12 : 18)
        text.string = value
        text.setSelectedRange(NSRange(location: TextOffsets.convert(value, offset: cursor, sourceUTF8: true), length: 0))
        DispatchQueue.main.async { [weak text] in if let text { text.scrollRangeToVisible(text.selectedRange()) } }
        text.delegate = context.coordinator
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let text = scroll.documentView as? NSTextView else { return }
        text.font = face
        let requested = context.coordinator.requestsFocus(focusEpoch)
        if text.string != value || requested {
            context.coordinator.applying = true
            text.string = value
            let at = TextOffsets.convert(value, offset: cursor, sourceUTF8: true)
            text.setSelectedRange(NSRange(location: at, length: 0))
            text.scrollRangeToVisible(text.selectedRange())
            context.coordinator.applying = false
        }
        decorate(text)
        if requested { focus(text, in: scroll, coordinator: context.coordinator) }
    }

    private var face: NSFont? {
        NSFont(name: FontRegistration.serifName, size: textSize) ?? .systemFont(ofSize: textSize)
    }

    /// The editor takes the keyboard only when its modal boundary, if any, permits it.
    private func focus(_ text: NSTextView, in scroll: NSScrollView, coordinator: Coordinator) {
        guard let window = scroll.window else { return }
        let epoch = focusEpoch
        // AppKit settles key-window and modal ownership after this SwiftUI update.
        DispatchQueue.main.async { [weak text, weak scroll, weak window, weak coordinator] in
            guard let text, let scroll, let window, let coordinator, coordinator.parent.focusEpoch == epoch,
                coordinator.observedFocusEpoch == epoch, scroll.documentView === text,
                text.window === window, window.isKeyWindow,
                ModalFocusBoundary.permits(coordinator.parent.modalFocusBoundary, in: window) else { return }
            window.makeFirstResponder(text)
        }
    }

    /// The dotted underline each underlined mark puts under its line.
    private func decorate(_ text: NSTextView) {
        guard let manager = text.layoutManager else { return }
        let whole = NSRange(location: 0, length: (text.string as NSString).length)
        manager.removeTemporaryAttribute(.underlineStyle, forCharacterRange: whole)
        manager.removeTemporaryAttribute(.underlineColor, forCharacterRange: whole)
        let lines = text.string.components(separatedBy: "\n")
        for (line, color) in underlines where line >= 1 && line <= lines.count {
            let start = lines.prefix(line - 1).reduce(0) { $0 + ($1 as NSString).length + 1 }
            let range = NSRange(location: start, length: (lines[line - 1] as NSString).length)
            manager.addTemporaryAttributes([.underlineStyle: NSUnderlineStyle.thick.rawValue | NSUnderlineStyle.patternDot.rawValue, .underlineColor: NSColor(color)], forCharacterRange: range)
        }
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: CodeText
        var applying = false
        var observedFocusEpoch: Int
        var pendingEdit: (String, Int, (String) -> Void, (Int) -> Void)?
        var selectionGeneration = 0

        init(_ parent: CodeText) { self.parent = parent; observedFocusEpoch = parent.focusEpoch }

        func requestsFocus(_ epoch: Int) -> Bool {
            let requested = epoch > observedFocusEpoch
            observedFocusEpoch = max(observedFocusEpoch, epoch)
            return requested
        }

        func textDidChange(_ notification: Notification) {
            guard !applying, let text = notification.object as? NSTextView else { return }
            let scheduled = pendingEdit != nil
            selectionGeneration += 1
            let at = TextOffsets.convert(text.string, offset: text.selectedRange().location, sourceUTF8: false)
            pendingEdit = (text.string, at, parent.edit, parent.place)
            if !scheduled { DispatchQueue.main.async { [weak self] in self?.flushEdit() } }
        }

        private func flushEdit() {
            let pending = pendingEdit
            pendingEdit = nil
            if let (value, at, edit, place) = pending {
                edit(value)
                if at != value.utf8.count { place(at) }
            }
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            guard !applying, let text = notification.object as? NSTextView else { return }
            // A text mutation also changes selection; its edit must arrive before any separate caret event.
            guard text.string == parent.value else { return }
            let at = TextOffsets.convert(text.string, offset: text.selectedRange().location, sourceUTF8: false)
            guard at != parent.cursor else { return }
            let place = parent.place
            let generation = selectionGeneration
            DispatchQueue.main.async { [weak self] in if self?.selectionGeneration == generation { place(at) } }
        }
    }
}

/// UTF-8 byte offsets to UTF-16 code units and back, as AppKit and the application count them.
enum TextOffsets {
    static func convert(_ value: String, offset: Int, sourceUTF8: Bool) -> Int {
        value.unicodeScalars.reduce((consumed: 0, converted: 0, finished: false)) { current, scalar in
            let source = sourceUTF8 ? scalar.utf8.count : scalar.utf16.count
            let destination = sourceUTF8 ? scalar.utf16.count : scalar.utf8.count
            guard !current.finished && current.consumed + source <= offset else {
                return (current.consumed, current.converted, true)
            }
            return (current.consumed + source, current.converted + destination, false)
        }.converted
    }
}
#endif
