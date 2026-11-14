import CoreText
import SwiftUI

// Implements [UI-COMPONENTS-MATH]: one record per mathematics node, told as the kinds `sym`,
// `num`, `seq`, `sup`, `sub`, `frac`, `root`, `binder`, `op`, `paren` and `big`. Glyphs align
// ink on a shared baseline in the registered mathematics face, and wide formulae stay
// inspectable inside their pane.

/// Lays out the told mathematics tree with the registered STIX Two Math face.
struct MathView: View {
    let node: Tree
    var size: CGFloat

    var body: some View { laid(node) }

    private func glyph(_ text: String, italic: Bool = false) -> some View { MathGlyph(text: text, size: size, italic: italic) }

    private func smaller(_ node: Tree) -> MathView { MathView(node: node, size: size * 0.7) }

    private func operatorSpace(_ symbol: String) -> CGFloat {
        let relations = ["=", "≠", "<", ">", "≤", "≥", "→", "↔", "⇒", "⇔", "∈", "⊆"]
        return size * (relations.contains(symbol) ? 5 : 4) / 18
    }

    private func operation(_ symbol: String, _ left: Tree, _ right: Tree) -> AnyView {
        if symbol == ":" {
            return AnyView(HStack(alignment: .firstTextBaseline, spacing: 0) { laid(left); glyph(symbol).padding(.trailing, size / 6); laid(right) })
        }
        return AnyView(HStack(alignment: .firstTextBaseline, spacing: operatorSpace(symbol)) { laid(left); glyph(symbol); laid(right) })
    }

    private func laid(_ node: Tree) -> AnyView {
        switch node.text("kind") {
        case "sym": return AnyView(glyph(node.text("text"), italic: node.text("text").count == 1 && node.text("text").first?.isLetter == true))
        case "num": return AnyView(glyph(node.text("text")))
        case "seq": return AnyView(HStack(alignment: .firstTextBaseline, spacing: 2) { ForEach(Array(node.list("items").enumerated()), id: \.offset) { _, item in laid(item) } })
        case "sup": return AnyView(HStack(alignment: .top, spacing: 1) { laid(node["base"] ?? .null).padding(.top, size * 0.35); smaller(node["script"] ?? .null) })
        case "sub": return AnyView(HStack(alignment: .bottom, spacing: 1) { laid(node["base"] ?? .null).padding(.bottom, size * 0.25); smaller(node["script"] ?? .null) })
        case "frac": return AnyView(VStack(spacing: 2) { laid(node["top"] ?? .null); Rectangle().frame(height: 1); laid(node["bottom"] ?? .null) }.fixedSize())
        case "root": return AnyView(HStack(spacing: 0) { glyph("√"); laid(node["body"] ?? .null).overlay(alignment: .top) { Rectangle().frame(height: 1) } })
        case "binder": return AnyView(HStack(alignment: .firstTextBaseline, spacing: 3) {
            glyph(node.text("symbol"))
            ForEach(Array(node.list("names").enumerated()), id: \.offset) { _, name in laid(name) }
            glyph(",")
            laid(node["body"] ?? .null)
        })
        case "op": return operation(node.text("symbol"), node["left"] ?? .null, node["right"] ?? .null)
        case "paren": return AnyView(HStack(alignment: .firstTextBaseline, spacing: 1) { glyph("("); laid(node["body"] ?? .null); glyph(")") })
        default: return big
        }
    }

    private var big: AnyView {
        AnyView(HStack(spacing: 4) {
            VStack(spacing: 0) { smaller(node["upper"] ?? .null); Text(node.text("symbol")).font(.system(size: size * 1.6)); smaller(node["lower"] ?? .null) }
            laid(node["body"] ?? .null)
        })
    }
}

/// A mathematics glyph: ink on a shared baseline rather than the prose font's line box.
struct MathGlyph: View {
    let text: String
    let size: CGFloat
    var italic = false

    private var rendered: String { italic ? MathematicalAlphabet.italic(text) : text }

    private var line: CTLine {
        let face = CTFontCreateWithName(FontRegistration.mathName as CFString, size, nil)
        let value = NSAttributedString(string: rendered, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): face])
        return CTLineCreateWithAttributedString(value)
    }

    var body: some View {
        let measured = line
        let bounds = CTLineGetBoundsWithOptions(measured, .useGlyphPathBounds)
        MathematicalInkLayout(advance: CGFloat(CTLineGetTypographicBounds(measured, nil, nil, nil)), ascent: max(0, bounds.maxY), descent: max(0, -bounds.minY)) {
            Text(rendered).font(.custom(FontRegistration.mathName, size: size))
        }
    }
}

/// The italic mathematical alphabet a single-letter name is set in.
private enum MathematicalAlphabet {
    static func italic(_ text: String) -> String { text.unicodeScalars.map { String(variable($0)) }.joined() }

    private static func variable(_ scalar: UnicodeScalar) -> UnicodeScalar {
        let value: UInt32
        switch scalar.value {
        case 65...90: value = 0x1D434 + scalar.value - 65
        case 97...122: value = scalar.value == 104 ? 0x210E : 0x1D44E + scalar.value - 97
        case 0x391...0x3A9: value = 0x1D6E2 + scalar.value - 0x391
        case 0x3B1...0x3C9: value = 0x1D6FC + scalar.value - 0x3B1
        default: return scalar
        }
        return UnicodeScalar(value) ?? scalar
    }
}

/// Places a glyph's view so its first baseline sits at the measured ink ascent.
private struct MathematicalInkLayout: Layout {
    let advance: CGFloat
    let ascent: CGFloat
    let descent: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        CGSize(width: advance, height: ascent + descent)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for view in subviews {
            let metrics = view.dimensions(in: .unspecified)
            view.place(at: CGPoint(x: bounds.minX, y: bounds.minY + ascent - metrics[.firstTextBaseline]), proposal: .unspecified)
        }
    }

    func explicitAlignment(of guide: VerticalAlignment, in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGFloat? {
        guide == .firstTextBaseline ? ascent : nil
    }
}

/// A whole formula: fixed at its natural size, scrolling horizontally when its pane is narrower.
struct MathLeaf: View {
    let node: Tree
    @Environment(\.ospreyInherited) private var inherited

    private var size: CGFloat { (inherited.size > 0 ? inherited.size : 26) * (inherited.mathScale > 0 ? inherited.mathScale : 1) }

    var body: some View {
        // A frame that hugs keeps the formula's natural width, never trading it for a scroll.
        if inherited.hug {
            formula
        } else {
            ViewThatFits(in: .horizontal) {
                formula
                ScrollView(.horizontal) { formula }.fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var formula: some View {
        MathView(node: node["math"] ?? .null, size: size).fixedSize()
            .accessibilityElement(children: .ignore).accessibilityLabel(node.text("spoken"))
    }
}
