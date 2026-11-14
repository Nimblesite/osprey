import AppKit
import SwiftUI
import XCTest

@testable import OspreyShared

// One more case beside each paragraph and editor layout test: other carets, other epochs,
// other lessons and a narrower serif paragraph.

extension ParagraphLayoutTests {
    @MainActor
    func testOpeningAppendedWorkRevealsTheAuthoredCaret_extraUnicodeTail() throws {
        let source = String(repeating: "Previous proof line.\n", count: 60) + "Theorem 𝑝→q ∧ α."
        let (host, window) = TestResources.hosted(editor("Previous proof."), width: 464, height: 160)
        defer { window.close() }
        host.rootView = editor(source)
        TestResources.settle()
        let text = try XCTUnwrap(descendant(host, NSTextView.self))
        let manager = try XCTUnwrap(text.layoutManager), container = try XCTUnwrap(text.textContainer)
        let end = manager.boundingRect(forGlyphRange: NSRange(location: (source as NSString).length - 1, length: 1), in: container).maxY + text.textContainerInset.height
        XCTAssertEqual(text.selectedRange().location, (source as NSString).length, "A UTF-8 end caret is the UTF-16 end")
        XCTAssertGreaterThanOrEqual(text.visibleRect.maxY, end, "A longer appended lesson with symbols must still reveal its tail")
    }

    @MainActor
    func testFirstAppearanceAppliesTheAuthoredUnicodeCaret_extraCaretAtTheStart() throws {
        for source in ["p→q", "𝑝→q", String(repeating: "Previous proof line.\n", count: 40) + "pq"] {
            var positions: [Int] = []
            let (host, window) = TestResources.hosted(editor(source, cursor: 0, place: { positions.append($0) }), width: 300, height: 100)
            defer { window.close() }
            let text = try XCTUnwrap(descendant(host, NSTextView.self))
            XCTAssertEqual(text.string, source)
            XCTAssertEqual(text.selectedRange().location, 0, "A caret authored at zero stays before the first symbol")
            XCTAssertTrue(positions.isEmpty, "Applying incoming state must not report a user caret movement")
            XCTAssertEqual(text.visibleRect.minY, 0, "The start caret keeps the first line in view")
        }
    }

    @MainActor
    func testAuthoredUtf8CaretLandsAfterTheImplicationSymbol_extraOnALaterLine() throws {
        let (host, window) = TestResources.hosted(editor("pq"), width: 300, height: 100)
        defer { window.close() }
        for (source, prefix) in [("p\nq→r", "p\nq→"), ("𝑝\n¬𝑞→r", "𝑝\n¬𝑞→"), ("α\nβ\nγ→δ", "α\nβ\nγ")] {
            host.rootView = editor(source, cursor: prefix.utf8.count)
            TestResources.settle(0.1)
            XCTAssertEqual(try XCTUnwrap(descendant(host, NSTextView.self)).selectedRange().location, prefix.utf16.count, source)
        }
    }

    @MainActor
    func testNativeSelectionReportsUtf8BytesForMathematicalSymbols_extraSelectionAtTheEnd() throws {
        var positions: [Int] = []
        let (host, window) = TestResources.hosted(editor("p→q", cursor: 0, place: { positions.append($0) }), width: 300, height: 100)
        defer { window.close() }
        for source in ["p→q", "𝑝→q", "α∧β→γ", "∀𝑥. 𝑥 = 𝑥"] {
            host.rootView = editor(source, cursor: 0, place: { positions.append($0) })
            TestResources.settle(0.1)
            let text = try XCTUnwrap(descendant(host, NSTextView.self))
            text.setSelectedRange(NSRange(location: (source as NSString).length, length: 0))
            TestResources.settle(0.1)
            XCTAssertEqual(positions.last, source.utf8.count, "The end caret is reported as every UTF-8 byte of \(source)")
        }
    }

    @MainActor
    func testOnlyAnAuthoredEpochAdvanceReturnsFocusToTheEditor_extraLargeJumps() throws {
        let (host, window) = try activeFocusHost(7)
        defer { window.close() }
        let other = try XCTUnwrap(descendant(host, NSTextField.self))
        let editorView = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        for epoch in [1_000, 9_007_199_254_740_991] {
            XCTAssertTrue(window.makeFirstResponder(other))
            try showEpoch(host, epoch)
            XCTAssertTrue(window.firstResponder === editorView, "An advance to \(epoch) must refocus the editor")
        }
        XCTAssertTrue(window.makeFirstResponder(other))
        try showEpoch(host, 9_007_199_254_740_991, value: "q")
        XCTAssertFalse(window.firstResponder === editorView, "The largest epoch replayed is not an advance")
    }

    @MainActor
    func testARecreatedEditorEstablishesItsFocusBaseline_extraEveryRecreationIsABaseline() throws {
        let (host, window) = try activeFocusHost(1)
        defer { window.close() }
        for epoch in [9, 50, 3] {
            host.rootView = EditorFocusFixture(node: Fixtures.text("Nothing to edit"))
            TestResources.settle(0.1)
            XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(descendant(host, NSTextField.self))))
            try showEpoch(host, epoch)
            let editorView = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
            XCTAssertFalse(window.firstResponder === editorView, "Recreated at \(epoch): a first appearance is a baseline")
        }
    }

    @MainActor
    func testChangingTheLessonRegionResetsTheEditorsFocusBaseline_extraThreeLessons() throws {
        let supplied = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        let (host, window) = TestResources.hosted(FocusRegionFixture(region: "lesson-a"), width: 400, height: 200, keyboard: true, suppliedWindow: supplied)
        defer { window.close() }
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(descendant(host, NSTextField.self))))
        for region in ["lesson-b", "lesson-c", "lesson-a", "lesson-c"] {
            host.rootView = FocusRegionFixture(region: region)
            TestResources.settle(0.2)
            let swapped = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
            XCTAssertEqual(swapped.string, "\(region)/formula")
            XCTAssertFalse(window.firstResponder === swapped, "\(region) starts at its own baseline")
        }
    }

    @MainActor
    func testAnEpochRequestNeverActivatesAnInactiveWindow_extraRepeatedAdvances() throws {
        let (host, window) = try activeFocusHost(0)
        defer { window.close() }
        let editorView = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(descendant(host, NSTextField.self))))
        try XCTUnwrap(window as? FocusTestWindow).keyState = false
        window.orderOut(nil)
        for epoch in 1...5 {
            try showEpoch(host, epoch, value: "p→q\(epoch)")
            XCTAssertFalse(window.isKeyWindow, "Advance \(epoch) must not activate the window")
            XCTAssertFalse(window.firstResponder === editorView)
        }
    }

    func testEditorFocusEpochRequiresAnExplicitBoundedIntegerWhenPresent_extraBoundaries() throws {
        for value in [1, 2, 4_503_599_627_370_496, 9_007_199_254_740_990] { XCTAssertEqual(Tree.number(Double(value)).whole, value, "Valid focus epoch: \(value)") }
        for value: Tree in [.number(.infinity), .number(-.infinity), .number(.nan), .number(0.5), .number(-9_007_199_254_740_991), .array([.number(1)]), .object(["epoch": .number(1)])] {
            XCTAssertNil(value.whole, "Invalid focus epoch: \(value)")
        }
        guard case var .object(members) = Fixtures.editor("p") else { return XCTFail("an editor is a record") }
        for epoch in [0, 3, 9_007_199_254_740_991] {
            members["focusEpoch"] = .number(Double(epoch))
            XCTAssertEqual(Tree.object(members)["focusEpoch"]?.whole, epoch, "\(epoch) is an epoch the editor honours")
        }
    }

    @MainActor
    func testMathematicalBodyUsesTheReferencesAvailableLineWidth_extraNarrowerParagraphWrapsSooner() throws {
        let words = "Review the mathematics before adding it to your document."
        let node = Fixtures.framed(["font": .string("serif"), "textSize": .number(19), "hug": .bool(true)], Fixtures.text(words))
        let style: [String: Tree] = ["width": .number(300), "font": .string("serif"), "textSize": .number(19), "lineHeight": .number(28.5)]
        let view = NodeView(node: Fixtures.framed(style, node), drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("text", "#242923")]))
            .frame(width: 464, height: 80, alignment: .topLeading).background(Color.white)
        let bitmap = try rendered(view)
        let edge = rightmostInk(bitmap)
        XCTAssertLessThanOrEqual(edge, 300, "A 300-point paragraph never inks past its width")
        XCTAssertLessThan(CGFloat(edge), referenceRightEdge() - 40, "The narrower paragraph must break its first line earlier than the reference")
        XCTAssertEqual(firstInkRow(bitmap), 4, "The authored line box still centers its leading")
    }
}
