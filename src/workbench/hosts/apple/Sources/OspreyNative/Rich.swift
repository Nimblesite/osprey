import SwiftUI

// Implements [UI-COMPONENTS-EDITOR], [UI-COMPONENTS-BOARD], [UI-COMPONENTS-MATH]
// and the media components for the Apple host. Each view draws what Osprey
// sent and reports only the control's value; none decides anything.

/// Media and file paths the application names are relative to the bundled resources.
public enum Bundled {
    public static func url(_ path: String) -> URL? {
        guard !path.isEmpty, !path.hasPrefix("/"), !path.split(separator: "/").contains("..") else { return nil }
        return Bundle.main.resourceURL?.appendingPathComponent(path)
    }

    public static func image(_ path: String) -> NSImage? { url(path).flatMap { NSImage(contentsOf: $0) } }
}

struct RichView: View {
    let node: Node
    let drawing: Drawing
    @Environment(\.ospreyPresentation) private var presentation
    @Environment(\.ospreyTypography) private var typography

    var body: some View {
        switch node {
        case let .editor(label, value, inline, cursor, focusEpoch, marks, event, placeEvent):
            EditorView(label: label, value: value, step: inline, cursor: cursor, focusEpoch: focusEpoch, marks: marks, edit: { drawing.edit(event, .string($0)) }, place: { drawing.edit(placeEvent, .number(Double($0))) })
        case let .board(slice, count, lasso, backdrop, panning, panEvent, event, lassoEvent):
            BoardView(slice: slice, count: count, lassoing: lasso, backdrop: Bundled.image(backdrop), panning: panning, pan: { if let panEvent { drawing.send(panEvent, .string($0)) } }, stroke: { drawing.send(event, $0) }, lasso: { drawing.send(lassoEvent, .array($0.map { .number(Double($0)) })) })
        case let .math(tree, spoken): MathViewport(tree: tree, size: CGFloat(typography.size > 0 ? typography.size : 26) * typography.mathScale, spoken: spoken)
        case let .image(source, alt, width, height): StillView(source: source, alt: alt, width: width, height: height)
        case let .video(source, poster, captions, alt, seconds): FilmView(source: source, poster: poster, captions: captions, alt: alt, seconds: seconds)
        case let .contextMenu(content, items):
            NodeView(node: content, drawing: drawing).contextMenu { Children(children: items, drawing: drawing) }
        default: EmptyView()
        }
    }
}

// MARK: Editor

/// The marks of the editor: the line's tone, a badge in the gutter list and its note on hover.
struct EditorView: View {
    let label: String
    let value: String
    let step: Bool
    let cursor: Int
    let focusEpoch: Int
    let marks: [Mark]
    let edit: (String) -> Void
    let place: (Int) -> Void
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyPresentation) private var presentation

    @Environment(\.ospreyTypography) private var typography

    private var prompt: String { step ? label : "State the problem, then work through your reasoning…" }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !step { Text(label).font(.custom("Geist", size: 13)).foregroundStyle(theme.tone(.secondary, dark: scheme == .dark)) }
            #if os(macOS)
            CodeText(value: value, cursor: cursor, focusEpoch: focusEpoch, underlines: underlines, textSize: typography.size > 0 ? CGFloat(typography.size) : 20, inline: step, ink: theme.tone(.primary, dark: scheme == .dark), edit: edit, place: place)
                .accessibilityLabel(label)
                .frame(minHeight: step ? 0 : 180, maxHeight: .infinity)
                .overlay(alignment: .topLeading) { if value.isEmpty { Text(prompt).font(typography.font(fallback: .custom(FontRegistration.serifName, size: 20))).foregroundStyle(theme.tone(.secondary, dark: scheme == .dark)).padding(.top, step ? 12 : 18).padding(.leading, step ? 0 : 16).allowsHitTesting(false) } }
            #endif
            ForEach(Array(marks.enumerated()), id: \.offset) { _, mark in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text("Line \(mark.line)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    BadgeView(label: mark.badge, symbol: symbol(mark.tone), color: theme.tone(mark.tone, dark: scheme == .dark))
                    Text(mark.note).font(.callout).textSelection(.enabled)
                }
            }
        }
    }

    private var underlines: [(Int, Color)] {
        marks.filter(\.underlined).map { ($0.line, theme.tone($0.tone, dark: scheme == .dark)) }
    }

    private func symbol(_ tone: Tone) -> String {
        switch tone {
        case .good: return "checkmark.seal"
        case .danger: return "xmark.octagon"
        default: return "exclamationmark.circle"
        }
    }
}

#if os(macOS)
import AppKit

/// A plain-text editor in a mathematics-friendly face. Typing reports the whole text; moving the caret reports its offset.
struct CodeText: NSViewRepresentable {
    @Environment(\.ospreyModalFocusBoundary) private var modalFocusBoundary
    let value: String
    let cursor: Int
    var focusEpoch = 0
    let underlines: [(Int, Color)]
    let textSize: CGFloat
    let inline: Bool
    let ink: Color
    let edit: (String) -> Void
    let place: (Int) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        guard let text = scroll.documentView as? NSTextView else { return scroll }
        text.isRichText = false
        text.allowsUndo = true
        text.font = NSFont(name: FontRegistration.serifName, size: textSize) ?? .systemFont(ofSize: textSize)
        text.textColor = NSColor(ink)
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
        text.textColor = NSColor(ink)
        text.font = NSFont(name: FontRegistration.serifName, size: textSize) ?? .systemFont(ofSize: textSize)
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

    private func focus(_ text: NSTextView, in scroll: NSScrollView, coordinator: Coordinator) {
        guard let window = scroll.window, window.isKeyWindow, ModalFocusBoundary.permits(modalFocusBoundary, in: window) else { return }
        let epoch = focusEpoch
        // Apply authored focus after the current SwiftUI view-update transaction returns.
        DispatchQueue.main.async { [weak text, weak scroll, weak window, weak coordinator] in
            guard let text, let scroll, let window, let coordinator, coordinator.parent.focusEpoch == epoch,
                coordinator.observedFocusEpoch == epoch, scroll.documentView === text,
                text.window === window, window.isKeyWindow,
                ModalFocusBoundary.permits(coordinator.parent.modalFocusBoundary, in: window) else { return }
            window.makeFirstResponder(text)
        }
    }

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
#endif

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

// MARK: Board

struct Stroke: Decodable {
    let points: [[Int]]
    let width: Int
    let at: Int
}

struct BoardFrame: Decodable {
    struct Size: Decodable { let width: Int; let height: Int }
    struct Camera: Decodable { let x: Int; let y: Int; let scale: Int }
    let size: Size
    let view: Camera
}

/// The board: strokes are read from the host-held slices `<slice>:<n>`; a drag draws, and with Shift held it lassos strokes to read.
struct BoardView: View {
    let slice: String
    let count: Int
    let lassoing: Bool
    let backdrop: NSImage?
    let panning: Bool
    let pan: (String) -> Void
    let stroke: (JSONValue) -> Void
    let lasso: ([Int]) -> Void
    @State private var live: [CGPoint] = []
    @State private var selecting = false
    @State private var cameraTranslation = CGSize.zero
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyFadeBackdrop) private var fadeBackdrop

    private var metadata: BoardFrame? { try? JSONDecoder().decode(BoardFrame.self, from: Data(SliceStore.shared.text(slice).utf8)) }

    private var frame: CGSize {
        let decoded = metadata
        return CGSize(width: decoded?.size.width ?? 1536, height: decoded?.size.height ?? 1024)
    }

    private var strokes: [Stroke] {
        (0..<max(count, 0)).compactMap { try? JSONDecoder().decode(Stroke.self, from: Data(SliceStore.shared.text("\(slice):\($0)").utf8)) }
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / frame.width, proxy.size.height / frame.height) * CGFloat(metadata?.view.scale ?? 100) / 100
            let offset = CGPoint(x: (proxy.size.width - frame.width * scale) / 2 + CGFloat(metadata?.view.x ?? 0) * scale + cameraTranslation.width, y: (proxy.size.height - frame.height * scale) / 2 + CGFloat(metadata?.view.y ?? 0) * scale + cameraTranslation.height)
            Canvas { context, _ in
                let chalk = theme.named("chalk", dark: scheme == .dark) ?? theme.tone(.primary, dark: scheme == .dark)
                for item in strokes {
                    context.stroke(path(item.points.map { CGPoint(x: CGFloat($0[0]), y: CGFloat($0[1])) }, scale, offset), with: .color(chalk), style: StrokeStyle(lineWidth: CGFloat(item.width) * scale, lineCap: .round, lineJoin: .round))
                }
                let ink = selecting ? theme.tone(.accent, dark: scheme == .dark) : chalk
                context.stroke(path(live, scale, offset), with: .color(ink), style: StrokeStyle(lineWidth: selecting ? 1.5 : 3 * scale, lineCap: .round, lineJoin: .round, dash: selecting ? [6, 4] : []))
            }
            .background {
                if let image = backdrop {
                    Image(nsImage: image).resizable().frame(width: frame.width * scale, height: frame.height * scale).modifier(BackdropFade(enabled: fadeBackdrop)).position(x: offset.x + frame.width * scale / 2, y: offset.y + frame.height * scale / 2)
                } else { theme.named("board", dark: scheme == .dark) ?? theme.color("canvas", dark: scheme == .dark) }
            }
            .clipped()
            .gesture(DragGesture(minimumDistance: 0).onChanged { change in
                if panning { cameraTranslation = change.translation; return }
                if live.isEmpty { selecting = lassoing && NSEvent.modifierFlags.contains(.shift) }
                live.append(CGPoint(x: (change.location.x - offset.x) / scale, y: (change.location.y - offset.y) / scale))
            }.onEnded { _ in
                if panning { finishCamera(scale); return }
                finish()
            })
            .accessibilityLabel("Drawing board, \(count) strokes")
        }
        .frame(minHeight: 280)
    }

    private func finishCamera(_ scale: CGFloat) {
        let x = (metadata?.view.x ?? 0) + Int((cameraTranslation.width / scale).rounded())
        let y = (metadata?.view.y ?? 0) + Int((cameraTranslation.height / scale).rounded())
        cameraTranslation = .zero
        pan(JSONValue.object(["x": .number(Double(x)), "y": .number(Double(y))]).text)
    }

    private func path(_ points: [CGPoint], _ scale: CGFloat, _ offset: CGPoint) -> Path {
        var path = Path()
        guard let first = points.first else { return path }
        path.move(to: CGPoint(x: first.x * scale + offset.x, y: first.y * scale + offset.y))
        points.dropFirst().forEach { path.addLine(to: CGPoint(x: $0.x * scale + offset.x, y: $0.y * scale + offset.y)) }
        if points.count == 1 { path.addLine(to: CGPoint(x: first.x * scale + offset.x + 0.1, y: first.y * scale + offset.y)) }
        return path
    }

    private func finish() {
        let points = live
        live = []
        guard !points.isEmpty else { return }
        if selecting { return lasso(enclosed(by: points)) }
        let clamped = points.map { JSONValue.array([.number(Double(max(0, Int($0.x)))), .number(Double(max(0, Int($0.y))))]) }
        stroke(.object(["points": .array(Array(clamped.prefix(8192))), "width": .number(3), "at": .number(Double(Int(Date().timeIntervalSince1970 * 1000)))]))
    }

    /// The strokes with any point inside the lasso.
    private func enclosed(by loop: [CGPoint]) -> [Int] {
        var outline = Path()
        outline.addLines(loop)
        outline.closeSubpath()
        return strokes.enumerated().compactMap { index, item in
            item.points.contains { outline.contains(CGPoint(x: CGFloat($0[0]), y: CGFloat($0[1]))) } ? index : nil
        }
    }
}

// MARK: Mathematics

/// Lays out Osprey's mathematics tree with the registered STIX Two Math face.
struct MathView: View {
    let tree: MathTree
    var size: CGFloat = 18
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyInk) private var ink

    var body: some View { node(tree).foregroundStyle(theme.color(ink.isEmpty ? "primary" : ink, dark: scheme == .dark)) }

    private func glyph(_ text: String, italic: Bool = false) -> some View { MathGlyph(text: text, size: size, italic: italic) }

    private func smaller(_ tree: MathTree) -> MathView { MathView(tree: tree, size: size * 0.7) }

    private func operatorSpace(_ symbol: String) -> CGFloat {
        let relations = ["=", "≠", "<", ">", "≤", "≥", "→", "↔", "⇒", "⇔", "∈", "⊆"]
        return size * (relations.contains(symbol) ? 5 : 4) / 18
    }

    private func operation(_ symbol: String, _ left: MathTree, _ right: MathTree) -> AnyView {
        if symbol == ":" {
            return AnyView(HStack(alignment: .firstTextBaseline, spacing: 0) { node(left); glyph(symbol).padding(.trailing, size / 6); node(right) })
        }
        return AnyView(HStack(alignment: .firstTextBaseline, spacing: operatorSpace(symbol)) { node(left); glyph(symbol); node(right) })
    }

    private func node(_ tree: MathTree) -> AnyView {
        switch tree {
        case let .sym(text): return AnyView(glyph(text, italic: text.count == 1 && text.first?.isLetter == true))
        case let .num(text): return AnyView(glyph(text))
        case let .seq(items): return AnyView(HStack(alignment: .firstTextBaseline, spacing: 2) { ForEach(Array(items.enumerated()), id: \.offset) { node($0.element) } })
        case let .sup(base, script): return AnyView(HStack(alignment: .top, spacing: 1) { node(base).padding(.top, size * 0.35); smaller(script) })
        case let .sub(base, script): return AnyView(HStack(alignment: .bottom, spacing: 1) { node(base).padding(.bottom, size * 0.25); smaller(script) })
        case let .frac(top, bottom): return AnyView(VStack(spacing: 2) { node(top); Rectangle().frame(height: 1); node(bottom) }.fixedSize())
        case let .root(body): return AnyView(HStack(spacing: 0) { glyph("√"); node(body).overlay(alignment: .top) { Rectangle().frame(height: 1) } })
        case let .binder(symbol, names, body): return AnyView(HStack(alignment: .firstTextBaseline, spacing: 3) { glyph(symbol); ForEach(Array(names.enumerated()), id: \.offset) { node($0.element) }; glyph(","); node(body) })
        case let .op(symbol, left, right): return operation(symbol, left, right)
        case let .paren(body): return AnyView(HStack(alignment: .firstTextBaseline, spacing: 1) { glyph("("); node(body); glyph(")") })
        case let .big(symbol, lower, upper, body):
            return AnyView(HStack(spacing: 4) { VStack(spacing: 0) { smaller(upper); Text(symbol).font(.system(size: size * 1.6)); smaller(lower) }; node(body) })
        }
    }
}

// MARK: Media

struct StillView: View {
    let source: String
    let alt: String
    let width: Int
    let height: Int

    var body: some View {
        if let picture = Bundled.image(source) {
            Image(nsImage: picture).resizable().aspectRatio(CGFloat(width) / CGFloat(max(height, 1)), contentMode: .fit)
                .frame(maxWidth: CGFloat(width)).clipShape(RoundedRectangle(cornerRadius: 8)).accessibilityLabel(alt)
        } else {
            Label("This picture is missing: \(alt)", systemImage: "photo").foregroundStyle(.secondary)
        }
    }
}

/// A film the person starts: its poster and a play control until then, never autoplaying, with its length on the poster.
