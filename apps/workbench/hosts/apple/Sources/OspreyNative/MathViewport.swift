import SwiftUI

// Implements [UI-COMPONENTS-MATH]: wide formulae remain inspectable inside their pane.
struct MathViewport: View {
    let tree: MathTree
    let size: CGFloat
    let spoken: String

    var body: some View {
        ViewThatFits(in: .horizontal) {
            formula
            ScrollView(.horizontal) { formula }.fixedSize(horizontal: false, vertical: true)
        }
    }

    private var formula: some View {
        MathView(tree: tree, size: size).fixedSize()
            .accessibilityElement(children: .ignore).accessibilityLabel(spoken)
    }
}
