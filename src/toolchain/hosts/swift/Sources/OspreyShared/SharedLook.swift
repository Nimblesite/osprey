import SwiftUI

// Implements [NATIVE-SHM-VIEW]'s styling half: every colour, size, weight, inset, space, corner
// and flag a control wears is read from its checked row. This file holds no colour, font or
// padding of its own, and a value the row did not send stays unset, so the system draws what it
// naturally would.

/// One control's style, read straight from the checked frame.
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

    init(_ control: RowControl) {
        let look = control.look, fit = control.fit
        ink = SharedPaint.colour(look.ink)
        fill = SharedPaint.colour(look.fill)
        edge = SharedPaint.colour(fit.border)
        points = Self.usable(look.size)
        inset = Self.usable(look.padding)
        gap = Self.usable(look.spacing)
        corner = Self.usable(look.radius)
        weight = SharedPaint.weight(look.weight)
        slanted = fit.flags & SharedFlag.slanted != 0
        fixedWidth = fit.flags & SharedFlag.fixedWidth != 0
        centred = fit.flags & SharedFlag.centred != 0
        width = Self.usable(fit.width)
        height = Self.usable(fit.height)
        flex = Self.share(fit.grow)
        disabled = fit.flags & SharedFlag.disabled != 0
    }

    /// The face the row asks for, or the system's own natural size when the row names none.
    var font: Font {
        let base = points > 0 ? Font.system(size: points, weight: weight) : Font.body
        return fixedWidth ? (slanted ? base.italic().monospaced() : base.monospaced()) : (slanted ? base.italic() : base)
    }

    /// `content` wearing everything the row carries: its padding, fixed extent, fill, corner,
    /// border and flex share.
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

    /// A length the host can lay out: a value that is not finite, or one below zero, is zero
    /// ([NATIVE-SHM-SAFETY]; the checker refuses such a frame, and this holds for the rest).
    static func usable(_ value: Double) -> CGFloat {
        value.isFinite && value > 0 ? CGFloat(value) : 0
    }

    /// A flex share the host can lay out, held to the same rule as `usable`.
    static func share(_ value: Double) -> Double {
        value.isFinite && value > 0 ? value : 0
    }
}

/// Colours and weights exactly as the shared frame spells them.
enum SharedPaint {
    /// `0xRRGGBBAA` as a colour; zero is no colour at all.
    static func colour(_ value: UInt32) -> Color? {
        guard value != 0 else { return nil }
        return Color(.sRGB,
                     red: Double((value >> 24) & 0xFF) / 255,
                     green: Double((value >> 16) & 0xFF) / 255,
                     blue: Double((value >> 8) & 0xFF) / 255,
                     opacity: Double(value & 0xFF) / 255)
    }

    /// A weight from 100 to 900, rounded to the nearest face the system has.
    static func weight(_ value: Int64) -> Font.Weight {
        let faces: [Font.Weight] = [.ultraLight, .thin, .light, .regular, .medium,
                                    .semibold, .bold, .heavy, .black]
        guard (100...900).contains(value) else { return .regular }
        return faces[Int(min(max((value + 50) / 100 - 1, 0), 8))]
    }
}
