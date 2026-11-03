#if os(macOS)
import AppKit
import SwiftUI

// Implements [UI-COMPONENTS-FRAMED]: a frame that sets `selectionGroup` joins the
// text leaves inside it into one selection, so a person drags across paragraphs,
// lists, code blocks, tables and bubbles, copies one span and selects all, as a
// chat app allows. The frame owns the group; each leaf draws through its own
// NSTextView with the same face, ink and spacing as its SwiftUI twin, and the
// group paints TextKit 1 temporary attributes, so every member looks selected
// together regardless of which view has focus.

/// The members of one selection group and the span painted in each.
@MainActor
final class SelectionGroup {
    private var members: [SelectableTextView] = []
    private var anchor: (view: SelectableTextView, index: Int)?
    private(set) var painted: [ObjectIdentifier: NSRange] = [:]

    /// Where `copy` lands; a self-test points this at a named pasteboard so a run never
    /// touches the person's clipboard. Nil copies to the general pasteboard.
    static var pasteboard: NSPasteboard?

    func register(_ view: SelectableTextView) {
        guard !members.contains(where: { $0 === view }) else { return }
        members.append(view)
    }

    func forget(_ view: SelectableTextView) {
        members.removeAll { $0 === view }
        painted.removeValue(forKey: ObjectIdentifier(view))
    }

    /// The members as the person reads them: top to bottom, then left to right.
    var ordered: [SelectableTextView] {
        members.compactMap { view -> (view: SelectableTextView, at: NSPoint)? in
            guard view.window != nil else { return nil }
            return (view, view.convert(.zero, to: nil))
        }.sorted { first, second in
            abs(first.at.y - second.at.y) > 1 ? first.at.y > second.at.y : first.at.x < second.at.x
        }.map(\.view)
    }

    /// A drag begins at the character under the pointer; whatever was selected is gone.
    func begin(_ view: SelectableTextView, index: Int) {
        anchor = (view, index)
        paint([:])
    }

    /// The selection runs from the anchor to the pointer, across every member between them.
    func drag(to view: SelectableTextView, index: Int) {
        guard let (from, start) = anchor else { return }
        guard let first = ordered.firstIndex(where: { $0 === from }), let last = ordered.firstIndex(where: { $0 === view }) else { return }
        var spans: [ObjectIdentifier: NSRange] = [:]
        if from === view {
            spans[ObjectIdentifier(view)] = NSRange(location: min(start, index), length: abs(index - start))
        } else {
            // However the drag runs, the upper member selects from its pointer to its end and
            // the lower one from its start to its pointer, with everything between whole.
            let downward = first < last
            let upper = downward ? from : view
            let lower = downward ? view : from
            let upperIndex = downward ? start : index
            let lowerIndex = downward ? index : start
            spans[ObjectIdentifier(upper)] = NSRange(location: upperIndex, length: max(0, upper.length - upperIndex))
            spans[ObjectIdentifier(lower)] = NSRange(location: 0, length: min(lowerIndex, lower.length))
            let between = downward ? (first + 1) ..< last : (last + 1) ..< first
            between.map { ordered[$0] }.forEach { spans[ObjectIdentifier($0)] = $0.wholeRange }
        }
        paint(spans)
    }

    /// A double or triple click keeps to one member, as a text view selects a word or a line.
    func selectLocal(_ view: SelectableTextView, _ range: NSRange) {
        anchor = nil
        paint([ObjectIdentifier(view): range])
    }

    /// Cmd-A: every member, in reading order.
    func selectAll() {
        anchor = nil
        paint(Dictionary(uniqueKeysWithValues: ordered.map { (ObjectIdentifier($0), $0.wholeRange) }))
    }

    /// The member the pointer is over, or the nearest one vertically, with the character it points at.
    func target(at windowPoint: NSPoint) -> (view: SelectableTextView, index: Int)? {
        let shown = ordered
        for view in shown {
            let local = view.convert(windowPoint, from: nil)
            if view.bounds.contains(local) { return (view, view.insertion(at: local)) }
        }
        // Between blocks the nearest member still bounds the drag: its first character from
        // above it, its last from below.
        guard let nearest = shown.min(by: { gap($0, windowPoint) < gap($1, windowPoint) }) else { return nil }
        let rect = nearest.convert(nearest.bounds, to: nil)
        return (nearest, windowPoint.y > rect.maxY ? 0 : nearest.length)
    }

    /// How far a member sits from the pointer, vertically first.
    private func gap(_ view: SelectableTextView, _ point: NSPoint) -> CGFloat {
        let rect = view.convert(view.bounds, to: nil)
        return max(rect.minY - point.y, point.y - rect.maxY, 0) * 2 + max(rect.minX - point.x, point.x - rect.maxX, 0)
    }

    /// The selection as one span of words: a member's as they are, members a line apart or
    /// more joined by a blank line, otherwise a newline.
    var selectedText: String {
        let pieces = ordered.compactMap { view -> (text: String, rect: NSRect, line: CGFloat)? in
            guard let range = painted[ObjectIdentifier(view)], range.length > 0, range.upperBound <= view.length else { return nil }
            return ((view.string as NSString).substring(with: range), view.rect(of: range), view.lineHeight)
        }
        return pieces.indices.dropFirst().reduce(pieces.first?.text ?? "") { joined, index in
            joined + (pieces[index - 1].rect.minY - pieces[index].rect.maxY > pieces[index - 1].line ? "\n\n" : "\n") + pieces[index].text
        }
    }

    /// The words a copy takes, on the pasteboard a self-test may have named.
    func copySelection() {
        let text = selectedText
        guard !text.isEmpty else { return }
        let board = Self.pasteboard ?? .general
        board.prepareForNewContents()
        board.setString(text, forType: .string)
    }

    /// The member's words changed, so its painted span is clamped to what it still holds.
    func textChanged(in view: SelectableTextView) {
        guard var range = painted[ObjectIdentifier(view)] else { return }
        range.length = min(range.length, max(0, view.length - range.location))
        painted[ObjectIdentifier(view)] = range.length > 0 ? range : nil
        view.paint(range.length > 0 ? range : nil)
    }

    private func paint(_ spans: [ObjectIdentifier: NSRange]) {
        members.forEach { $0.paint(spans[ObjectIdentifier($0)]) }
        painted = spans.filter { $0.value.length > 0 }
    }
}

/// One text leaf of a selection group: non-editable, without background or insets,
/// wrapping to the width SwiftUI proposes and reporting the height TextKit needs.
final class SelectableTextView: NSTextView {
    var group: SelectionGroup?

    var length: Int { (string as NSString).length }
    var wholeRange: NSRange { NSRange(location: 0, length: length) }

    /// One drawn line's height, for joining blocks that sit further apart than a line.
    var lineHeight: CGFloat {
        guard let manager = layoutManager, length > 0 else { return font?.boundingRectForFont.height ?? 13 }
        return manager.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil).height
    }

    func insertion(at local: NSPoint) -> Int { characterIndexForInsertion(at: local) }

    /// The selection's paint: temporary attributes, so all members look selected together.
    func paint(_ range: NSRange?) {
        guard let manager = layoutManager else { return }
        manager.setTemporaryAttributes([:], forCharacterRange: wholeRange)
        if let range, range.length > 0 {
            manager.addTemporaryAttribute(.backgroundColor, value: NSColor.selectedContentBackgroundColor, forCharacterRange: range)
        }
        needsDisplay = true
    }

    /// A range of characters' glyphs in this view's own coordinates.
    func glyphRect(_ range: NSRange) -> NSRect {
        guard let manager = layoutManager, let container = textContainer else { return .zero }
        let glyphs = manager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        return manager.boundingRect(forGlyphRange: glyphs, in: container)
    }

    /// A range of characters' place in the window, for joining one member to the next.
    func rect(of range: NSRange) -> NSRect { convert(glyphRect(range), to: nil) }

    /// New words, keeping the group's painted span within them.
    func setAttributed(_ text: NSAttributedString) {
        textStorage?.setAttributedString(text)
        group?.textChanged(in: self)
    }

    /// The words wrap at whatever width the SwiftUI twin beneath was laid out to.
    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        textContainer?.size = NSSize(width: newSize.width + 1, height: CGFloat.greatestFiniteMagnitude)
    }

    // The transcript's first click selects where it lands, even in a window that was not key.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let index = insertion(at: convert(event.locationInWindow, from: nil))
        let words = string as NSString
        switch event.clickCount {
        case 2: group?.selectLocal(self, words.wordRange(at: index))
        case 3...: group?.selectLocal(self, words.lineRange(at: index))
        default: group?.begin(self, index: index)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        enclosingScrollView?.autoscroll(with: event)
        guard let target = group?.target(at: event.locationInWindow) else { return }
        group?.drag(to: target.view, index: target.index)
    }

    // The wheel belongs to the frame's scroll view, not to one leaf.
    override func scrollWheel(with event: NSEvent) {
        if let scroll = enclosingScrollView { scroll.scrollWheel(with: event) } else { super.scrollWheel(with: event) }
    }

    override func copy(_ sender: Any?) { group?.copySelection() }
    override func selectAll(_ sender: Any?) { group?.selectAll() }

    /// One item, through the responder chain, as the context menu offers.
    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        menu.addItem(withTitle: "Copy", action: #selector(copy(_:)), keyEquivalent: "")
        return menu
    }
}

extension SelectableTextView {
    /// A non-editable, inset-free view over its own TextKit 1 stack, where temporary
    /// attributes paint selections.
    convenience init(attributed: NSAttributedString) {
        let storage = NSTextStorage(attributedString: attributed)
        let manager = NSLayoutManager()
        storage.addLayoutManager(manager)
        let container = NSTextContainer(size: NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        manager.addTextContainer(container)
        self.init(frame: .zero, textContainer: container)
        isEditable = false
        isSelectable = true
        isRichText = false
        drawsBackground = false
        textContainerInset = .zero
    }

    /// A point just before or after a word's glyphs in window coordinates, where a drag to
    /// it begins or ends; nil when this view does not hold the word.
    func point(_ words: String, after: Bool) -> NSPoint? {
        let range = (string as NSString).range(of: words)
        guard range.location != NSNotFound, range.length > 0 else { return nil }
        var rect = glyphRect(range)
        if rect.width < 2 { rect.size.width = 2; rect.origin.x -= 1 }
        let x = min(max(after ? rect.maxX + 1 : rect.minX - 1, 0), bounds.width)
        return convert(NSPoint(x: x, y: rect.midY), to: nil)
    }

    /// Whether this view holds the words.
    func holds(_ words: String) -> Bool { (string as NSString).range(of: words).location != NSNotFound }
}

extension NSString {
    /// The word a double click selects at `index`.
    func wordRange(at index: Int) -> NSRange {
        let word = CharacterSet(charactersIn: "_").union(.alphanumerics)
        func inside(_ character: unichar) -> Bool { Unicode.Scalar(character).map(word.contains) ?? false }
        var start = index
        while start > 0, inside(character(at: start - 1)) { start -= 1 }
        var end = index
        while end < length, inside(character(at: end)) { end += 1 }
        return NSRange(location: start, length: end - start)
    }

    /// The line a triple click selects at `index`.
    func lineRange(at index: Int) -> NSRange {
        lineRange(for: NSRange(location: min(max(index, 0), length), length: 0))
    }
}

/// One leaf's words through AppKit inside a selection group, laid over its hidden SwiftUI
/// twin, which keeps doing the layout: a screen does not move when its frame opts in, and
/// only the selection differs. The words are reset only when they changed, so a selection
/// survives re-renders.
struct SelectableText: NSViewRepresentable {
    let attributed: NSAttributedString
    let group: SelectionGroup

    func makeNSView(context: Context) -> SelectableTextView {
        let view = SelectableTextView(attributed: attributed)
        view.group = group
        group.register(view)
        return view
    }

    func updateNSView(_ view: SelectableTextView, context: Context) {
        view.group = group
        if view.attributedString() != attributed { view.setAttributed(attributed) }
    }

    static func dismantleNSView(_ view: SelectableTextView, coordinator: ()) {
        view.group?.forget(view)
    }
}

/// The frame's resolved ink, offered to the leaves a selection group draws.
struct FrameInk: ViewModifier {
    let colour: NSColor?

    func body(content: Content) -> some View { content.environment(\.ospreyInk, colour) }
}

private struct SelectionGroupKey: EnvironmentKey { static let defaultValue: SelectionGroup? = nil }
private struct FrameInkKey: EnvironmentKey { static let defaultValue: NSColor? = nil }

extension EnvironmentValues {
    /// The selection group the nearest `selectionGroup` frame opened.
    var ospreySelectionGroup: SelectionGroup? {
        get { self[SelectionGroupKey.self] }
        set { self[SelectionGroupKey.self] = newValue }
    }
    /// The ink the nearest frame resolved, for leaves AppKit draws.
    var ospreyInk: NSColor? {
        get { self[FrameInkKey.self] }
        set { self[FrameInkKey.self] = newValue }
    }
}
#endif
