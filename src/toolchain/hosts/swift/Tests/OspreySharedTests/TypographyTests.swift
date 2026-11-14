import AVKit
import AppKit
import CoreText
import SwiftUI
import XCTest

@testable import OspreyShared

// Typography, media and board pictures the Workbench's forked host proved, restored on the shared
// renderer: every input is the told tree a frame hands the host ([0053-NativeSharedViews]).

final class TypographyTests: RenderingTestCase {
    override func setUpWithError() throws {
        try Repo.registerFonts(["STIXTwoText.ttf", "STIXTwoText-Italic.ttf", "STIXTwoMath-Regular.ttf", "Geist.ttf"])
    }

    func testRegularSerifResolvesToTheBundledMathematicalFace() throws {
        XCTAssertTrue(FileManager.default.fileExists(atPath: Repo.assets.appendingPathComponent("fonts/STIXTwoText.ttf").path))
        let resolved = CTFontCreateWithName(FontRegistration.serifName as CFString, 27, nil)
        XCTAssertEqual(CTFontCopyPostScriptName(resolved) as String, "STIXTwoText")
    }

    @MainActor
    func testNaturalFrameKeepsItsContentHeightInsideAPane() throws {
        let height = measuredHeight(["name": "review", "width": 529, "top": 20, "right": 20, "bottom": 20, "left": 20, "radius": 5])
        XCTAssertGreaterThan(height, 30, "The geometry callback must observe the content")
        XCTAssertLessThan(height, 100, "A natural dialog frame must not inherit its 700-point pane height")
    }

    @MainActor
    func testAsideDividerSpansTheTallerDocumentRatherThanOnlyTheGoal() throws {
        let bitmap = try TestResources.rendered(asideView(document: 120, goal: 20))
        XCTAssertLessThan(try TestResources.pixel(bitmap, 280, 60).redComponent, 0.1, "The goal divider must span the adjacent 120-point document")
    }

    @MainActor
    func testLayerKeepsAnOverlaidControlAtItsAuthoredTopInset() throws {
        let bitmap = try TestResources.rendered(layered(top: 12, size: 32))
        XCTAssertLessThan(try TestResources.pixel(bitmap, 160, 16).redComponent, 0.1, "A source close control must stay 12 points from the top, rather than centering in the source panel")
        for y in [11, 44] {
            XCTAssertGreaterThan(try TestResources.pixel(bitmap, 160, y).redComponent, 0.9, "Pixels immediately outside the authored control must stay clear")
        }
        for y in [12, 43] {
            XCTAssertLessThan(try TestResources.pixel(bitmap, 160, y).redComponent, 0.1, "The control must occupy exactly y12 through y43")
        }
    }

    @MainActor
    func testActualMathematicalProseKeepsItsAuthoredSingleLineHeight() throws {
        let style: [String: Tree] = ["font": "serif", "textSize": 21, "lineHeight": .number(31.5)]
        let runs = [run("From "), run("hq", italic: true), run(".")]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("From hq.", "lead")), 31.5, accuracy: 0.5)
        XCTAssertEqual(measuredHeight(style, content: rich(runs, "lead")), 31.5, accuracy: 0.5)
    }

    @MainActor
    func testChalkBackdropRemainsVisibleAgainstThePaperTheme() throws {
        let image = try XCTUnwrap(NSImage(contentsOf: Repo.assets.appendingPathComponent("chalk-study.png")))
        let chalkboard = Color(red: 32.0 / 255, green: 39.0 / 255, blue: 35.0 / 255)
        let scene = ZStack { Color.white; faded(image).background(chalkboard) }.frame(width: 1010, height: 676)
        XCTAssertGreaterThan(brightPixels(scene), 500, "A light parent must not erase the chalk diagram on its dark working board")
    }

    @MainActor
    func testNonemptyCaptionKeepsItsAuthoredLineHeightWithTheBundledFont() throws {
        let style: [String: Tree] = ["font": "sans", "textSize": 13, "lineHeight": .number(22.75)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("Hypotheses", "caption", "secondary")), 22.75, accuracy: 0.01)
    }

    @MainActor
    func testDefaultSystemFaceKeepsAnAuthoredCaptionLineHeight() throws {
        let style: [String: Tree] = ["textSize": 13, "lineHeight": .number(22.75)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("Hypotheses", "caption", "secondary")), 22.75, accuracy: 0.01)
    }

    @MainActor
    func testWrappedSourceMeasuresActualFirstAndLastBaselines() throws {
        for count in [9, 41] {
            XCTAssertEqual(baselineDistance(lines: count), (Double(count - 1) * 26.6).rounded(.up), accuracy: 0.01,
                           "Native text must round the total baseline distance outward once, including forty intervals")
        }
    }

    @MainActor
    func testRepeatedSourceLinesKeepExactAuthoredBaselineSpacing() throws {
        let lines = (0..<9).map { _ in rich([run("exact hq")], "mono") }
        XCTAssertEqual(measuredHeight(sourceStyle, content: column(lines)), 239.4, accuracy: 0.05, "Nine source lines must not accumulate fractional font-metric errors")
    }

    @MainActor
    func testEmptyCaptionDoesNotAddAnInvisibleLineToTheDocument() throws {
        let style: [String: Tree] = ["font": "sans", "textSize": 13, "lineHeight": .number(22.75)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("", "caption", "secondary")), 0, accuracy: 0.01,
                       "An empty caption must match the browser’s zero-height paragraph instead of pushing the manuscript beyond its window")
    }

    @MainActor
    func testGrowingFrameKeepsNaturalContentCenteredWhenItFits() throws {
        let bitmap = try TestResources.rendered(centred(card: 40))
        XCTAssertLessThan(try TestResources.pixel(bitmap, 100, 100).redComponent, 0.1, "A fitting empty-state card must keep the frame’s authored center alignment")
    }

    @MainActor
    func testGrowingFrameScrollsOversizedNaturalContentWithinItsPane() throws {
        let tall = Fixtures.framed(["height": 900], Fixtures.text("A long mathematical document"))
        XCTAssertEqual(measuredHeight(["grow": true], content: tall), 700, accuracy: 0.01, "Natural content must scroll inside its bounded pane instead of pushing the app header off screen")
    }

    @MainActor
    func testNaturalFrameIncludesItsBorderInLayoutLikeTheBrowser() throws {
        XCTAssertEqual(measuredHeight(["line": "outline"]) - measuredHeight([:]), 2, accuracy: 0.01, "A one-point border must occupy one point on each side of the content")
    }

    @MainActor
    func testApplicationLineHeightControlsWrappedMathematicalTypography() throws {
        let style: [String: Tree] = ["name": "title", "width": 369, "font": "serif", "textSize": 29, "lineHeight": .number(32.48)]
        let height = measuredHeight(style, content: Fixtures.text("Commutativity\nof conjunction", "display"))
        XCTAssertEqual(height, 64.96, accuracy: 0.55, "The two-line title must use the application’s leading, not the font’s default")
    }

    @MainActor
    func testMathematicsUsesScopedSlateInkInsteadOfAncestorForeground() throws {
        // Mathematics always sits in a frame, whose ink is the theme's, whatever an ancestor chose.
        let view = framedView(["textSize": 48, "hug": true], Fixtures.math(Fixtures.mathSym("p")), theme: [("text", "#ECEDE5")])
            .foregroundStyle(Color.black)
            .frame(width: 100, height: 100).background(Color.black)
        XCTAssertGreaterThan(brightPixels(view), 20, "Scoped slate mathematics must remain legible over the dark surface")
    }

    @MainActor
    func testDisplayedFormulaUsesMathematicalInkHeightInsteadOfTextLineHeight() throws {
        let height = measuredHeight(["textSize": 48], content: Fixtures.math(commutes, spoken: "Conjunction commutes"))
        XCTAssertGreaterThan(height, 30)
        XCTAssertLessThan(height, 40, "The reference’s 48-point formula occupies its ink bounds, not a full text line")
    }

    @MainActor
    func testMathematicalVariableUsesItsItalicMathGlyph() throws {
        let host = NSHostingView(rootView: MathView(node: Fixtures.mathSym("p"), size: 48).fixedSize())
        XCTAssertEqual(host.fittingSize.width, mathAdvance(["𝑝"], size: 48).rounded(.up), accuracy: 0.01, "A mathematical variable must use the font’s mathematical italic alphabet")
    }

    @MainActor
    func testSharedMathematicsScalePreservesTheReferencesFractionalTypeSize() throws {
        let style: [String: Tree] = ["textSize": 34, "mathScale": .number(1.08), "hug": true]
        let host = NSHostingView(rootView: framedView(style, Fixtures.math(Fixtures.mathSym("p"), spoken: "p")).fixedSize())
        XCTAssertEqual(host.fittingSize.width, mathAdvance(["𝑝"], size: 36.72).rounded(.up), accuracy: 0.01, "The 1.08 scale must preserve 36.72 points instead of rounding the reference to 37")
    }

    @MainActor
    func testLogicalOperatorSpacingMatchesTheMeasuredReferenceFormula() throws {
        let host = NSHostingView(rootView: MathView(node: commutes, size: 36.72).fixedSize())
        XCTAssertEqual(host.fittingSize.width, 217, accuracy: 0.5, "The reference’s MathML row measures 216.828 points; native layout rounds outward once")
    }

    @MainActor
    func testHypothesisColonUsesPunctuationSpacingRatherThanBinarySpacing() throws {
        let host = NSHostingView(rootView: MathView(node: Fixtures.mathOp(":", Fixtures.mathSym("hp"), Fixtures.mathSym("p")), size: 27).fixedSize())
        XCTAssertEqual(host.fittingSize.width, (mathAdvance(["hp", ":", "𝑝"], size: 27) + 4.5).rounded(.up), accuracy: 0.01, "A binding colon has no space before it and a thin space after it")
    }

    @MainActor
    func testCompactSaveButtonKeepsItsLabelOnOneLine() throws { try saveLabel(pointSize: 12) }

    @MainActor
    func testConstrainedButtonsDoNotBreakTheirLabelsIntoVerticalFragments() throws { try saveLabel(pointSize: 14) }

    @MainActor
    func testVideoRendererPreservesTheAuthoredCaptionFile() throws {
        let video = film(captions: "film.en.vtt")
        guard case let .view(leaf) = drawnLeaf(video) else { return XCTFail("A video must draw as the film leaf") }
        XCTAssertEqual(leaf.node.text("captions"), "film.en.vtt", "The native renderer must retain the same caption file that the browser enables")
        XCTAssertEqual(leaf.node.count("seconds"), 42)
    }

    @MainActor
    func testLandscapeLessonVideoPreservesItsSixteenByNineLayout() {
        let size = TestResources.measured(FilmPlayer(player: AVPlayer(), cues: [], onFailure: { _ in }).fixedSize(horizontal: false, vertical: true), width: 640, height: 700)
        XCTAssertEqual(size.height, 360, accuracy: 0.5, "The shipped 1920×1080 films must not use a portrait player")
    }

    func testCaptionsFollowPlaybackIncludingExactCueBoundaries() throws {
        let cues = try FilmCaptions.parse("WEBVTT\n\n00:00.350 --> 00:01.500\nAssume h.\n\nsecond\n00:01.000 --> 00:02.000\nFrom hq.\nThen hp.\n")
        XCTAssertEqual(FilmCaptions.text(cues, at: 0), "")
        XCTAssertEqual(FilmCaptions.text(cues, at: 0.350), "Assume h.")
        XCTAssertEqual(FilmCaptions.text(cues, at: 1.0), "Assume h.\nFrom hq.\nThen hp.")
        XCTAssertEqual(FilmCaptions.text(cues, at: 1.5), "From hq.\nThen hp.")
        XCTAssertEqual(FilmCaptions.text(cues, at: 2), "")
    }

    @MainActor
    func testAFileThatCannotDecodeReportsAVisiblePlaybackFailure() throws {
        let (player, failure) = try undecodable()
        XCTAssertEqual(player.currentItem?.status, .failed, "The invalid fixture must actually fail to decode")
        XCTAssertFalse(failure.isEmpty, "An AVPlayer decoding failure must be reported beside the player")
    }

    func testCaptionHeaderVariantsAndMathematicalTextAreReadable() throws {
        let cues = try FilmCaptions.parse("\u{FEFF}WEBVTT - Mathematics\n\n00:00.000 --> 00:02.000\np < q and q &lt; r &amp; p\n")
        XCTAssertEqual(FilmCaptions.text(cues, at: 1), "p < q and q < r & p")
    }

    func testMissingAndMalformedCaptionTracksFailExplicitly() {
        for source in ["", "WEBVTT", "WEBVTT\n\nnot timing", "WEBVTT\n\n00:60.000 --> 01:01.000\nBad", "WEBVTT\n\n00:02.000 --> 00:01.000\nBad", "WEBVTT\n\n00:00.000 --> 00:01.000\n<b>Unsupported markup</b>"] {
            XCTAssertThrowsError(try FilmCaptions.parse(source), source)
        }
        XCTAssertThrowsError(try FilmCaptions.load("../outside.vtt"))
        XCTAssertThrowsError(try FilmCaptions.load("missing-caption.vtt"))
        XCTAssertThrowsError(try FilmCaptions.parse(String(repeating: "x", count: 65537)))
    }

    func testEveryShippedLessonFilmMeetsCaptionAndByteLimits() async throws {
        for slug in ["what-is-a-proof", "what-is-a-solver", "navier-stokes"] {
            let file = lessons.appendingPathComponent("media/\(slug)/film.en.vtt")
            let cues = try FilmCaptions.parse(String(contentsOf: file, encoding: .utf8))
            XCTAssertFalse(cues.isEmpty, slug)
            XCTAssertLessThanOrEqual(try XCTUnwrap(cues.last).end, 60, slug)
            let film = file.deletingLastPathComponent().appendingPathComponent("film.mp4")
            let size = try XCTUnwrap(FileManager.default.attributesOfItem(atPath: film.path)[.size] as? NSNumber)
            XCTAssertLessThanOrEqual(size.intValue, 2_000_000, slug)
            let duration = try await AVURLAsset(url: film).load(.duration)
            XCTAssertEqual(try declaredSeconds(lessons.appendingPathComponent("\(slug).json")), Int(ceil(duration.seconds)), slug)
        }
    }

    @MainActor
    func testLinksRespectTheSharedFramesAuthoredTypeSize() throws {
        let small = measuredHeight(["font": "sans", "textSize": 11], content: link)
        let large = measuredHeight(["font": "sans", "textSize": 22], content: link)
        XCTAssertGreaterThan(large, small * 1.5, "Footer links must inherit the same authored typography as adjacent text")
    }

    @MainActor
    func testLinksDoNotInventAnIconMissingFromTheSharedNode() throws {
        let style: [String: Tree] = ["font": "sans", "textSize": 11, "hug": true]
        let linked = try XCTUnwrap(ImageRenderer(content: framedView(style, link).fixedSize()).cgImage)
        let plain = try XCTUnwrap(ImageRenderer(content: framedView(style, Fixtures.text("Built with Osprey", "caption")).fixedSize()).cgImage)
        XCTAssertEqual(linked.width, plain.width, "The protocol carries no link icon; native must match the browser's label-only link")
    }

    @MainActor
    func testStandaloneIconsRespectTheSharedFramesAuthoredSize() throws {
        let icon = Tree.object(["kind": "icon", "symbol": "chevron.left.forwardslash.chevron.right"])
        let small = measuredHeight(["font": "sans", "textSize": 12], content: icon)
        let large = measuredHeight(["font": "sans", "textSize": 24], content: icon)
        XCTAssertGreaterThan(large, small * 1.5, "A source-heading icon must use its authored size rather than the generic 32-point fallback")
    }

    @MainActor
    func testMonospacedSourceUsesItsAuthoredLineHeight() throws {
        let height = measuredHeight(sourceStyle, content: Fixtures.text("namespace proofs\nexact hp", "mono"))
        XCTAssertEqual(height, 53.2, accuracy: 0.5, "Two source lines must occupy the authored 26.6-point baselines")
    }

    @MainActor
    func testStatusBadgeKeepsTheReferencesTwentySevenPointHeight() throws {
        let height = measuredHeight([:], content: badge("Checked", tone: "good"))
        XCTAssertEqual(height, 27, accuracy: 0.1, "Changing a verdict must not change the document's vertical rhythm")
    }

    @MainActor
    func testTrailingSymbolsFollowTheirButtonLabel() throws {
        let ink = try outerInk(trailingButton("MMMMMMMM"))
        XCTAssertGreaterThan(ink.0, ink.1, "The dense text must precede the smaller trailing plus symbol")
    }

    @MainActor
    func testBackdropFadeBlendsTheImageCornersIntoTheBoard() throws {
        let bitmap = try TestResources.rendered(faded(whiteSquare()).background(Color.black))
        XCTAssertLessThan(try TestResources.pixel(bitmap, 2, 10).redComponent, 0.05, "The illustrative image's corner must fade into its board")
        XCTAssertGreaterThan(try TestResources.pixel(bitmap, 505, 338).redComponent, 0.95, "The center of the illustrative image must remain opaque")
        XCTAssertGreaterThan(try TestResources.pixel(bitmap, 190, 125).redComponent, 0.95, "The reference's broad opaque plateau must preserve chalk near the upper-left proof")
    }
}

// MARK: fixtures

extension TypographyTests {
    var sourceStyle: [String: Tree] { ["font": "mono", "textSize": 14, "lineHeight": .number(26.6)] }
    var lessons: URL { Repo.src.appendingPathComponent("osprey/workbench/lessons") }
    var link: Tree { .object(["kind": "link", "label": "Built with Osprey", "url": "https://ospreylang.dev/"]) }

    /// (p ∧ q) → (q ∧ p), the reference's displayed formula.
    var commutes: Tree {
        Fixtures.mathOp("→", Fixtures.mathOp("∧", Fixtures.mathSym("p"), Fixtures.mathSym("q")), Fixtures.mathOp("∧", Fixtures.mathSym("q"), Fixtures.mathSym("p")))
    }

    func run(_ text: String, italic: Bool = false) -> Tree {
        .object(["text": .string(text), "ink": "", "italic": .bool(italic), "emphasis": ""])
    }

    func rich(_ runs: [Tree], _ role: String) -> Tree { .object(["kind": "rich-text", "runs": .array(runs), "role": .string(role)]) }

    func column(_ children: [Tree]) -> Tree { .object(["kind": "stack", "axis": "vertical", "children": .array(children)]) }

    func row(_ children: [Tree]) -> Tree { .object(["kind": "stack", "axis": "horizontal", "children": .array(children)]) }

    func badge(_ label: String, tone: String, symbol: String = "") -> Tree {
        .object(["kind": "badge", "label": .string(label), "symbol": .string(symbol), "tone": .string(tone)])
    }

    func button(_ label: String, symbol: String) -> Tree {
        .object(["kind": "button", "label": .string(label), "symbol": .string(symbol), "role": "plain", "enabled": true, "event": 0])
    }

    func film(captions: String) -> Tree {
        .object(["kind": "video", "source": "lesson.mp4", "poster": "poster.jpg", "captions": .string(captions), "alt": "A proof.", "seconds": 42])
    }

    /// A frame over `content`, drawn as the shared renderer draws it.
    func framedView(_ style: [String: Tree], _ content: Tree, theme: [(name: String, colour: String)] = [("text", "#000000")]) -> some View {
        NodeView(node: Fixtures.framed(style, content), drawing: Fixtures.drawing()).environment(\.ospreyTheme, Fixtures.resolver(theme))
    }

    /// The leaf view the renderer draws for `node`, when it is a film.
    enum Drawn { case view(FilmLeaf), other }

    @MainActor
    func drawnLeaf(_ node: Tree) -> Drawn {
        let body = NodeView(node: node, drawing: Fixtures.drawing()).body
        return firstFilm(body).map(Drawn.view) ?? .other
    }

    private func firstFilm(_ value: Any) -> FilmLeaf? {
        if let film = value as? FilmLeaf { return film }
        return Mirror(reflecting: value).children.lazy.compactMap { self.firstFilm($0.value) }.first
    }

    func declaredSeconds(_ file: URL) throws -> Int {
        let doc = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any])
        let blocks = try XCTUnwrap(doc["blocks"] as? [[String: Any]])
        let clip = try XCTUnwrap(blocks.first { $0["kind"] as? String == "video" })
        return try XCTUnwrap(clip["seconds"] as? Int)
    }

    func mathAdvance(_ texts: [String], size: CGFloat) -> CGFloat {
        let face = CTFontCreateWithName(FontRegistration.mathName as CFString, size, nil)
        return texts.reduce(0) { total, text in
            total + CTLineGetTypographicBounds(CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): face])), nil, nil, nil)
        }
    }

    /// An undecodable film played until AVFoundation gives up, and the failure the player reported.
    @MainActor
    func undecodable() throws -> (AVPlayer, String) {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("workbench-invalid-\(UUID().uuidString).mp4")
        try Data("not a movie".utf8).write(to: file)
        defer { do { try FileManager.default.removeItem(at: file) } catch { XCTFail("Cannot remove the test film: \(error)") } }
        let player = AVPlayer(url: file)
        var failure = ""
        let host = NSHostingView(rootView: FilmPlayer(player: player, cues: [], onFailure: { failure = $0 }).frame(width: 640, height: 360))
        host.layoutSubtreeIfNeeded()
        player.play()
        let deadline = Date(timeIntervalSinceNow: 3)
        while player.currentItem?.status != .failed && Date() < deadline { TestResources.settle(0.05) }
        TestResources.settle(0.1)
        player.pause()
        return (player, failure)
    }
}

// MARK: views and measurements

extension TypographyTests {
    @MainActor
    func asideView(document: Int, goal: Int) -> some View {
        let tall = Fixtures.framed(["height": .number(Double(document))], Fixtures.text("Document"))
        let short = Fixtures.framed(["height": .number(Double(goal))], Fixtures.text("Goal"))
        let node = Tree.object(["kind": "aside", "side": "trailing", "pane": tall, "aside": short, "width": 120, "least": 80, "most": 300, "keep": 360, "event": 0])
        return NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("divider", "#000000")]))
            .frame(width: 400).fixedSize(horizontal: false, vertical: true).background(Color.white)
    }

    @MainActor
    func layered(top: Int, size: Int) -> some View {
        let background = Fixtures.framed(["width": 200, "height": 200, "fill": "canvas"], Fixtures.text(""))
        let control = Fixtures.framed(["width": .number(Double(size)), "height": .number(Double(size)), "fill": "ink"], Fixtures.text(""))
        let overlay = Fixtures.framed(["top": .number(Double(top)), "right": 12, "cross": "end"], control)
        return NodeView(node: .object(["kind": "layer", "children": .array([background, overlay])]), drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("canvas", "#FFFFFF"), ("ink", "#000000")])).fixedSize()
    }

    @MainActor
    func centred(card: Int) -> some View {
        let child = Fixtures.framed(["height": .number(Double(card)), "fill": "ink"], Fixtures.text(""))
        return framedView(["grow": true, "align": "center"], child, theme: [("ink", "#000000")]).frame(width: 200, height: 200).background(Color.white)
    }

    @MainActor
    func trailingButton(_ label: String) -> some View {
        let style: [String: Tree] = ["hug": true, "font": "sans", "textSize": 16, "trailingSymbols": true, "quietControls": true, "controlPadding": 0]
        return framedView(style, button(label, symbol: "plus")).fixedSize().background(Color.white)
    }

    func faded(_ image: NSImage) -> some View {
        Image(nsImage: image).resizable().frame(width: 1010, height: 676).modifier(BackdropFade(enabled: true))
    }

    func whiteSquare() -> NSImage {
        let image = NSImage(size: NSSize(width: 100, height: 100))
        image.lockFocus()
        NSColor.white.setFill()
        NSRect(x: 0, y: 0, width: 100, height: 100).fill()
        image.unlockFocus()
        return image
    }

    @MainActor
    func saveLabel(pointSize: Int) throws {
        let header: [String: Tree] = ["width": 200, "height": 75, "font": "sans", "textSize": .number(Double(pointSize)), "align": "center", "quietControls": true, "controlHeight": 40]
        let sized: [String: Tree] = ["width": 82, "height": 38, "quietControls": true, "controlHeight": 38]
        let spacer = Tree.object(["kind": "spacer"])
        let content = row([spacer, Fixtures.framed(sized, button("Save", symbol: "square.and.arrow.down")), spacer])
        let rows = TestResources.darkInkRows(try TestResources.rendered(framedView(header, content).frame(width: 200, height: 75).background(Color.white)))
        XCTAssertGreaterThan(rows.count, 5, "The button label must render")
        XCTAssertLessThan((rows.last ?? 0) - (rows.first ?? 0), 20, "Save must not wrap inside the design’s 82-point button")
    }

    @MainActor
    func baselineDistance(lines count: Int) -> CGFloat {
        let content = rich([run(Array(repeating: "exact hq", count: count).joined(separator: "\n"))], "mono")
        var distance: CGFloat = 0
        let view = BaselineProbe(receive: { distance = $0[.lastTextBaseline] - $0[.firstTextBaseline] }) { framedView(sourceStyle, content) }.fixedSize()
        NSHostingView(rootView: view).layoutSubtreeIfNeeded()
        return distance
    }

    @MainActor
    func brightPixels<V: View>(_ view: V) -> Int {
        guard let bitmap = try? TestResources.rendered(view) else { XCTFail("The snapshot must render"); return 0 }
        return TestResources.pixels(bitmap) { $0.redComponent > 0.6 && $0.greenComponent > 0.6 && $0.blueComponent > 0.6 }
    }

    /// Dark ink in the left third and in the right third of the picture.
    @MainActor
    func outerInk<V: View>(_ view: V) throws -> (Int, Int) {
        let bitmap = try TestResources.rendered(view)
        return (0..<bitmap.pixelsWide).reduce((0, 0)) { totals, x in
            let count = (0..<bitmap.pixelsHigh).filter { y in
                guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return colour.redComponent < 0.4 && colour.greenComponent < 0.4 && colour.blueComponent < 0.4
            }.count
            return (totals.0 + (x < bitmap.pixelsWide / 3 ? count : 0), totals.1 + (x >= bitmap.pixelsWide * 2 / 3 ? count : 0))
        }
    }

    @MainActor
    func measuredHeight(_ style: [String: Tree], content: Tree? = nil) -> CGFloat {
        // A told frame always carries its gap, zero unless authored, as `Tell` writes it.
        measuredHeight(view: framedView(["gap": 0].merging(style) { _, authored in authored }, content ?? column([Fixtures.text("Review")])), width: 529)
    }

    @MainActor
    func measuredHeight<V: View>(view: V, width: CGFloat) -> CGFloat {
        var height: CGFloat = 0
        let root = ZStack(alignment: .top) { view.onGeometryChange(for: CGFloat.self, of: \.size.height) { height = $0 } }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        let host = NSHostingView(rootView: root)
        host.setFrameSize(NSSize(width: width, height: 700))
        host.layoutSubtreeIfNeeded()
        TestResources.settle()
        return height
    }
}

private struct BaselineProbe: Layout {
    let receive: (ViewDimensions) -> Void

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let child = subviews.first else { return .zero }
        let measured = child.dimensions(in: proposal)
        receive(measured)
        return CGSize(width: measured.width, height: measured.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, anchor: .topLeading, proposal: proposal)
    }
}
