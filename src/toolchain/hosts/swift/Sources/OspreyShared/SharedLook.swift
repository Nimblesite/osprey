import SwiftUI
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW]'s styling half: every colour, size, weight, inset,
// space, corner and flag a control wears is read from its row. This file holds
// no colour, font or padding of its own, and a value the row did not send stays
// unset, so the system draws what it naturally would.

/// One row's style, read straight from the shared frame.
struct SharedLook {
    let ink: Color?
    let fill: Color?
    let edge: Color?
    let points: CGFloat
    let inset: CGFloat
    let gap: CGFloat
    let corner: CGFloat
    let weight: Font.Weight
    let slanted: Bool
    let fixedWidth: Bool
    let centred: Bool
    let width: CGFloat
    let height: CGFloat
    let flex: Double
    let disabled: Bool

    init(_ row: OspNativeView) {
        ink = SharedPaint.colour(row.colour)
        fill = SharedPaint.colour(row.background)
        edge = SharedPaint.colour(row.border)
        points = OspNativeView.usable(row.size)
        inset = OspNativeView.usable(row.padding)
        gap = OspNativeView.usable(row.spacing)
        corner = OspNativeView.usable(row.radius)
        weight = SharedPaint.weight(row.weight)
        slanted = row.has(SharedFlag.slanted)
        fixedWidth = row.has(SharedFlag.fixedWidth)
        centred = row.has(SharedFlag.centred)
        width = OspNativeView.usable(row.width)
        height = OspNativeView.usable(row.height)
        flex = OspNativeView.share(row.grow)
        disabled = row.has(SharedFlag.disabled)
    }

    /// The face the row asks for, or the system's own natural size when the row
    /// names none.
    var font: Font {
        let base = points > 0 ? Font.system(size: points, weight: weight) : Font.body
        return fixedWidth ? (slanted ? base.italic().monospaced() : base.monospaced()) : (slanted ? base.italic() : base)
    }

    /// `content` wearing everything the row carries: its padding, fixed extent,
    /// fill, corner, border and flex share.
    func dressed(_ content: AnyView, axis: SharedAxis) -> AnyView {
        grown(painted(extended(AnyView(content.padding(inset)))), axis: axis)
    }

    private func extended(_ content: AnyView) -> AnyView {
        guard width > 0 || height > 0 else { return content }
        return AnyView(content.frame(width: width > 0 ? width : nil, height: height > 0 ? height : nil))
    }

    private func painted(_ content: AnyView) -> AnyView {
        var view = content
        if let fill {
            view = AnyView(view.background(fill, in: RoundedRectangle(cornerRadius: corner)))
        }
        if let edge {
            view = AnyView(view.overlay(RoundedRectangle(cornerRadius: corner).strokeBorder(edge, lineWidth: 1)))
        }
        return view
    }

    private func grown(_ content: AnyView, axis: SharedAxis) -> AnyView {
        guard flex > 0, axis != .none else { return content }
        return AnyView(content
            .frame(maxWidth: axis == .horizontal ? .infinity : nil,
                   maxHeight: axis == .vertical ? .infinity : nil)
            .layoutPriority(flex))
    }
}

/// Colours and weights exactly as the shared frame spells them.
enum SharedPaint {
    /// `0xRRGGBBAA` as a colour; zero is no colour at all.
    static func colour(_ value: Int64) -> Color? {
        guard value != 0 else { return nil }
        let bits = UInt64(bitPattern: value) & 0xFFFF_FFFF
        return Color(.sRGB,
                     red: Double((bits >> 24) & 0xFF) / 255,
                     green: Double((bits >> 16) & 0xFF) / 255,
                     blue: Double((bits >> 8) & 0xFF) / 255,
                     opacity: Double(bits & 0xFF) / 255)
    }

    /// A weight from 100 to 900, rounded to the nearest face the system has;
    /// anything else is its regular.
    static func weight(_ value: Int64) -> Font.Weight {
        let faces: [Font.Weight] = [.ultraLight, .thin, .light, .regular, .medium,
                                    .semibold, .bold, .heavy, .black]
        guard (100...900).contains(value) else { return .regular }
        return faces[Int(min(max((value + 50) / 100 - 1, 0), 8))]
    }
}

extension OspNativeView {
    /// Whether the row carries `flag`.
    func has(_ flag: Int64) -> Bool { flags & flag != 0 }

    /// A length the host can lay out: a value that is not finite, or one below
    /// zero, is zero [NATIVE-SHM-SAFETY].
    static func usable(_ value: Double) -> CGFloat {
        value.isFinite && value > 0 ? CGFloat(value) : 0
    }

    /// A flex share the host can lay out, held to the same rule as `usable`.
    static func share(_ value: Double) -> Double {
        value.isFinite && value > 0 ? value : 0
    }
}
