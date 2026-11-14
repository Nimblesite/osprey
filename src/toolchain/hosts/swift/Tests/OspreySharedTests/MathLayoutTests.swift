import AppKit
import CoreText
import SwiftUI
import XCTest

@testable import OspreyShared

// Mathematics in the told tree: glyphs align on a shared baseline in the registered
// mathematics face, wide formulae stay inspectable inside their pane, and native controls
// follow the application's canvas and ink rather than the system appearance
// ([UI-COMPONENTS-MATH]).

final class MathLayoutTests: RenderingTestCase {
    override func setUpWithError() throws {
        try Repo.registerFonts(["STIXTwoMath-Regular.ttf", "STIXTwoText.ttf", "STIXTwoText-Italic.ttf", "Geist.ttf"])
    }

    /// A themed subtree draws with the application's canvas colour, whatever the system's
    /// appearance is when it renders.
    @MainActor
    func testThemedNativeControlsUseTheCanvasAppearanceInsteadOfTheSystemAppearance() throws {
        for (canvas, system) in [("#FAFAF7", ColorScheme.dark), ("#202723", .light)] {
            let theme = Tree.object(["tokens": .array([.object(["name": .string("background"), "light": .string(canvas), "dark": .string(canvas)])]),
                                     "spacing": .array([.number(8)]), "feedback": .number(0), "settle": .number(0)])
            let picker = Tree.object(["kind": .string("picker"), "title": .string("Theorem"), "selected": .string("proof"),
                                      "choices": .array([.object(["key": .string("proof"), "label": .string("Checked theorem")])]), "event": .number(0)])
            let node = Tree.object(["kind": .string("themed"), "theme": theme, "body": picker])
            let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
                .environment(\.colorScheme, system).frame(width: 430, height: 60), width: 430, height: 60)
            let pixel = try TestResources.pixel(try TestResources.raster(host), 400, 30)
            XCTAssertEqual(pixel.redComponent, Double(Int(canvas.dropFirst().prefix(2), radix: 16)!) / 255, accuracy: 0.02,
                           "Native arrows and menus must contrast with the application's canvas")
            window.close()
        }
    }

    /// Paper controls keep visible dark title ink even when the system is dark.
    @MainActor
    func testPaperPickerUsesApplicationInkEvenWhenTheSystemIsDark() throws {
        let node = Tree.object(["kind": .string("picker"), "title": .string("Theorem"), "selected": .string("proof"),
                                "choices": .array([.object(["key": .string("proof"), "label": .string("Checked theorem")])]), "event": .number(0)])
        let (host, window) = TestResources.hosted(PickerView(node: node) { _ in }
            .environment(\.ospreyTheme, Fixtures.resolver([("secondary", "#242923")]))
            .environment(\.colorScheme, .dark)
            .foregroundStyle(Color(red: 36 / 255, green: 41 / 255, blue: 35 / 255))
            .frame(width: 430, height: 60).background(Color.white), width: 430, height: 60)
        let bitmap = try TestResources.backingRaster(host)
        let ink = (0..<bitmap.pixelsHigh).reduce(0) { count, y in
            count + (0..<bitmap.pixelsWide - 36).filter { x in
                guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return max(colour.redComponent, colour.greenComponent, colour.blueComponent) < 0.3
            }.count
        }
        XCTAssertGreaterThan(ink, 50, "Paper controls need visible dark title ink even when the system is dark")
        window.close()
    }

    /// A picker with a very long selected option keeps itself inside the pane it was given.
    @MainActor
    func testLongPickerOptionsRemainInsideTheMathematicalPane() throws {
        let name = String(repeating: "Namespace.Nested.", count: 15) + "theorem"
        let node = Tree.object(["kind": .string("picker"), "title": .string("Theorem"), "selected": .string("proof"),
                                "choices": .array([.object(["key": .string("proof"), "label": .string(name)])]), "event": .number(0)])
        for width in [CGFloat(160), 300] {
            let size = TestResources.measured(PickerView(node: node) { _ in }, width: width, height: 60)
            XCTAssertLessThanOrEqual(size.width, width, "The selected theorem must not push its control past the pane")
            let row = TestResources.measured(HStack(spacing: 16) { Text("Statement"); PickerView(node: node) { _ in } }
                .frame(width: width, height: 60, alignment: .topLeading), width: width, height: 60)
            XCTAssertEqual(row.width, width, "the pane keeps its width beside the picker")
        }
    }

    /// A long imported name and a wide formula beside a board must not paint across the board.
    @MainActor
    func testImportedNamesAndMathematicsDoNotPaintAcrossTheAdjacentBoard() throws {
        let name = String(repeating: "Namespace.Nested.", count: 15) + "theorem"
        let items: [Tree] = [
            .object(["kind": .string("section"), "title": .string("Proof library"), "footer": .string("Inspect checked mathematics."),
                     "children": .array([Fixtures.text("Open proof library")])]),
            Tree.object(["kind": .string("picker"), "title": .string("Theorem"), "selected": .string("proof"),
                         "choices": .array([.object(["key": .string("proof"), "label": .string(name)])]), "event": .number(0)]),
            Fixtures.text(name, "headline", "one"),
            Fixtures.math(Fixtures.mathSym(name), spoken: name),
        ]
        let node = Tree.object(["kind": .string("aside"),
                                "pane": Fixtures.framed(["grow": .bool(true), "fill": "board"], Fixtures.text("")),
                                "side": .string("trailing"),
                                "aside": .object(["kind": .string("stack"), "axis": .string("vertical"), "children": .array(items)]),
                                "width": .number(430), "least": .number(160), "most": .number(640), "event": .number(1)])
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("board", "#00FF00"), ("one", "#000000")]))
            .frame(width: 1000, height: 500), width: 1000, height: 500)
        let board = try TestResources.pixel(try TestResources.raster(host), 540, 40)
        XCTAssertLessThan(board.redComponent, 0.1, "A long theorem must not push the library card across the board")
        XCTAssertGreaterThan(board.greenComponent, 0.9)
        window.close()
    }

    /// A long formal statement stays inside its inspectable pane at its natural ink height.
    @MainActor
    func testLongFormalStatementRemainsInsideItsInspectablePane() {
        let source = String(repeating: "formal_term ", count: 40)
        let size = TestResources.measured(mathLeaf(source, hug: false), width: 300, height: 100)
        XCTAssertLessThanOrEqual(size.width, 300, "A long imported statement must scroll within its pane instead of being clipped outside it")
        XCTAssertLessThan(size.height, 100, "Horizontal inspection must retain the formula’s natural ink height")
    }

    /// A fitting formula keeps its natural width and height.
    @MainActor
    func testFittingMathematicsKeepsItsNaturalWidthAndHeight() {
        let size = TestResources.measured(mathLeaf("p", hug: true), width: 300, height: 100)
        XCTAssertGreaterThan(size.width, 0)
        XCTAssertLessThan(size.width, 100)
        XCTAssertLessThan(size.height, 100)
    }

    /// Raised ink contributes to a row's height so a frame cannot cut it off.
    @MainActor
    func testSuperscriptInkReservesItsRaisedExtentInsideAClippedFrame() {
        let raised = NSHostingView(rootView: mathView(mathOp("sup", script: "2")).fixedSize())
        raised.layoutSubtreeIfNeeded()
        XCTAssertGreaterThan(raised.fittingSize.height, baseFitting().height + 5, "Raised ink must contribute to the row height so a frame cannot cut it off")
    }

    /// Lowered ink contributes to a row's height so a frame cannot cut it off.
    @MainActor
    func testSubscriptInkReservesItsLoweredExtentInsideAClippedFrame() {
        let lowered = NSHostingView(rootView: mathView(mathOp("sub", script: "2")).fixedSize())
        lowered.layoutSubtreeIfNeeded()
        XCTAssertGreaterThan(lowered.fittingSize.height, baseFitting().height + 5, "Lowered ink must contribute to the row height so a frame cannot cut it off")
    }

    /// Two fitting hypotheses beside each other keep their natural widths.
    @MainActor
    func testAdjacentFittingHypothesesKeepTheirNaturalWidths() {
        var widths = [CGFloat](repeating: 0, count: 2)
        let view = HStack(spacing: 16) {
            ForEach(0..<2) { index in
                self.mathLeaf("hp: p", hug: true)
                    .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { widths[index] = $0 }
            }
        }.frame(width: 300, height: 100, alignment: .topLeading)
        TestResources.measured(view, width: 300, height: 100)
        for width in widths {
            XCTAssertGreaterThan(width, 40, "Fitting hypotheses must not collapse into a scrolling sliver")
            XCTAssertLessThan(width, 90, "A fitting formula must not consume the remaining row width")
        }
    }

    /// A long formula beside a label shares the bounded row with it.
    @MainActor
    func testLongFormulaBesideALabelRemainsInsideTheRow() {
        var width: CGFloat = 0
        let view = HStack(spacing: 16) {
            Text("Statement")
            self.mathLeaf(String(repeating: "formal_term ", count: 40), spoken: "Long statement", hug: false)
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width = $0 }
        }.frame(width: 300, height: 100, alignment: .topLeading)
        TestResources.measured(view, width: 300, height: 100)
        XCTAssertGreaterThan(width, 0)
        XCTAssertLessThan(width, 300, "A formula must share the bounded row with its label")
    }

    /// A wide formula is a real horizontal scroll view whose end is reachable.
    @MainActor
    func testWideFormulaInstallsARealHorizontalScrollView() throws {
        let (host, window) = TestResources.hosted(self.mathLeaf(String(repeating: "formal_term ", count: 40), spoken: "Long statement", hug: false)
            .frame(width: 300, height: 100, alignment: .topLeading), width: 300, height: 100)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first, "A clipped formula is not a scrollable formula")
        try TestResources.assertInspectableEnd(scroll)
    }

    /// Intrinsic short hypotheses expose their mathematical baselines instead of a scroll container's bottom edge.
    @MainActor
    func testIntrinsicHypothesesDoNotBecomeBaselineClippingScrollViews() throws {
        let items = ["hp", "hq"].map { name in
            Fixtures.framed(["cross": "center", "hug": .bool(true), "textSize": .number(25), "mathScale": .number(1.08)],
                                  Fixtures.math(Fixtures.mathOp(":", Fixtures.mathSym(name), Fixtures.mathSym(String(name.dropFirst()))), spoken: name))
        }
        let row = Fixtures.framed(["gap": .number(16), "align": "baseline", "hug": .bool(true)],
                                       .object(["kind": .string("stack"), "axis": .string("horizontal"), "children": .array(items)]))
        let (host, window) = TestResources.hosted(NodeView(node: row, drawing: Fixtures.drawing()).frame(width: 369, height: 100, alignment: .topLeading), width: 369, height: 100)
        defer { window.close() }
        XCTAssertTrue(TestResources.scrollViews(host).isEmpty, "Intrinsic short hypotheses must expose their mathematical baselines instead of a scroll container’s bottom edge")
    }

    // Extra: a themed subtree's text token scopes the ink of everything inside it, not just
    // its canvas.
    @MainActor
    func testThemedNativeControlsUseTheCanvasAppearanceInsteadOfTheSystemAppearance_extraThemedInkScopesTheWordsInsideIt() throws {
        let theme = Tree.object(["tokens": .array([.object(["name": .string("text"), "light": .string("#FF0000"), "dark": .string("#FF0000")])]),
                                 "spacing": .array([.number(8)]), "feedback": .number(0), "settle": .number(0)])
        let node = Tree.object(["kind": .string("themed"), "theme": theme, "body": Fixtures.text("Ink")])
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.colorScheme, .light).frame(width: 200, height: 40).background(Color.white), width: 200, height: 40)
        let bitmap = try TestResources.raster(host)
        XCTAssertGreaterThan(TestResources.pixels(bitmap, where: { $0.redComponent > 0.8 && $0.blueComponent < 0.2 }), 10)
        window.close()
    }

    // Extra: paper ink stays dark in the light system appearance too.
    @MainActor
    func testPaperPickerUsesApplicationInkEvenWhenTheSystemIsDark_extraPaperPickerKeepsInkInLightSystemAppearance() throws {
        let node = Fixtures.picker([("proof", "Checked theorem")], selected: "proof")
        let (host, window) = TestResources.hosted(PickerView(node: node) { _ in }
            .environment(\.ospreyTheme, Fixtures.resolver([("secondary", "#242923")]))
            .environment(\.colorScheme, .light)
            .frame(width: 430, height: 60).background(Color.white), width: 430, height: 60)
        let bitmap = try TestResources.backingRaster(host)
        XCTAssertGreaterThan(TestResources.pixels(bitmap, where: { max($0.redComponent, $0.greenComponent, $0.blueComponent) < 0.3 }), 50)
        window.close()
    }

    // Extra: the picker keeps the complete mathematical name as its accessible value.
    @MainActor
    func testLongPickerOptionsRemainInsideTheMathematicalPane_extraAlongPickerKeepsItsFullNameReachable() throws {
        let name = String(repeating: "Namespace.Nested.", count: 15) + "theorem"
        var value = ""
        let (host, window) = TestResources.hosted(PickerView(node: Fixtures.picker([("proof", name)], selected: "proof")) { _ in }
            .onGeometryChange(for: String.self, of: { _ in "" }) { _ in value = name }
            .frame(width: 160, height: 60), width: 160, height: 60)
        _ = host
        XCTAssertEqual(value, name, "The menu must retain the complete mathematical name")
        window.close()
    }

    // Extra: a leading aside keeps its column on the left and the board on the right.
    @MainActor
    func testImportedNamesAndMathematicsDoNotPaintAcrossTheAdjacentBoard_extraALeadingAsideKeepsItsColumnLeftOfTheBoard() throws {
        let node = Tree.object(["kind": .string("aside"),
                                "pane": Fixtures.framed(["grow": .bool(true), "fill": "board"], Fixtures.text("")),
                                "side": .string("leading"),
                                "aside": Fixtures.framed(["width": .number(200), "fill": "column"], Fixtures.text("Aside")),
                                "width": .number(200), "least": .number(100), "most": .number(400), "event": .number(1)])
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("board", "#00FF00"), ("column", "#FF0000")]))
            .frame(width: 1000, height: 400), width: 1000, height: 400)
        let bitmap = try TestResources.raster(host)
        let column = try TestResources.pixel(bitmap, 100, 200)
        let board = try TestResources.pixel(bitmap, 600, 200)
        XCTAssertGreaterThan(column.redComponent, 0.9, "the aside column sits on the left")
        XCTAssertLessThan(column.greenComponent, 0.1)
        XCTAssertGreaterThan(board.greenComponent, 0.9, "the pane keeps the board beside it")
        window.close()
    }

    // Extra: a two-line formula wraps nothing — its height stays one line of ink.
    @MainActor
    func testLongFormalStatementRemainsInsideItsInspectablePane_extraAOneLineFormulaKeepsOneLineOfHeight() {
        let size = TestResources.measured(self.mathLeaf("p → q", hug: false), width: 300, height: 100)
        XCTAssertLessThan(size.height, 40)
        XCTAssertGreaterThan(size.height, 10)
    }

    // Extra: fractions and roots keep natural bounds under a fitting frame.
    @MainActor
    func testFittingMathematicsKeepsItsNaturalWidthAndHeight_extraFractionsAndRootsKeepNaturalBounds() {
        let frac = Fixtures.math(.object(["kind": .string("frac"),
                                          "top": Fixtures.mathNum("1"), "bottom": Fixtures.mathNum("2")]))
        let root = Fixtures.math(.object(["kind": .string("root"), "body": Fixtures.mathNum("2")]))
        for node in [frac, root] {
            let size = TestResources.measured(NodeView(node: node, drawing: Fixtures.drawing())
                .environment(\.ospreyInherited, Inherited().merging(["hug": .bool(true)])), width: 300, height: 100)
            XCTAssertGreaterThan(size.width, 4)
            XCTAssertLessThan(size.width, 100)
            XCTAssertLessThan(size.height, 100)
        }
    }

    // Extra: a superscript whose script is itself a formula keeps room for the wider script.
    @MainActor
    func testSuperscriptInkReservesItsRaisedExtentInsideAClippedFrame_extraNestedSuperscriptsKeepRoomForTheWiderScript() {
        let single = NSHostingView(rootView: mathView(mathOp("sup", script: "2")).fixedSize())
        let nestedTree = Tree.object(["kind": .string("sup"), "base": Fixtures.mathSym("p"),
                                      "script": Fixtures.mathOp("→", Fixtures.mathSym("q"), Fixtures.mathSym("r"))])
        let nested = NSHostingView(rootView: mathView(nestedTree).fixedSize())
        single.layoutSubtreeIfNeeded()
        nested.layoutSubtreeIfNeeded()
        XCTAssertGreaterThan(nested.fittingSize.width, single.fittingSize.width + 8)
    }

    // Extra: a subscript and a superscript together reserve both extents.
    @MainActor
    func testSubscriptInkReservesItsLoweredExtentInsideAClippedFrame_extraSubscriptAndSuperscriptReserveBothExtents() {
        let tree = Tree.object(["kind": .string("seq"), "items": .array([
            mathOp("sub", script: "2"), Fixtures.mathSym("p"), mathOp("sup", script: "2"),
        ])])
        let both = NSHostingView(rootView: mathView(tree).fixedSize())
        both.layoutSubtreeIfNeeded()
        XCTAssertGreaterThan(both.fittingSize.height, baseFitting().height + 8)
    }

    // Extra: three fitting hypotheses share the row without any one collapsing or exploding.
    @MainActor
    func testAdjacentFittingHypothesesKeepTheirNaturalWidths_extraThreeFittingHypothesesShareTheRow() {
        var widths = [CGFloat](repeating: 0, count: 3)
        let view = HStack(spacing: 12) {
            ForEach(0..<3) { index in
                self.mathLeaf("h\(index): p", hug: true)
                    .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { widths[index] = $0 }
            }
        }.frame(width: 300, height: 100, alignment: .topLeading)
        TestResources.measured(view, width: 300, height: 100)
        for width in widths {
            XCTAssertGreaterThan(width, 30)
            XCTAssertLessThan(width, 90)
        }
    }

    // Extra: two bounded formulae beside each other each take less than half of the row.
    @MainActor
    func testLongFormulaBesideALabelRemainsInsideTheRow_extraTwoBoundedFormulaeShareOneRow() {
        var widths = [CGFloat](repeating: 0, count: 2)
        let view = HStack(spacing: 12) {
            ForEach(0..<2) { index in
                self.mathLeaf(String(repeating: "formal_term ", count: 40), spoken: "Long \(index)", hug: false)
                    .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { widths[index] = $0 }
            }
        }.frame(width: 300, height: 100, alignment: .topLeading)
        TestResources.measured(view, width: 300, height: 100)
        for width in widths {
            XCTAssertGreaterThan(width, 20, "each formula keeps a readable sliver")
            XCTAssertLessThan(width, 160, "two formulae must share the row")
        }
    }

    // Extra: a short formula installs no scroll view at all.
    @MainActor
    func testWideFormulaInstallsARealHorizontalScrollView_extraAShortFormulaInstallsNoScrollView() throws {
        let (host, window) = TestResources.hosted(self.mathLeaf("p", hug: false)
            .frame(width: 300, height: 100, alignment: .topLeading), width: 300, height: 100)
        defer { window.close() }
        XCTAssertTrue(TestResources.scrollViews(host).isEmpty, "a fitting formula needs no scroll container")
    }

    // Extra: a single intrinsic hypothesis keeps exactly its natural width.
    @MainActor
    func testIntrinsicHypothesesDoNotBecomeBaselineClippingScrollViews_extraASingleIntrinsicHypothesisKeepsNaturalWidth() {
        var width: CGFloat = 0
        let view = self.mathLeaf("hp: p", hug: true)
            .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width = $0 }
        TestResources.measured(view.frame(width: 369, height: 100, alignment: .topLeading), width: 369, height: 100)
        XCTAssertGreaterThan(width, 40)
        XCTAssertLessThan(width, 120)
    }

    // MARK: helpers

    /// One `math` leaf at the size and hug state a frame would author.
    private func mathLeaf(_ words: String, spoken: String = "formula", hug: Bool) -> some View {
        NodeView(node: Fixtures.math(Fixtures.mathSym(words), spoken: spoken), drawing: Fixtures.drawing())
            .environment(\.ospreyInherited, Inherited().merging(hug ? ["hug": .bool(true)] : [:]))
    }

    /// One mathematics tree at `size`.
    private func mathView(_ inner: Tree, size: CGFloat = 27) -> MathView { MathView(node: inner, size: size) }

    /// A `sup` or `sub` around the variable `p`.
    private func mathOp(_ kind: String, script: String) -> Tree {
        .object(["kind": .string(kind), "base": Fixtures.mathSym("p"), "script": Fixtures.mathNum(script)])
    }

    private func baseFitting() -> CGSize {
        let host = NSHostingView(rootView: mathView(Fixtures.mathSym("p")).fixedSize())
        host.layoutSubtreeIfNeeded()
        return host.fittingSize
    }
}
