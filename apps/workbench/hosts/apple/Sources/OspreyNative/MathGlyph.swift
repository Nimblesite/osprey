import CoreText
import SwiftUI

// Implements [UI-COMPONENTS-MATH]: mathematical rows align ink on a shared
// baseline rather than reserving the surrounding prose font's line box.
struct MathGlyph: View {
    let text: String
    let size: CGFloat
    var italic = false

    private var rendered: String { italic ? MathematicalAlphabet.italic(text) : text }

    private var line: CTLine {
        let face = CTFontCreateWithName("STIXTwoMath-Regular" as CFString, size, nil)
        let value = NSAttributedString(string: rendered, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): face])
        return CTLineCreateWithAttributedString(value)
    }

    var body: some View {
        let measured = line
        let bounds = CTLineGetBoundsWithOptions(measured, .useGlyphPathBounds)
        MathematicalInkLayout(advance: CGFloat(CTLineGetTypographicBounds(measured, nil, nil, nil)), ascent: max(0, bounds.maxY), descent: max(0, -bounds.minY)) {
            Text(rendered).font(.custom("STIXTwoMath-Regular", size: size))
        }
    }
}

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
