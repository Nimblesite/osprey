import AppKit
import CoreText
import SwiftUI
import XCTest

@testable import OspreyShared

// Paragraph and editor layout: the authored UTF-8 caret, focus epochs that only an advance
// satisfies, per-region baselines, and the serif body's reference line width.

final class ParagraphLayoutTests: RenderingTestCase {
    override func setUpWithError() throws {
        try Repo.registerFonts(["STIXTwoText.ttf", "STIXTwoText-Italic.ttf"])
    }

    /// Opening appended work reveals its newly appended text beside its caret.
    @MainActor
    func testOpeningAppendedWorkRevealsTheAuthoredCaret() throws {
        let source = String(repeating: "Previous proof line.\n", count: 40) + "Theorem new_lesson."
        let (host, window) = TestResources.hosted(editor("Previous proof."), width: 464, height: 160)
        defer { window.close() }
        host.rootView = editor(source)
        TestResources.settle()
        let text = try XCTUnwrap(descendant(host, NSTextView.self))
        let manager = try XCTUnwrap(text.layoutManager)
        let container = try XCTUnwrap(text.textContainer)
        let end = manager.boundingRect(forGlyphRange: NSRange(location: (source as NSString).length - 1, length: 1), in: container).maxY + text.textContainerInset.height
        XCTAssertEqual(text.selectedRange().location, (source as NSString).length)
        XCTAssertGreaterThanOrEqual(text.visibleRect.maxY, end, "Opening a lesson must reveal its newly appended text beside that lesson's goals")
    }

    /// A new editor applies its authored UTF-8 caret before any edit or epoch advance.
    @MainActor
    func testFirstAppearanceAppliesTheAuthoredUnicodeCaret() throws {
        let previous = String(repeating: "Previous proof line.\n", count: 40)
        for (source, prefix) in [("p→q", "p→"), ("𝑝→q", "𝑝→"), (previous + "pq", previous + "p")] {
            var positions: [Int] = []
            let (host, window) = TestResources.hosted(editor(source, cursor: prefix.utf8.count, place: { positions.append($0) }), width: 300, height: 100)
            defer { window.close() }
            let text = try XCTUnwrap(descendant(host, NSTextView.self))
            XCTAssertEqual(text.string, source)
            XCTAssertEqual(text.selectedRange().location, prefix.utf16.count, "A new editor must apply its authored UTF-8 caret before any edit or epoch advance")
            XCTAssertTrue(positions.isEmpty, "Applying incoming state must not report a user caret movement")
            let manager = try XCTUnwrap(text.layoutManager), container = try XCTUnwrap(text.textContainer)
            let end = manager.boundingRect(forGlyphRange: NSRange(location: prefix.utf16.count - 1, length: 1), in: container).maxY + text.textContainerInset.height
            XCTAssertGreaterThanOrEqual(text.visibleRect.maxY, end, "The initial authored caret must be visible")
        }
    }

    /// An authored caret lands after the implication symbol, in UTF-8 as in UTF-16.
    @MainActor
    func testAuthoredUtf8CaretLandsAfterTheImplicationSymbol() throws {
        let (host, window) = TestResources.hosted(editor("pq"), width: 300, height: 100)
        defer { window.close() }
        for (source, prefix) in [("p→q", "p→"), ("𝑝→q", "𝑝→"), ("α∧β→γ", "α∧β")] {
            host.rootView = editor(source, cursor: prefix.utf8.count)
            TestResources.settle(0.1)
            XCTAssertEqual(try XCTUnwrap(descendant(host, NSTextView.self)).selectedRange().location, prefix.utf16.count)
        }
    }

    /// A native selection reports UTF-8 bytes for mathematical symbols.
    @MainActor
    func testNativeSelectionReportsUtf8BytesForMathematicalSymbols() throws {
        var positions: [Int] = []
        let (host, window) = TestResources.hosted(editor("p→q", cursor: 0, place: { positions.append($0) }), width: 300, height: 100)
        defer { window.close() }
        for (source, prefix) in [("p→q", "p→"), ("𝑝→q", "𝑝→"), ("α∧β→γ", "α∧β")] {
            host.rootView = editor(source, cursor: 0, place: { positions.append($0) })
            TestResources.settle(0.1)
            let text = try XCTUnwrap(descendant(host, NSTextView.self))
            text.setSelectedRange(NSRange(location: prefix.utf16.count, length: 0))
            TestResources.settle(0.1)
            XCTAssertEqual(positions.last, prefix.utf8.count)
        }
    }

    /// Only an authored epoch advance returns focus to the editor.
    @MainActor
    func testOnlyAnAuthoredEpochAdvanceReturnsFocusToTheEditor() throws {
        let (host, window) = try activeFocusHost(7)
        defer { window.close() }
        let other = try XCTUnwrap(descendant(host, NSTextField.self))
        let editorView = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertTrue(window.makeFirstResponder(other))
        XCTAssertTrue(window.isKeyWindow, "Focus restoration is tested in the active window")
        try showEpoch(host, 7, value: "p→q→p")
        XCTAssertFalse(window.firstResponder === editorView, "Ordinary text patches must not steal focus")
        try showEpoch(host, 8, value: "p→q→p")
        XCTAssertTrue(window.firstResponder === editorView, "A retained editor must honor an authored focus advance")
        XCTAssertTrue(window.makeFirstResponder(other))
        try showEpoch(host, 8)
        XCTAssertFalse(window.firstResponder === editorView, "An unchanged epoch must not refocus")
        try showEpoch(host, 6)
        XCTAssertFalse(window.firstResponder === editorView, "A decreased epoch must not refocus")
        try showEpoch(host, 8)
        XCTAssertFalse(window.firstResponder === editorView, "Replaying an already observed epoch must not refocus after a decrease")
    }

    /// A recreated editor establishes its focus baseline: a first appearance is not a request.
    @MainActor
    func testARecreatedEditorEstablishesItsFocusBaseline() throws {
        let (host, window) = try activeFocusHost(1)
        defer { window.close() }
        host.rootView = EditorFocusFixture(node: Fixtures.text("Nothing to edit"))
        TestResources.settle(0.1)
        let other = try XCTUnwrap(descendant(host, NSTextField.self))
        XCTAssertTrue(window.makeFirstResponder(other))
        try showEpoch(host, 9)
        let editorView = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertFalse(window.firstResponder === editorView, "A first appearance is a baseline, not a request")
    }

    /// A different lesson's editor starts at its own baseline, and its lower epoch still works.
    @MainActor
    func testChangingTheLessonRegionResetsTheEditorsFocusBaseline() throws {
        let supplied = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        var region = "lesson-a"
        let (host, window) = TestResources.hosted(FocusRegionFixture(region: region), width: 400, height: 200, keyboard: true, suppliedWindow: supplied)
        defer { window.close() }
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(descendant(host, NSTextField.self))))
        region = "lesson-b"
        host.rootView = FocusRegionFixture(region: region)
        TestResources.settle(0.2)
        let swapped = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertEqual(swapped.string, "lesson-b/formula")
        XCTAssertFalse(window.firstResponder === swapped, "A different lesson starts at its own baseline")
        region = "lesson-a"
        host.rootView = FocusRegionFixture(region: region)
        TestResources.settle(0.2)
        let returned = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertEqual(returned.string, "lesson-a/formula")
        XCTAssertTrue(window.makeFirstResponder(returned), "Lesson A epoch 1 must work after lesson B's baseline")
    }

    /// An epoch request never activates an inactive window.
    @MainActor
    func testAnEpochRequestNeverActivatesAnInactiveWindow() throws {
        let (host, window) = try activeFocusHost(0)
        defer { window.close() }
        let other = try XCTUnwrap(descendant(host, NSTextField.self))
        let editorView = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertTrue(window.makeFirstResponder(other))
        try XCTUnwrap(window as? FocusTestWindow).keyState = false
        window.orderOut(nil)
        XCTAssertFalse(window.isKeyWindow)
        try showEpoch(host, 1)
        XCTAssertFalse(window.isKeyWindow)
        XCTAssertFalse(window.firstResponder === editorView)
    }

    /// A focus epoch reaches the editor only as an explicit bounded whole number: told rows
    /// carry whole numbers as 64-bit values, reals as reals, and text is not an epoch at all.
    func testEditorFocusEpochRequiresAnExplicitBoundedIntegerWhenPresent() throws {
        for value in [0, 9_007_199_254_740_991] { XCTAssertEqual(Tree.number(Double(value)).whole, value, "Valid focus epoch: \(value)") }
        for value: Tree in [.null, "1", .bool(true), .number(-1), .number(1.5), .number(9_007_199_254_740_992)] {
            XCTAssertNil(value.whole, "Invalid focus epoch: \(value)")
        }
        let wire = FrameWire()
        wire.row(34, words: "", hint: "region", weight: 0)
        wire.row(33, parent: 0, words: "root", hint: "key", weight: 0)
        wire.row(34, parent: 0, words: "", hint: "node", weight: 0)
        wire.row(33, parent: 2, words: "", hint: "focusEpoch", weight: 1, colour: 8)
        let frame = try XCTUnwrap(Told.frame(try wire.read()))
        XCTAssertEqual(frame.regions[0].node["focusEpoch"], .number(8), "a told whole number is a bounded integer by construction")
        guard case var .object(members) = Fixtures.editor("p") else { return XCTFail("an editor is a record") }
        for epoch: Tree in ["later", "1", .bool(true), .number(-1), .number(1.5), .null] {
            members["focusEpoch"] = epoch
            XCTAssertNil(Tree.object(members)["focusEpoch"]?.whole, "\(epoch) is not an epoch; the editor must not request focus for it")
        }
    }

    /// The serif body keeps the reference's available line width and stroke weight.
    @MainActor
    func testMathematicalBodyUsesTheReferencesAvailableLineWidth() throws {
        let node = Fixtures.framed(["font": .string("serif"), "textSize": .number(19), "hug": .bool(true)], Fixtures.text("Review the mathematics before adding it to your document."))
        let style: [String: Tree] = ["width": .number(464), "font": .string("serif"), "textSize": .number(19), "lineHeight": .number(28.5)]
        let outer = Fixtures.framed(style, node)
        let view = NodeView(node: outer, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("text", "#242923")])).background(Color.white)
        let bitmap = try rendered(view)
        XCTAssertEqual(CGFloat(rightmostInk(bitmap)), referenceRightEdge(), accuracy: 2, "The reference’s first line ends with ‘your’, within its 464-point paragraph")
        XCTAssertEqual(inkCoverage(bitmap, bounds: NSRect(x: 0, y: 0, width: 464, height: 80)), try referenceCoverage(), accuracy: 75, "Regular mathematical prose must retain the reference’s stroke weight")
        XCTAssertEqual(firstInkRow(bitmap), 4, "The authored line box centers its leading around the text baseline")
    }

    // MARK: helpers

    /// One editor, at `cursor` UTF-8 bytes, reporting caret moves to `place`.
    func editor(_ value: String, cursor: Int? = nil, focusEpoch: Int = 0, place: @escaping (Int) -> Void = { _ in }) -> CodeText {
        CodeText(value: value, cursor: cursor ?? value.utf8.count, focusEpoch: focusEpoch, underlines: [], textSize: 20, inline: false,
                 edit: { _ in }, place: place)
    }

    func descendant<T: NSView>(_ view: NSView, _ type: T.Type) -> T? {
        (view as? T) ?? view.subviews.lazy.compactMap { self.descendant($0, type) }.first
    }

    /// An active window holding a plain field and the editor fixture at `epoch`.
    @MainActor
    func activeFocusHost(_ epoch: Int) throws -> (NSHostingView<EditorFocusFixture>, NSWindow) {
        let window = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        return TestResources.hosted(EditorFocusFixture(node: Fixtures.editor("p→q", focusEpoch: epoch)), width: 400, height: 200, keyboard: true, suppliedWindow: window)
    }

    /// Shows a fixture at `epoch`, waiting until the native editor applied it.
    @MainActor
    func showEpoch(_ host: NSHostingView<EditorFocusFixture>, _ epoch: Int, value: String = "p→q") throws {
        host.rootView = EditorFocusFixture(node: Fixtures.editor(value, focusEpoch: epoch))
        let deadline = Date(timeIntervalSinceNow: 2)
        repeat {
            host.layoutSubtreeIfNeeded()
            host.displayIfNeeded()
            let applied = TestResources.scrollViews(host).contains { scroll in
                guard let text = scroll.documentView as? NSTextView,
                      let coordinator = text.delegate as? CodeText.Coordinator else { return false }
                return coordinator.parent.focusEpoch == epoch && text.string == value
            }
            if applied {
                let settled = expectation(description: "Native focus queue drained")
                DispatchQueue.main.async { settled.fulfill() }
                wait(for: [settled], timeout: 2)
                return
            }
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.01))
        } while Date() < deadline
        XCTFail("Native editor did not apply focus epoch \(epoch) and text \(value)")
    }

    @MainActor
    func rendered<V: View>(_ view: V) throws -> NSBitmapImageRep {
        let host = NSHostingView(rootView: view.frame(width: 464, height: 80, alignment: .topLeading))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 464, height: 80), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        defer { window.close() }
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 464, pixelsHigh: 80, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 1856, bitsPerPixel: 32))
        host.cacheDisplay(in: host.bounds, to: bitmap)
        return try XCTUnwrap(NSBitmapImageRep(data: XCTUnwrap(bitmap.representation(using: .png, properties: [:]))))
    }

    func referenceRightEdge() -> CGFloat {
        let font = CTFontCreateWithName(FontRegistration.serifName as CFString, 19, nil)
        let text = NSAttributedString(string: "Review the mathematics before adding it to your", attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font])
        return CTLineGetBoundsWithOptions(CTLineCreateWithAttributedString(text), .useGlyphPathBounds).maxX
    }

    func rightmostInk(_ bitmap: NSBitmapImageRep) -> Int {
        (0..<bitmap.pixelsWide).filter { x in
            (0..<min(28, bitmap.pixelsHigh)).contains { y in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.redComponent < 0.4
            }
        }.max() ?? 0
    }

    func referenceCoverage() throws -> Double {
        let path = Repo.src.appendingPathComponent("../docs/designs/proof-workbench/screens/transcription.png").standardizedFileURL
        let decoded = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: path)))
        let bitmap = try XCTUnwrap(decoded.retagging(with: .sRGB))
        return inkCoverage(bitmap, bounds: NSRect(x: 967, y: 437, width: 464, height: 64))
    }

    func firstInkRow(_ bitmap: NSBitmapImageRep) -> Int {
        (0..<bitmap.pixelsHigh).first { y in
            (0..<bitmap.pixelsWide).contains { x in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.alphaComponent > 0.5 && color.redComponent < 0.4
            }
        } ?? bitmap.pixelsHigh
    }

    func inkCoverage(_ bitmap: NSBitmapImageRep, bounds: NSRect) -> Double {
        (Int(bounds.minX)..<Int(bounds.maxX)).reduce(0.0) { total, x in
            total + (Int(bounds.minY)..<Int(bounds.maxY)).reduce(0.0) { column, y in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return column }
                return column + color.alphaComponent * (1 - min(color.redComponent, min(color.greenComponent, color.blueComponent)))
            }
        }
    }
}

/// A plain field above one editor node.
struct EditorFocusFixture: View {
    let node: Tree

    var body: some View {
        VStack {
            TextField("Other input", text: .constant("Keep focus here"))
            NodeView(node: node, drawing: Fixtures.drawing("focus"))
        }
    }
}

/// The editor of one lesson's region; a different region is a different editor identity.
struct FocusRegionFixture: View {
    let region: String

    var body: some View {
        VStack {
            TextField("Other input", text: .constant("Keep focus here"))
            NodeView(node: Fixtures.editor("\(region)/formula", focusEpoch: 1), drawing: Fixtures.drawing(region)).id(region)
        }
    }
}
