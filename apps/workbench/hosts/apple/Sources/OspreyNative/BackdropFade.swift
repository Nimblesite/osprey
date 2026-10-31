import SwiftUI

// Implements [UI-COMPONENTS-FRAMED]: intersect the reference's horizontal
// and vertical edge fades while keeping its broad opaque chalk area.
struct BackdropFade: ViewModifier {
    let enabled: Bool
    private static let horizontal = Gradient(stops: [.init(color: .clear, location: 0), .init(color: .black, location: 0.05), .init(color: .black, location: 0.6), .init(color: .clear, location: 1)])
    private static let vertical = Gradient(stops: [.init(color: .clear, location: 0), .init(color: .black, location: 0.05), .init(color: .black, location: 0.86), .init(color: .clear, location: 1)])

    @ViewBuilder func body(content: Content) -> some View {
        if enabled {
            content.mask(LinearGradient(gradient: Self.horizontal, startPoint: .leading, endPoint: .trailing))
                .mask(LinearGradient(gradient: Self.vertical, startPoint: .top, endPoint: .bottom))
                .blendMode(.lighten)
        } else { content }
    }
}
