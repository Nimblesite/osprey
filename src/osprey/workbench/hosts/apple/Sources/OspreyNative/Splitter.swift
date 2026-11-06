import SwiftUI

// Implements [UI-COMPONENTS-SPLITTER] and [DESIGN-COMPONENTS]: the splitter
// between an aside's pane and its column. Osprey owns the width and its
// limits; while a drag is in progress the host previews the width it would
// send, and the drag's end delivers that width to the splitter's binding.

extension Splitter {
    /// Points a keyboard or assistive adjustment moves the splitter.
    static let step: CGFloat = 20

    /// The column's width once its splitter moves `offset` points from `start`, within the limits Osprey sent and
    /// leaving the pane `keep` points of the `total` both columns share.
    func preview(from start: CGFloat, dragged offset: CGFloat, side: Side, total: CGFloat) -> CGFloat {
        let wanted = side == .trailing ? start - offset : start + offset
        let widest = max(CGFloat(least), min(CGFloat(most), total - CGFloat(keep)))
        return min(max(wanted, CGFloat(least)), widest)
    }
}

#if os(macOS)
/// The draggable splitter, as wide as the theme's gap between columns: a line on the theme's `line` colour,
/// the accent while hovered or dragged.
struct SplitterHandle: View {
    let splitter: Splitter
    let side: Side
    let total: CGFloat
    @Binding var dragging: CGFloat?
    let commit: (CGFloat) -> Void
    @State private var start: CGFloat?
    @State private var hovering = false
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        Color.clear.frame(width: 1).frame(maxHeight: .infinity)
            .overlay { Rectangle().fill(line).frame(width: active ? 2 : 1) }
            .contentShape(Rectangle().inset(by: -4))
            .onContinuousHover(perform: hover)
            .gesture(drag)
            .accessibilityElement()
            .accessibilityLabel("Column splitter")
            .accessibilityValue("\(splitter.width) points")
            .accessibilityAdjustableAction(adjust)
    }

    private var active: Bool { hovering || start != nil }

    private var line: Color { theme.color(active ? "accent" : "divider", dark: scheme == .dark) }

    // Translation is global so the splitter's own movement does not feed back into the drag.
    private var drag: some Gesture {
        DragGesture(minimumDistance: 1, coordinateSpace: .global)
            .onChanged { change in
                let base = start ?? CGFloat(splitter.width)
                start = base
                dragging = splitter.preview(from: base, dragged: change.translation.width, side: side, total: total)
            }
            .onEnded { _ in
                start = nil
                if let dragging { commit(dragging) }
            }
    }

    private func hover(_ phase: HoverPhase) {
        hovering = phase != .ended
        (hovering ? NSCursor.resizeLeftRight : NSCursor.arrow).set()
    }

    private func adjust(_ direction: AccessibilityAdjustmentDirection) {
        let offset = direction == .increment ? Splitter.step : -Splitter.step
        commit(splitter.preview(from: CGFloat(splitter.width), dragged: side == .trailing ? -offset : offset, side: side, total: total))
    }
}
#endif
