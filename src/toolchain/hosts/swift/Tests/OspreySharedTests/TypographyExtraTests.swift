import AVKit
import AppKit
import CoreText
import SwiftUI
import XCTest

@testable import OspreyShared

// One more case beside each restored typography test: another size, edge, track or file.

extension TypographyTests {
    func testRegularSerifResolvesToTheBundledMathematicalFace_extraItalicAndMathFaces() throws {
        XCTAssertEqual(CTFontCopyPostScriptName(CTFontCreateWithName(FontRegistration.serifItalicName as CFString, 27, nil)) as String, "STIXTwoText-Italic")
        XCTAssertEqual(CTFontCopyPostScriptName(CTFontCreateWithName(FontRegistration.mathName as CFString, 27, nil)) as String, "STIXTwoMath-Regular")
    }

    @MainActor
    func testNaturalFrameKeepsItsContentHeightInsideAPane_extraDeeperInsets() throws {
        let shallow = measuredHeight(["width": 529, "top": 20, "bottom": 20])
        let deep = measuredHeight(["width": 529, "top": 40, "bottom": 40])
        XCTAssertEqual(deep - shallow, 40, accuracy: 0.5, "Insets add to the content, never the 700-point pane")
        XCTAssertLessThan(deep, 140)
    }

    @MainActor
    func testAsideDividerSpansTheTallerDocumentRatherThanOnlyTheGoal_extraTallerGoal() throws {
        let bitmap = try TestResources.rendered(asideView(document: 20, goal: 120))
        XCTAssertLessThan(try TestResources.pixel(bitmap, 280, 60).redComponent, 0.1, "The divider spans whichever column is taller")
        XCTAssertLessThan(try TestResources.pixel(bitmap, 280, 110).redComponent, 0.1)
    }

    @MainActor
    func testLayerKeepsAnOverlaidControlAtItsAuthoredTopInset_extraDeeperInset() throws {
        let bitmap = try TestResources.rendered(layered(top: 24, size: 32))
        for y in [23, 56] { XCTAssertGreaterThan(try TestResources.pixel(bitmap, 160, y).redComponent, 0.9, "Clear outside the control at y\(y)") }
        for y in [24, 55] { XCTAssertLessThan(try TestResources.pixel(bitmap, 160, y).redComponent, 0.1, "The control occupies y24 through y55") }
    }

    @MainActor
    func testActualMathematicalProseKeepsItsAuthoredSingleLineHeight_extraTwoLines() throws {
        let style: [String: Tree] = ["font": "serif", "textSize": 21, "lineHeight": .number(31.5)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("From hq.\nThen hp.", "lead")), 63, accuracy: 1)
        XCTAssertEqual(measuredHeight(style, content: rich([run("From "), run("hq", italic: true), run(".\nThen hp.")], "lead")), 63, accuracy: 1)
    }

    @MainActor
    func testChalkBackdropRemainsVisibleAgainstThePaperTheme_extraUnfadedPictureStaysWhole() throws {
        let plain = Image(nsImage: whiteSquare()).resizable().frame(width: 1010, height: 676).modifier(BackdropFade(enabled: false)).background(Color.black)
        let bitmap = try TestResources.rendered(plain)
        XCTAssertGreaterThan(try TestResources.pixel(bitmap, 2, 10).redComponent, 0.95, "Without the fade the picture's corner stays opaque")
        XCTAssertGreaterThan(try TestResources.pixel(bitmap, 1005, 670).redComponent, 0.95)
    }

    @MainActor
    func testNonemptyCaptionKeepsItsAuthoredLineHeightWithTheBundledFont_extraLargerSans() throws {
        let style: [String: Tree] = ["font": "sans", "textSize": 16, "lineHeight": .number(28)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("Propositions", "caption", "secondary")), 28, accuracy: 0.01)
    }

    @MainActor
    func testDefaultSystemFaceKeepsAnAuthoredCaptionLineHeight_extraBodyRole() throws {
        let style: [String: Tree] = ["textSize": 15, "lineHeight": .number(24)]
        // The forked Workbench host measured 25 here, its rounded line box; the shared one must match it.
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("Hypotheses", "body")), 25, accuracy: 0.01)
    }

    @MainActor
    func testWrappedSourceMeasuresActualFirstAndLastBaselines_extraTwoLines() throws {
        XCTAssertEqual(baselineDistance(lines: 2), 27, accuracy: 0.01, "One 26.6-point interval rounds outward once")
    }

    @MainActor
    func testRepeatedSourceLinesKeepExactAuthoredBaselineSpacing_extraFourLines() throws {
        let lines = (0..<4).map { _ in rich([run("exact hq")], "mono") }
        XCTAssertEqual(measuredHeight(sourceStyle, content: column(lines)), 106.4, accuracy: 0.05)
    }

    @MainActor
    func testEmptyCaptionDoesNotAddAnInvisibleLineToTheDocument_extraEmptySerifBody() throws {
        let style: [String: Tree] = ["font": "serif", "textSize": 19, "lineHeight": .number(28.5)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("", "body")), 0, accuracy: 0.01)
    }

    @MainActor
    func testGrowingFrameKeepsNaturalContentCenteredWhenItFits_extraEndAlignment() throws {
        let child = Fixtures.framed(["height": 40, "fill": "ink"], Fixtures.text(""))
        let view = framedView(["grow": true, "align": "end"], child, theme: [("ink", "#000000")]).frame(width: 200, height: 200).background(Color.white)
        let bitmap = try TestResources.rendered(view)
        XCTAssertLessThan(try TestResources.pixel(bitmap, 100, 180).redComponent, 0.1, "An end-aligned card sits at the bottom")
        XCTAssertGreaterThan(try TestResources.pixel(bitmap, 100, 100).redComponent, 0.9)
    }

    @MainActor
    func testGrowingFrameScrollsOversizedNaturalContentWithinItsPane_extraOnlyOversizedContentScrolls() throws {
        for (height, scrolls) in [(300, false), (900, true)] {
            let content = Fixtures.framed(["height": .number(Double(height))], Fixtures.text("A document"))
            let (host, window) = TestResources.hosted(framedView(["grow": true], content), width: 529, height: 700)
            defer { window.close() }
            XCTAssertEqual(!TestResources.scrollViews(host).isEmpty, scrolls, "\(height) points in a 700-point pane")
        }
    }

    @MainActor
    func testNaturalFrameIncludesItsBorderInLayoutLikeTheBrowser_extraOneSideEdge() throws {
        XCTAssertEqual(measuredHeight(["line": "outline", "lineEdge": "left"]) - measuredHeight([:]), 0, accuracy: 0.01, "A left edge adds no height")
        XCTAssertEqual(measuredHeight(["line": "outline", "lineEdge": "top"]) - measuredHeight([:]), 1, accuracy: 0.01, "A top edge adds one point")
    }

    @MainActor
    func testApplicationLineHeightControlsWrappedMathematicalTypography_extraThreeLines() throws {
        let style: [String: Tree] = ["width": 369, "font": "serif", "textSize": 29, "lineHeight": .number(32.48)]
        XCTAssertEqual(measuredHeight(style, content: Fixtures.text("Commutativity\nof\nconjunction", "display")), 97.44, accuracy: 0.8)
    }

    @MainActor
    func testMathematicsUsesScopedSlateInkInsteadOfAncestorForeground_extraPaperInk() throws {
        let view = framedView(["textSize": 48, "hug": true], Fixtures.math(Fixtures.mathSym("p")), theme: [("text", "#242923")])
            .foregroundStyle(Color.white).frame(width: 100, height: 100).background(Color.white)
        let dark = TestResources.pixels(try TestResources.rendered(view)) { $0.redComponent < 0.4 && $0.greenComponent < 0.4 && $0.blueComponent < 0.4 }
        XCTAssertGreaterThan(dark, 20, "Paper mathematics keeps its dark ink under a white ancestor")
    }

    @MainActor
    func testDisplayedFormulaUsesMathematicalInkHeightInsteadOfTextLineHeight_extraHalfSize() throws {
        let height = measuredHeight(["textSize": 24], content: Fixtures.math(commutes, spoken: "Conjunction commutes"))
        XCTAssertGreaterThan(height, 15)
        XCTAssertLessThan(height, 20, "Half the size, half the ink height")
    }

    @MainActor
    func testMathematicalVariableUsesItsItalicMathGlyph_extraSecondVariable() throws {
        let host = NSHostingView(rootView: MathView(node: Fixtures.mathSym("q"), size: 48).fixedSize())
        XCTAssertEqual(host.fittingSize.width, mathAdvance(["𝑞"], size: 48).rounded(.up), accuracy: 0.01)
    }

    @MainActor
    func testSharedMathematicsScalePreservesTheReferencesFractionalTypeSize_extraNoScale() throws {
        let host = NSHostingView(rootView: framedView(["textSize": 34, "hug": true], Fixtures.math(Fixtures.mathSym("p"), spoken: "p")).fixedSize())
        XCTAssertEqual(host.fittingSize.width, mathAdvance(["𝑝"], size: 34).rounded(.up), accuracy: 0.01, "No scale keeps the authored size")
    }

    @MainActor
    func testLogicalOperatorSpacingMatchesTheMeasuredReferenceFormula_extraHalfSize() throws {
        let host = NSHostingView(rootView: MathView(node: commutes, size: 18.36).fixedSize())
        XCTAssertEqual(host.fittingSize.width, 108.5, accuracy: 1, "Operator spacing scales with the type")
    }

    @MainActor
    func testHypothesisColonUsesPunctuationSpacingRatherThanBinarySpacing_extraConjunctionTakesBinarySpacing() throws {
        let host = NSHostingView(rootView: MathView(node: Fixtures.mathOp("∧", Fixtures.mathSym("p"), Fixtures.mathSym("q")), size: 27).fixedSize())
        XCTAssertEqual(host.fittingSize.width, (mathAdvance(["𝑝", "∧", "𝑞"], size: 27) + 12).rounded(.up), accuracy: 1, "A binary operator has a medium space on each side")
    }

    @MainActor
    func testCompactSaveButtonKeepsItsLabelOnOneLine_extraThirteenPoints() throws { try saveLabel(pointSize: 13) }

    @MainActor
    func testConstrainedButtonsDoNotBreakTheirLabelsIntoVerticalFragments_extraTenPoints() throws { try saveLabel(pointSize: 10) }

    @MainActor
    func testVideoRendererPreservesTheAuthoredCaptionFile_extraAnotherTrack() throws {
        guard case let .view(leaf) = drawnLeaf(film(captions: "film.fr.vtt")) else { return XCTFail("A video must draw as the film leaf") }
        XCTAssertEqual(leaf.node.text("captions"), "film.fr.vtt")
        XCTAssertEqual(leaf.node.text("alt"), "A proof.")
    }

    @MainActor
    func testLandscapeLessonVideoPreservesItsSixteenByNineLayout_extraNarrowPane() {
        let size = TestResources.measured(FilmPlayer(player: AVPlayer(), cues: [], onFailure: { _ in }).fixedSize(horizontal: false, vertical: true), width: 320, height: 700)
        XCTAssertEqual(size.height, 180, accuracy: 0.5)
    }

    func testCaptionsFollowPlaybackIncludingExactCueBoundaries_extraHourTimestamps() throws {
        let cues = try FilmCaptions.parse("WEBVTT\n\n01:00:00.000 --> 01:00:01.000\nLate\n")
        XCTAssertEqual(FilmCaptions.text(cues, at: 3599.9), "")
        XCTAssertEqual(FilmCaptions.text(cues, at: 3600), "Late")
        XCTAssertEqual(FilmCaptions.text(cues, at: 3601), "")
    }

    @MainActor
    func testAFileThatCannotDecodeReportsAVisiblePlaybackFailure_extraTextFileNamedAsAMovie() throws {
        let (player, failure) = try undecodable()
        XCTAssertNotEqual(player.currentItem?.status, .readyToPlay)
        XCTAssertTrue(failure.isEmpty == false || player.currentItem?.error != nil, "Either the player or AVFoundation names the failure")
    }

    func testCaptionHeaderVariantsAndMathematicalTextAreReadable_extraWindowsLineEndsAndTabHeader() throws {
        let cues = try FilmCaptions.parse("WEBVTT\tTabbed\r\n\r\n00:00.000 --> 00:01.000\r\n&gt; q &nbsp;r\r\n")
        XCTAssertEqual(FilmCaptions.text(cues, at: 0.5), "> q \u{00A0}r")
    }

    func testMissingAndMalformedCaptionTracksFailExplicitly_extraAbsoluteAndNotePaths() {
        XCTAssertThrowsError(try FilmCaptions.load("/etc/hosts"))
        XCTAssertThrowsError(try FilmCaptions.load(""))
        XCTAssertThrowsError(try FilmCaptions.parse("WEBVTT\n\nNOTE only a note\n"), "A track of notes has no cues")
        XCTAssertThrowsError(try FilmCaptions.parse("WEBVTT\n\n00:00.000 --> 00:01.000\n"), "A cue with no words")
    }

    func testEveryShippedLessonFilmMeetsCaptionAndByteLimits_extraPostersShip() throws {
        for slug in ["what-is-a-proof", "what-is-a-solver", "navier-stokes"] {
            let poster = lessons.appendingPathComponent("media/\(slug)/poster.jpg")
            XCTAssertNotNil(NSImage(contentsOf: poster), "\(slug) ships a decodable poster")
            let size = try XCTUnwrap(FileManager.default.attributesOfItem(atPath: poster.path)[.size] as? NSNumber)
            XCTAssertLessThanOrEqual(size.intValue, 2_000_000, slug)
        }
    }

    @MainActor
    func testLinksRespectTheSharedFramesAuthoredTypeSize_extraSerif() throws {
        let small = measuredHeight(["font": "serif", "textSize": 11], content: link)
        let large = measuredHeight(["font": "serif", "textSize": 22], content: link)
        XCTAssertGreaterThan(large, small * 1.5)
    }

    @MainActor
    func testLinksDoNotInventAnIconMissingFromTheSharedNode_extraLargerType() throws {
        let style: [String: Tree] = ["font": "sans", "textSize": 18, "hug": true]
        let linked = try XCTUnwrap(ImageRenderer(content: framedView(style, link).fixedSize()).cgImage)
        let plain = try XCTUnwrap(ImageRenderer(content: framedView(style, Fixtures.text("Built with Osprey", "caption")).fixedSize()).cgImage)
        XCTAssertEqual(linked.width, plain.width)
    }

    @MainActor
    func testStandaloneIconsRespectTheSharedFramesAuthoredSize_extraSymbolSizeWins() throws {
        let icon = Tree.object(["kind": "icon", "symbol": "chevron.left.forwardslash.chevron.right"])
        let text = measuredHeight(["font": "sans", "textSize": 12], content: icon)
        let symbol = measuredHeight(["font": "sans", "textSize": 12, "symbolSize": 30], content: icon)
        XCTAssertGreaterThan(symbol, text * 1.5, "An authored symbol size outranks the text size")
    }

    @MainActor
    func testMonospacedSourceUsesItsAuthoredLineHeight_extraThreeLines() throws {
        // Three authored 26.6-point lines, as the forked Workbench host measured them: 80.6.
        XCTAssertEqual(measuredHeight(sourceStyle, content: Fixtures.text("namespace proofs\nexact hp\nqed", "mono")), 80.6, accuracy: 0.05)
    }

    @MainActor
    func testStatusBadgeKeepsTheReferencesTwentySevenPointHeight_extraWithSymbolAndOtherTones() throws {
        for tone in ["good", "danger", "caution"] {
            XCTAssertEqual(measuredHeight([:], content: badge("Checked", tone: tone, symbol: "checkmark.seal")), 27, accuracy: 0.1, tone)
        }
    }

    @MainActor
    func testTrailingSymbolsFollowTheirButtonLabel_extraWideLetters() throws {
        let ink = try outerInk(trailingButton("WWWWWW"))
        XCTAssertGreaterThan(ink.0, ink.1)
    }

    @MainActor
    func testBackdropFadeBlendsTheImageCornersIntoTheBoard_extraFarCorner() throws {
        let bitmap = try TestResources.rendered(faded(whiteSquare()).background(Color.black))
        XCTAssertLessThan(try TestResources.pixel(bitmap, 1007, 670).redComponent, 0.05, "The far corner fades too")
        XCTAssertLessThan(try TestResources.pixel(bitmap, 900, 338).redComponent, try TestResources.pixel(bitmap, 505, 338).redComponent, "The right side fades towards its edge")
    }
}
