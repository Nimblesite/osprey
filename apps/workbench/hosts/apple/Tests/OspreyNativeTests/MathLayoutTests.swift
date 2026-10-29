import AppKit
import CoreText
import SwiftUI
import XCTest
@testable import OspreyNative

final class MathLayoutTests: XCTestCase {
    private var mathFont: URL { TestResources.assets.appendingPathComponent("fonts/STIXTwoMath-Regular.ttf") }

    override func setUpWithError() throws {
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(mathFont as CFURL, .process, nil))
    }

    override func tearDownWithError() throws {
        XCTAssertTrue(CTFontManagerUnregisterFontsForURL(mathFont as CFURL, .process, nil))
    }

    @MainActor
    func testLongFormalStatementRemainsInsideItsInspectablePane() {
        let source = String(repeating: "formal_term ", count: 40)
        let size = measured(.math(.sym(source), spoken: source))
        XCTAssertLessThanOrEqual(size.width, 300, "A long imported statement must scroll within its pane instead of being clipped outside it")
        XCTAssertLessThan(size.height, 100, "Horizontal inspection must retain the formula’s natural ink height")
    }

    @MainActor
    func testFittingMathematicsKeepsItsNaturalWidthAndHeight() {
        let size = measured(.math(.sym("p"), spoken: "p"))
        XCTAssertGreaterThan(size.width, 0)
        XCTAssertLessThan(size.width, 100)
        XCTAssertLessThan(size.height, 100)
    }

    @MainActor
    func testSuperscriptInkReservesItsRaisedExtentInsideAClippedFrame() {
        let base = NSHostingView(rootView: MathView(tree: .sym("p"), size: 27).fixedSize()).fittingSize
        let raised = NSHostingView(rootView: MathView(tree: .sup(.sym("p"), .num("2")), size: 27).fixedSize()).fittingSize
        XCTAssertGreaterThan(raised.height, base.height + 5, "Raised ink must contribute to the row height so a frame cannot cut it off")
    }

    @MainActor
    func testSubscriptInkReservesItsLoweredExtentInsideAClippedFrame() {
        let base = NSHostingView(rootView: MathView(tree: .sym("p"), size: 27).fixedSize()).fittingSize
        let lowered = NSHostingView(rootView: MathView(tree: .sub(.sym("p"), .num("2")), size: 27).fixedSize()).fittingSize
        XCTAssertGreaterThan(lowered.height, base.height + 5, "Lowered ink must contribute to the row height so a frame cannot cut it off")
    }

    @MainActor
    func testAdjacentFittingHypothesesKeepTheirNaturalWidths() {
        var widths = [CGFloat](repeating: 0, count: 2)
        let view = HStack(spacing: 16) {
            ForEach(0..<2) { index in
                MathViewport(tree: .op(":", .sym("hp"), .sym("p")), size: 27, spoken: "hp: p")
                    .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { widths[index] = $0 }
            }
        }.frame(width: 300, height: 100, alignment: .topLeading)
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        for width in widths {
            XCTAssertGreaterThan(width, 40, "Fitting hypotheses must not collapse into a scrolling sliver")
            XCTAssertLessThan(width, 90, "A fitting formula must not consume the remaining row width")
        }
    }

    @MainActor
    func testLongFormulaBesideALabelRemainsInsideTheRow() {
        var width: CGFloat = 0
        let view = HStack(spacing: 16) {
            Text("Statement")
            MathViewport(tree: .sym(String(repeating: "formal_term ", count: 40)), size: 27, spoken: "Long statement")
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width = $0 }
        }.frame(width: 300, height: 100, alignment: .topLeading)
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        XCTAssertGreaterThan(width, 0)
        XCTAssertLessThan(width, 300, "A formula must share the bounded row with its label")
    }

    @MainActor
    private func measured(_ node: Node) -> CGSize {
        var size = CGSize.zero
        let view = NodeView(node: node, drawing: Drawing(region: "math-layout", revision: 1, send: { _, _ in }))
            .onGeometryChange(for: CGSize.self, of: \.size) { size = $0 }
            .frame(width: 300, height: 100, alignment: .topLeading)
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        return size
    }
}
