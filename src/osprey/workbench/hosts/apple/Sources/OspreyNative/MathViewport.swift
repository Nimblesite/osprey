import SwiftUI

// Implements [UI-COMPONENTS-MATH]: wide formulae remain inspectable inside their pane.
struct MathViewport: View {
    let tree: MathTree
    let size: CGFloat
    let spoken: String
    @Environment(\.ospreyIntrinsicWidth) private var intrinsic

    var body: some View {
        if intrinsic {
            formula
        } else {
            ViewThatFits(in: .horizontal) {
                formula
                ScrollView(.horizontal) { formula }.fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var formula: some View {
        MathView(tree: tree, size: size).fixedSize()
            .accessibilityElement(children: .ignore).accessibilityLabel(spoken)
    }
}

private struct IntrinsicWidthKey: EnvironmentKey { static let defaultValue = false }

extension EnvironmentValues {
    var ospreyIntrinsicWidth: Bool {
        get { self[IntrinsicWidthKey.self] }
        set { self[IntrinsicWidthKey.self] = newValue }
    }
}
