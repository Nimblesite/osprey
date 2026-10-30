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
    func testThemedNativeControlsUseTheCanvasAppearanceInsteadOfTheSystemAppearance() throws {
        for (canvas, system, expected) in [("#FAFAF7", ColorScheme.dark, NSAppearance.Name.aqua), ("#202723", .light, .darkAqua)] {
            let theme = Theme(tokens: [ThemeToken(name: "canvas", light: canvas, dark: canvas)], spacing: [], feedback: 0, settle: 0)
            let node = Node.themed(theme, .picker(label: "Theorem", choices: [Choice(key: "proof", label: "Checked theorem")], selected: "proof", event: 0))
            let view = NodeView(node: node, drawing: Drawing(region: "test", revision: 1, send: { _, _ in }))
                .environment(\.colorScheme, system).frame(width: 430, height: 60)
            let (host, window) = TestResources.hosted(view, width: 430, height: 60)
            defer { window.close() }
            let popup = try XCTUnwrap(descendants(host).compactMap { $0 as? NSPopUpButton }.first)
            XCTAssertEqual(popup.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]), expected, "Native arrows and menus must contrast with the application's canvas")
        }
    }

    @MainActor
    func testPaperPickerUsesApplicationInkEvenWhenTheSystemIsDark() throws {
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#242923", dark: "#242923")], spacing: [], feedback: 0, settle: 0)
        let view = PickerView(label: "Theorem", choices: [Choice(key: "proof", label: "Checked theorem")], selected: "proof", choose: { _ in })
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).environment(\.colorScheme, .dark)
            .foregroundStyle(Color(red: 36 / 255, green: 41 / 255, blue: 35 / 255)).frame(width: 430, height: 60).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 430, height: 60)
        defer { window.close() }
        let popup = try XCTUnwrap(descendants(host).compactMap { $0 as? NSPopUpButton }.first)
        let bitmap = try XCTUnwrap(popup.bitmapImageRepForCachingDisplay(in: popup.bounds))
        popup.cacheDisplay(in: popup.bounds, to: bitmap)
        let ink = (6..<(bitmap.pixelsHigh - 6)).reduce(0) { count, y in
            count + (12..<(bitmap.pixelsWide - 36)).filter { x in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return max(color.redComponent, color.greenComponent, color.blueComponent) < 0.3
            }.count
        }
        XCTAssertGreaterThan(ink, 50, "Paper controls need visible dark title ink even when the system is dark")
    }

    @MainActor
    func testLongPickerOptionsRemainInsideTheMathematicalPane() throws {
        let name = String(repeating: "Namespace.Nested.", count: 15) + "theorem"
        let view = PickerView(label: "Theorem", choices: [Choice(key: "proof", label: name)], selected: "proof", choose: { _ in })
            .frame(width: 300, height: 60)
        let (host, window) = TestResources.hosted(view, width: 300, height: 60)
        defer { window.close() }
        let popup = try XCTUnwrap(descendants(host).compactMap { $0 as? NSPopUpButton }.first)
        let bounds = popup.convert(popup.bounds, to: host)
        XCTAssertGreaterThanOrEqual(bounds.minX, 0, "The picker must not expand into the adjacent working board")
        XCTAssertLessThanOrEqual(bounds.maxX, 300, "The selected theorem must not push its control past the pane")
        XCTAssertEqual(popup.selectedItem?.title, name, "The menu must retain the complete mathematical name")
    }

    @MainActor
    private func descendants(_ view: NSView) -> [NSView] { [view] + view.subviews.flatMap(descendants) }

    @MainActor
    func testImportedNamesAndMathematicsDoNotPaintAcrossTheAdjacentBoard() throws {
        let name = String(repeating: "Namespace.Nested.", count: 15) + "theorem"
        let items: [Node] = [.section(title: "Proof library", footer: "Inspect checked mathematics.", [.text("Open proof library", .body, .primary)]),
            .picker(label: "Theorem", choices: [Choice(key: "proof", label: name)], selected: "proof", event: 0),
            .text(name, .headline, .primary), .math(.sym(name), spoken: name)]
        let pane = Node.framed(try TestResources.frame(["grow": true, "fill": "board"]), .text("", .body, .primary))
        let node = Node.aside(pane: pane, side: .trailing, aside: .stack(.vertical, items), splitter: Splitter(width: 430, least: 160, most: 640, keep: 320, event: 1))
        let theme = Theme(tokens: [ThemeToken(name: "board", light: "#00FF00", dark: "#00FF00"), ThemeToken(name: "surface", light: "#FFFFFF", dark: "#FFFFFF")], spacing: [4, 8, 12, 16], feedback: 0, settle: 0)
        let view = NodeView(node: node, drawing: Drawing(region: "test", revision: 1, send: { _, _ in }))
            .environment(\.ospreyPlacement, .pane).environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 1000, height: 500)
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
        let board = try XCTUnwrap(bitmap.colorAt(x: 540, y: 40)?.usingColorSpace(.sRGB))
        XCTAssertLessThan(board.redComponent, 0.1, "A long theorem must not push the library card across the board")
        XCTAssertGreaterThan(board.greenComponent, 0.9)
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

    @MainActor
    func testWideFormulaInstallsARealHorizontalScrollView() throws {
        let view = MathViewport(tree: .sym(String(repeating: "formal_term ", count: 40)), size: 27, spoken: "Long statement")
            .frame(width: 300, height: 100, alignment: .topLeading)
        let (host, window) = TestResources.hosted(view, width: 300, height: 100)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first, "A clipped formula is not a scrollable formula")
        try TestResources.assertInspectableEnd(scroll)
    }

    @MainActor
    func testIntrinsicHypothesesDoNotBecomeBaselineClippingScrollViews() throws {
        let style = try TestResources.frame(["cross": "center", "hug": true, "textSize": 25, "mathScale": 1.08])
        let items = [("hp", "p"), ("hq", "q")].map { name, variable in
            Node.framed(style, .math(.op(":", .sym(name), .sym(variable)), spoken: name + ": " + variable))
        }
        let row = Node.framed(try TestResources.frame(["gap": 16, "align": "baseline", "hug": true]), .stack(.horizontal, items))
        let view = NodeView(node: row, drawing: Drawing(region: "hypotheses", revision: 1, send: { _, _ in })).frame(width: 369, height: 100, alignment: .topLeading)
        let (host, window) = TestResources.hosted(view, width: 369, height: 100)
        defer { window.close() }
        XCTAssertTrue(TestResources.scrollViews(host).isEmpty, "Intrinsic short hypotheses must expose their mathematical baselines instead of a scroll container’s bottom edge")
    }


}
