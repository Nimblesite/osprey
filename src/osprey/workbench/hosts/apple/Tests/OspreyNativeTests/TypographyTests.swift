import AVKit
import AppKit
import SwiftUI
import CoreText
import XCTest
@testable import OspreyNative

final class TypographyTests: XCTestCase {
    func testRegularSerifResolvesToTheBundledMathematicalFace() throws {
        let font = regularFont
        XCTAssertTrue(FileManager.default.fileExists(atPath: font.path))
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let resolved = CTFontCreateWithName(FontRegistration.serifName as CFString, 27, nil)
        XCTAssertEqual(CTFontCopyPostScriptName(resolved) as String, "STIXTwoText")
    }

    @MainActor
    func testNaturalFrameKeepsItsContentHeightInsideAPane() throws {
        let source = #"{"name":"review","width":529,"height":0,"top":20,"right":20,"bottom":20,"left":20,"radius":5,"gap":0,"fill":"","ink":"","line":"","grow":false}"#
        let style = try JSONDecoder().decode(FrameStyle.self, from: Data(source.utf8))
        let height = measuredHeight(style)
        XCTAssertGreaterThan(height, 30, "The geometry callback must observe the content")
        XCTAssertLessThan(height, 100, "A natural dialog frame must not inherit its 700-point pane height")
    }

    @MainActor
    func testAsideDividerSpansTheTallerDocumentRatherThanOnlyTheGoal() throws {
        let tall = Node.framed(try decodedStyle(["height": 120]), .text("Document", .body, .primary))
        let short = Node.framed(try decodedStyle(["height": 20]), .text("Goal", .body, .primary))
        let splitter = Splitter(width: 120, least: 80, most: 300, keep: 360, event: 0)
        let node = Node.aside(pane: tall, side: .trailing, aside: short, splitter: splitter)
        let theme = Theme(tokens: [ThemeToken(name: "divider", light: "#000000", dark: "#000000")], spacing: [], feedback: 0, settle: 0)
        let view = NodeView(node: node, drawing: Drawing(region: "test", revision: 1, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 400).fixedSize(horizontal: false, vertical: true).background(Color.white)
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
        let border = try XCTUnwrap(bitmap.colorAt(x: 280, y: 60)?.usingColorSpace(.sRGB))
        XCTAssertLessThan(border.redComponent, 0.1, "The goal divider must span the adjacent 120-point document")
    }

    @MainActor
    func testLayerKeepsAnOverlaidControlAtItsAuthoredTopInset() throws {
        let background = Node.framed(try decodedStyle(["width": 200, "height": 200, "fill": "canvas"]), .text("", .body, .primary))
        let control = Node.framed(try decodedStyle(["width": 32, "height": 32, "fill": "primary"]), .text("", .body, .primary))
        let overlay = Node.framed(try decodedStyle(["top": 12, "right": 12, "cross": "end"]), control)
        let theme = Theme(tokens: [ThemeToken(name: "canvas", light: "#FFFFFF", dark: "#FFFFFF"), ThemeToken(name: "primary", light: "#000000", dark: "#000000")], spacing: [], feedback: 0, settle: 0)
        let view = NodeView(node: .layer([background, overlay]), drawing: Drawing(region: "source", revision: 1, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).fixedSize()
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
        let top = try XCTUnwrap(bitmap.colorAt(x: 160, y: 16)?.usingColorSpace(.sRGB))
        XCTAssertLessThan(top.redComponent, 0.1, "A source close control must stay 12 points from the top, rather than centering in the source panel")
        for y in [11, 44] {
            XCTAssertGreaterThan(try XCTUnwrap(bitmap.colorAt(x: 160, y: y)?.usingColorSpace(.sRGB)).redComponent, 0.9, "Pixels immediately outside the authored control must stay clear")
        }
        for y in [12, 43] {
            XCTAssertLessThan(try XCTUnwrap(bitmap.colorAt(x: 160, y: y)?.usingColorSpace(.sRGB)).redComponent, 0.1, "The control must occupy exactly y12 through y43")
        }
    }

    @MainActor
    func testActualMathematicalProseKeepsItsAuthoredSingleLineHeight() throws {
        let italic = regularFont.deletingLastPathComponent().appendingPathComponent("STIXTwoText-Italic.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(regularFont as CFURL, .process, nil))
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(italic as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(regularFont as CFURL, .process, nil); CTFontManagerUnregisterFontsForURL(italic as CFURL, .process, nil) }
        let style = try decodedStyle(["font": "serif", "textSize": 21, "lineHeight": 31.5])
        let runs = [TextRun(text: "From ", ink: "", italic: false), TextRun(text: "hq", ink: "", italic: true), TextRun(text: ".", ink: "", italic: false)]
        XCTAssertEqual(measuredHeight(style, content: .text("From hq.", .lead, .primary)), 31.5, accuracy: 0.5)
        XCTAssertEqual(measuredHeight(style, content: .richText(runs, .lead)), 31.5, accuracy: 0.5)
    }

    @MainActor
    func testChalkBackdropRemainsVisibleAgainstThePaperTheme() throws {
        let path = regularFont.deletingLastPathComponent().appendingPathComponent("../chalk-study.png").standardizedFileURL
        let image = try XCTUnwrap(NSImage(contentsOf: path))
        let color = Color(red: 32.0 / 255, green: 39.0 / 255, blue: 35.0 / 255)
        let board = BoardView(slice: "paper-backdrop-test", count: 0, lassoing: false, backdrop: image, panning: false, pan: { _ in }, stroke: { _ in }, lasso: { _ in })
            .environment(\.ospreyFadeBackdrop, true).frame(width: 1010, height: 676).background(color)
        let scene = ZStack { Color.white; board }.frame(width: 1010, height: 676)
        XCTAssertGreaterThan(brightPixels(scene), 500, "A light parent must not erase the chalk diagram on its dark working board")
    }

    @MainActor
    func testNonemptyCaptionKeepsItsAuthoredLineHeightWithTheBundledFont() throws {
        let geist = regularFont.deletingLastPathComponent().appendingPathComponent("Geist.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(geist as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(geist as CFURL, .process, nil) }
        let style = try decodedStyle(["font": "sans", "textSize": 13, "lineHeight": 22.75])
        XCTAssertEqual(measuredHeight(style, content: .text("Hypotheses", .caption, .secondary)), 22.75, accuracy: 0.01)
    }

    @MainActor
    func testDefaultSystemFaceKeepsAnAuthoredCaptionLineHeight() throws {
        let style = try decodedStyle(["textSize": 13, "lineHeight": 22.75])
        XCTAssertEqual(measuredHeight(style, content: .text("Hypotheses", .caption, .secondary)), 22.75, accuracy: 0.01)
    }

    @MainActor
    func testWrappedSourceMeasuresActualFirstAndLastBaselines() throws {
        let style = try decodedStyle(["font": "mono", "textSize": 14, "lineHeight": 26.6])
        for count in [9, 41] {
            let content = Node.richText([TextRun(text: Array(repeating: "exact hq", count: count).joined(separator: "\n"), ink: "", italic: false)], .mono)
            var distance: CGFloat = 0
            let view = BaselineProbe(receive: { distance = $0[.lastTextBaseline] - $0[.firstTextBaseline] }) {
                FramedView(style: style, content: content, drawing: Drawing(region: "baseline", revision: 1, send: { _, _ in }))
            }.fixedSize()
            let host = NSHostingView(rootView: view)
            host.layoutSubtreeIfNeeded()
            XCTAssertEqual(distance, (Double(count - 1) * 26.6).rounded(.up), accuracy: 0.01, "Native text must round the total baseline distance outward once, including forty intervals")
        }
    }

    @MainActor
    func testRepeatedSourceLinesKeepExactAuthoredBaselineSpacing() throws {
        let style = try decodedStyle(["font": "mono", "textSize": 14, "lineHeight": 26.6])
        let content = (0..<9).map { _ in Node.richText([TextRun(text: "exact hq", ink: "", italic: false)], .mono) }
        XCTAssertEqual(measuredHeight(style, content: .stack(.vertical, content)), 239.4, accuracy: 0.05, "Nine source lines must not accumulate fractional font-metric errors")
    }

    @MainActor
    func testEmptyCaptionDoesNotAddAnInvisibleLineToTheDocument() throws {
        let style = try decodedStyle(["font": "sans", "textSize": 13, "lineHeight": 22.75])
        XCTAssertEqual(measuredHeight(style, content: .text("", .caption, .secondary)), 0, accuracy: 0.01, "An empty caption must match the browser’s zero-height paragraph instead of pushing the manuscript beyond its window")
    }

    @MainActor
    func testGrowingFrameKeepsNaturalContentCenteredWhenItFits() throws {
        let style = try decodedStyle(["grow": true, "align": "center"])
        let child = Node.framed(try decodedStyle(["height": 40, "fill": "primary"]), .text("", .body, .primary))
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#000000", dark: "#000000")], spacing: [], feedback: 0, settle: 0)
        let view = FramedView(style: style, content: child, drawing: Drawing(region: "center", revision: 1, send: { _, _ in })).environment(\.ospreyPlacement, .pane).environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 200, height: 200).background(Color.white)
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
        XCTAssertLessThan(try XCTUnwrap(bitmap.colorAt(x: 100, y: 100)?.usingColorSpace(.sRGB)).redComponent, 0.1, "A fitting empty-state card must keep the frame’s authored center alignment")
    }

    @MainActor
    func testGrowingFrameScrollsOversizedNaturalContentWithinItsPane() throws {
        let style = try decodedStyle(["grow": true])
        let tall = Node.framed(try decodedStyle(["height": 900]), .text("A long mathematical document", .body, .primary))
        XCTAssertEqual(measuredHeight(style, content: tall), 700, accuracy: 0.01, "Natural content must scroll inside its bounded pane instead of pushing the app header off screen")
    }

    @MainActor
    func testNaturalFrameIncludesItsBorderInLayoutLikeTheBrowser() throws {
        let clear = try decodedStyle([:])
        let bordered = try decodedStyle(["line": "outline"])
        XCTAssertEqual(measuredHeight(bordered) - measuredHeight(clear), 2, accuracy: 0.01, "A one-point border must occupy one point on each side of the content")
    }

    private var regularFont: URL {
        TestResources.assets.appendingPathComponent("fonts/STIXTwoText.ttf")
    }

    @MainActor
    func testApplicationLineHeightControlsWrappedMathematicalTypography() throws {
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(regularFont as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(regularFont as CFURL, .process, nil) }
        let source = #"{"name":"title","width":369,"height":0,"top":0,"right":0,"bottom":0,"left":0,"radius":0,"gap":0,"fill":"","ink":"","line":"","grow":false,"font":"serif","textSize":29,"lineHeight":32.48}"#
        let style = try JSONDecoder().decode(FrameStyle.self, from: Data(source.utf8))
        let height = measuredHeight(style, content: .text("Commutativity\nof conjunction", .display, .primary))
        XCTAssertEqual(height, 64.96, accuracy: 0.55, "The two-line title must use the application’s leading, not the font’s default")
    }

    @MainActor
    func testMathematicsUsesScopedSlateInkInsteadOfAncestorForeground() throws {
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#ECEDE5", dark: "#ECEDE5")], spacing: [], feedback: 0, settle: 0)
        let view = MathView(tree: .sym("p"), size: 48)
            .environment(\.ospreyTheme, ThemeResolver(theme: theme))
            .foregroundStyle(Color.black)
            .frame(width: 100, height: 100).background(Color.black)
        XCTAssertGreaterThan(brightPixels(view), 20, "Scoped slate mathematics must remain legible over the dark surface")
    }

    @MainActor
    func testDisplayedFormulaUsesMathematicalInkHeightInsteadOfTextLineHeight() throws {
        let font = regularFont.deletingLastPathComponent().appendingPathComponent("STIXTwoMath-Regular.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let formula = MathTree.op("→", .op("∧", .sym("p"), .sym("q")), .op("∧", .sym("q"), .sym("p")))
        let style = try decodedStyle(["textSize": 48])
        let height = measuredHeight(style, content: .math(formula, spoken: "Conjunction commutes"))
        XCTAssertGreaterThan(height, 30)
        XCTAssertLessThan(height, 40, "The reference’s 48-point formula occupies its ink bounds, not a full text line")
    }

    @MainActor
    func testMathematicalVariableUsesItsItalicMathGlyph() throws {
        let font = regularFont.deletingLastPathComponent().appendingPathComponent("STIXTwoMath-Regular.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let face = CTFontCreateWithName("STIXTwoMath-Regular" as CFString, 48, nil)
        let reference = NSAttributedString(string: "𝑝", attributes: [NSAttributedString.Key(kCTFontAttributeName as String): face])
        let expected = CTLineGetTypographicBounds(CTLineCreateWithAttributedString(reference), nil, nil, nil)
        let host = NSHostingView(rootView: MathView(tree: .sym("p"), size: 48).fixedSize())
        XCTAssertEqual(host.fittingSize.width, expected.rounded(.up), accuracy: 0.01, "A mathematical variable must use the font’s mathematical italic alphabet")
    }

    @MainActor
    func testSharedMathematicsScalePreservesTheReferencesFractionalTypeSize() throws {
        let font = regularFont.deletingLastPathComponent().appendingPathComponent("STIXTwoMath-Regular.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let style = try decodedStyle(["textSize": 34, "mathScale": 1.08, "hug": true])
        let drawing = Drawing(region: "test", revision: 1, send: { _, _ in })
        let host = NSHostingView(rootView: FramedView(style: style, content: .math(.sym("p"), spoken: "p"), drawing: drawing).fixedSize())
        let face = CTFontCreateWithName("STIXTwoMath-Regular" as CFString, 36.72, nil)
        let reference = NSAttributedString(string: "𝑝", attributes: [NSAttributedString.Key(kCTFontAttributeName as String): face])
        let expected = CTLineGetTypographicBounds(CTLineCreateWithAttributedString(reference), nil, nil, nil).rounded(.up)
        XCTAssertEqual(host.fittingSize.width, expected, accuracy: 0.01, "The 1.08 scale must preserve 36.72 points instead of rounding the reference to 37")
    }

    @MainActor
    func testLogicalOperatorSpacingMatchesTheMeasuredReferenceFormula() throws {
        let font = regularFont.deletingLastPathComponent().appendingPathComponent("STIXTwoMath-Regular.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let formula = MathTree.op("→", .op("∧", .sym("p"), .sym("q")), .op("∧", .sym("q"), .sym("p")))
        let host = NSHostingView(rootView: MathView(tree: formula, size: 36.72).fixedSize())
        XCTAssertEqual(host.fittingSize.width, 217, accuracy: 0.5, "The reference’s MathML row measures 216.828 points; native layout rounds outward once")
    }

    @MainActor
    func testHypothesisColonUsesPunctuationSpacingRatherThanBinarySpacing() throws {
        let font = regularFont.deletingLastPathComponent().appendingPathComponent("STIXTwoMath-Regular.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let face = CTFontCreateWithName("STIXTwoMath-Regular" as CFString, 27, nil)
        let advances = ["hp", ":", "𝑝"].map { text in
            CTLineGetTypographicBounds(CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): face])), nil, nil, nil)
        }
        let host = NSHostingView(rootView: MathView(tree: .op(":", .sym("hp"), .sym("p")), size: 27).fixedSize())
        XCTAssertEqual(host.fittingSize.width, (advances.reduce(0, +) + 4.5).rounded(.up), accuracy: 0.01, "A binding colon has no space before it and a thin space after it")
    }

    @MainActor
    func testCompactSaveButtonKeepsItsLabelOnOneLine() throws { try saveLabel(pointSize: 12) }

    @MainActor
    func testVideoRendererPreservesTheAuthoredCaptionFile() throws {
        let node = Node.video(source: "lesson.mp4", poster: "poster.jpg", captions: "film.en.vtt", alt: "A proof.", seconds: 42)
        let drawn = RichView(node: node, drawing: Drawing(region: "lesson", revision: 1, send: { _, _ in })).body
        let film = try XCTUnwrap(firstFilm(drawn))
        let captions = Mirror(reflecting: film).children.first { $0.label == "captions" }?.value as? String
        XCTAssertEqual(captions, "film.en.vtt", "The native renderer must retain the same caption file that the browser enables")
    }

    @MainActor
    func testLandscapeLessonVideoPreservesItsSixteenByNineLayout() {
        let height = measuredHeight(view: FilmPlayer(player: AVPlayer(), cues: [], onFailure: { _ in }), width: 640)
        XCTAssertEqual(height, 360, accuracy: 0.5, "The shipped 1920×1080 films must not use a portrait player")
    }

    func testCaptionsFollowPlaybackIncludingExactCueBoundaries() throws {
        let source = "WEBVTT\n\n00:00.350 --> 00:01.500\nAssume h.\n\nsecond\n00:01.000 --> 00:02.000\nFrom hq.\nThen hp.\n"
        let cues = try FilmCaptions.parse(source)
        XCTAssertEqual(FilmCaptions.text(cues, at: 0), "")
        XCTAssertEqual(FilmCaptions.text(cues, at: 0.350), "Assume h.")
        XCTAssertEqual(FilmCaptions.text(cues, at: 1.0), "Assume h.\nFrom hq.\nThen hp.")
        XCTAssertEqual(FilmCaptions.text(cues, at: 1.5), "From hq.\nThen hp.")
        XCTAssertEqual(FilmCaptions.text(cues, at: 2), "")
    }

    @MainActor
    func testAFileThatCannotDecodeReportsAVisiblePlaybackFailure() throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("workbench-invalid-\(UUID().uuidString).mp4")
        try Data("not a movie".utf8).write(to: file)
        defer { do { try FileManager.default.removeItem(at: file) } catch { XCTFail("Cannot remove the test film: \(error)") } }
        let player = AVPlayer(url: file)
        var failure = ""
        let view = FilmPlayer(player: player, cues: [], onFailure: { failure = $0 })
        let host = NSHostingView(rootView: view.frame(width: 640, height: 360))
        host.layoutSubtreeIfNeeded()
        player.play()
        let deadline = Date(timeIntervalSinceNow: 3)
        while player.currentItem?.status != .failed && Date() < deadline { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05)) }
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        XCTAssertEqual(player.currentItem?.status, .failed, "The invalid fixture must actually fail to decode")
        XCTAssertFalse(failure.isEmpty, "An AVPlayer decoding failure must be reported beside the player")
        player.pause()
    }

    func testCaptionHeaderVariantsAndMathematicalTextAreReadable() throws {
        let source = "\u{FEFF}WEBVTT - Mathematics\n\n00:00.000 --> 00:02.000\np < q and q &lt; r &amp; p\n"
        let cues = try FilmCaptions.parse(source)
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
        let lessons = regularFont.deletingLastPathComponent().appendingPathComponent("../../lessons").standardizedFileURL
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

    private func declaredSeconds(_ file: URL) throws -> Int {
        let doc = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any])
        let blocks = try XCTUnwrap(doc["blocks"] as? [[String: Any]])
        let clip = try XCTUnwrap(blocks.first { $0["kind"] as? String == "video" })
        return try XCTUnwrap(clip["seconds"] as? Int)
    }

    private func firstFilm(_ value: Any) -> FilmView? {
        if let film = value as? FilmView { return film }
        return Mirror(reflecting: value).children.lazy.compactMap { self.firstFilm($0.value) }.first
    }

    @MainActor
    func testLinksRespectTheSharedFramesAuthoredTypeSize() throws {
        let url = try XCTUnwrap(URL(string: "https://ospreylang.dev/"))
        let link = Node.link(label: "Built with Osprey", url: url)
        let small = measuredHeight(try decodedStyle(["font": "sans", "textSize": 11]), content: link)
        let large = measuredHeight(try decodedStyle(["font": "sans", "textSize": 22]), content: link)
        XCTAssertGreaterThan(large, small * 1.5, "Footer links must inherit the same authored typography as adjacent text")
    }

    @MainActor
    func testLinksDoNotInventAnIconMissingFromTheSharedNode() throws {
        let label = "Built with Osprey"
        let url = try XCTUnwrap(URL(string: "https://ospreylang.dev/"))
        let style = try decodedStyle(["font": "sans", "textSize": 11, "hug": true])
        let drawing = Drawing(region: "footer", revision: 1, send: { _, _ in })
        let link = NodeView(node: .framed(style, .link(label: label, url: url)), drawing: drawing).fixedSize()
        let text = NodeView(node: .framed(style, .text(label, .caption, .primary)), drawing: drawing).fixedSize()
        let linkImage = try XCTUnwrap(ImageRenderer(content: link).cgImage)
        let textImage = try XCTUnwrap(ImageRenderer(content: text).cgImage)
        XCTAssertEqual(linkImage.width, textImage.width, "The protocol carries no link icon; native must match the browser's label-only link")
    }

    @MainActor
    func testStandaloneIconsRespectTheSharedFramesAuthoredSize() throws {
        let icon = Node.icon("chevron.left.forwardslash.chevron.right")
        let small = measuredHeight(try decodedStyle(["font": "sans", "textSize": 12]), content: icon)
        let large = measuredHeight(try decodedStyle(["font": "sans", "textSize": 24]), content: icon)
        XCTAssertGreaterThan(large, small * 1.5, "A source-heading icon must use its authored size rather than the generic 32-point fallback")
    }

    @MainActor
    func testMonospacedSourceUsesItsAuthoredLineHeight() throws {
        let style = try decodedStyle(["font": "mono", "textSize": 14, "lineHeight": 26.6])
        let height = measuredHeight(style, content: .text("namespace proofs\nexact hp", .mono, .primary))
        XCTAssertEqual(height, 53.2, accuracy: 0.5, "Two source lines must occupy the authored 26.6-point baselines")
    }

    @MainActor
    func testStatusBadgeKeepsTheReferencesTwentySevenPointHeight() throws {
        let style = try decodedStyle([:])
        let height = measuredHeight(style, content: .badge(label: "Checked", symbol: "", tone: .good))
        XCTAssertEqual(height, 27, accuracy: 0.1, "Changing a verdict must not change the document's vertical rhythm")
    }

    @MainActor
    func testTrailingSymbolsFollowTheirButtonLabel() throws {
        let style = try decodedStyle(["hug": true, "font": "sans", "textSize": 16, "trailingSymbols": true, "quietControls": true, "controlPadding": 0])
        let node = Node.button(label: "MMMMMMMM", symbol: "plus", role: .plain, enabled: true, event: 0)
        let drawing = Drawing(region: "test", revision: 1, send: { _, _ in })
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#000000", dark: "#000000")], spacing: [], feedback: 0, settle: 0)
        let view = FramedView(style: style, content: node, drawing: drawing).fixedSize().environment(\.ospreyTheme, ThemeResolver(theme: theme)).background(Color.white)
        let ink = try outerInk(view)
        XCTAssertGreaterThan(ink.0, ink.1, "The dense text must precede the smaller trailing plus symbol")
    }

    @MainActor
    func testBackdropFadeBlendsTheImageCornersIntoTheBoard() throws {
        let image = NSImage(size: NSSize(width: 100, height: 100))
        image.lockFocus()
        NSColor.white.setFill()
        NSRect(x: 0, y: 0, width: 100, height: 100).fill()
        image.unlockFocus()
        let board = BoardView(slice: "test-fade", count: 0, lassoing: false, backdrop: image, panning: false, pan: { _ in }, stroke: { _ in }, lasso: { _ in })
            .environment(\.ospreyFadeBackdrop, true).frame(width: 1010, height: 676).background(Color.black)
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: board).cgImage))
        let corner = try XCTUnwrap(bitmap.colorAt(x: 2, y: 10)?.usingColorSpace(.sRGB))
        let center = try XCTUnwrap(bitmap.colorAt(x: 505, y: 338)?.usingColorSpace(.sRGB))
        XCTAssertLessThan(corner.redComponent, 0.05, "The illustrative image's corner must fade into its board")
        XCTAssertGreaterThan(center.redComponent, 0.95, "The center of the illustrative image must remain opaque")
        let upperChalk = try XCTUnwrap(bitmap.colorAt(x: 190, y: 125)?.usingColorSpace(.sRGB))
        XCTAssertGreaterThan(upperChalk.redComponent, 0.95, "The reference's broad opaque plateau must preserve chalk near the upper-left proof")
    }

    @MainActor
    func testConstrainedButtonsDoNotBreakTheirLabelsIntoVerticalFragments() throws { try saveLabel(pointSize: 14) }

    @MainActor
    private func saveLabel(pointSize: Int) throws {
        let font = regularFont.deletingLastPathComponent().appendingPathComponent("Geist.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#000000", dark: "#000000")], spacing: [], feedback: 0, settle: 0)
        let header = try decodedStyle(["width": 200, "height": 75, "font": "sans", "textSize": pointSize, "align": "center", "quietControls": true, "controlHeight": 40])
        let button = try decodedStyle(["width": 82, "height": 38, "quietControls": true, "controlHeight": 38])
        let node = Node.framed(header, .stack(.horizontal, [.spacer, .framed(button, .button(label: "Save", symbol: "square.and.arrow.down", role: .plain, enabled: true, event: 0)), .spacer]))
        let view = NodeView(node: node, drawing: Drawing(region: "heading", revision: 1, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 200, height: 75).background(Color.white)
        let rows = try darkInkRows(view)
        XCTAssertGreaterThan(rows.count, 5, "The button label must render")
        XCTAssertLessThan((rows.last ?? 0) - (rows.first ?? 0), 20, "Save must not wrap inside the design’s 82-point button")
    }

    private func decodedStyle(_ values: [String: Any]) throws -> FrameStyle {
        try TestResources.frame(values)
    }

    @MainActor
    private func darkInkRows<V: View>(_ view: V) throws -> [Int] {
        let renderer = ImageRenderer(content: view)
        let image = try XCTUnwrap(renderer.cgImage)
        let bitmap = NSBitmapImageRep(cgImage: image)
        return (0..<bitmap.pixelsHigh).filter { y in
            (0..<bitmap.pixelsWide).contains { x in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.redComponent < 0.4 && color.greenComponent < 0.4 && color.blueComponent < 0.4
            }
        }
    }

    @MainActor
    private func brightPixels<V: View>(_ view: V) -> Int {
        let renderer = ImageRenderer(content: view)
        guard let image = renderer.cgImage else { XCTFail("The math snapshot must render"); return 0 }
        let bitmap = NSBitmapImageRep(cgImage: image)
        return (0..<bitmap.pixelsHigh).reduce(0) { count, y in
            count + (0..<bitmap.pixelsWide).filter { x in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.redComponent > 0.6 && color.greenComponent > 0.6 && color.blueComponent > 0.6
            }.count
        }
    }

    @MainActor
    private func outerInk<V: View>(_ view: V) throws -> (Int, Int) {
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
        return (0..<bitmap.pixelsWide).reduce((0, 0)) { totals, x in
            let count = (0..<bitmap.pixelsHigh).filter { y in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.redComponent < 0.4 && color.greenComponent < 0.4 && color.blueComponent < 0.4
            }.count
            return (totals.0 + (x < bitmap.pixelsWide / 3 ? count : 0), totals.1 + (x >= bitmap.pixelsWide * 2 / 3 ? count : 0))
        }
    }

    @MainActor
    private func measuredHeight(_ style: FrameStyle, content: Node = .stack(.vertical, [.text("Review", .body, .primary)])) -> CGFloat {
        let drawing = Drawing(region: "test", revision: 1, send: { _, _ in })
        return measuredHeight(view: FramedView(style: style, content: content, drawing: drawing), width: 529)
    }

    @MainActor
    private func measuredHeight<V: View>(view: V, width: CGFloat) -> CGFloat {
        var height: CGFloat = 0
        let root = ZStack(alignment: .top) {
            view.onGeometryChange(for: CGFloat.self, of: \.size.height) { height = $0 }
        }.frame(maxWidth: .infinity, maxHeight: .infinity).environment(\.ospreyPlacement, .pane)
        let host = NSHostingView(rootView: root)
        host.setFrameSize(NSSize(width: width, height: 700))
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
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
