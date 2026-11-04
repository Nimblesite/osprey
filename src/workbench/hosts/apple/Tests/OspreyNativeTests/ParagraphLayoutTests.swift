import AppKit
import CoreText
import SwiftUI
import XCTest
@testable import OspreyNative

final class ParagraphLayoutTests: XCTestCase {
    @MainActor
    func testOpeningAppendedWorkRevealsTheAuthoredCaret() throws {
        let source = String(repeating: "Previous proof line.\n", count: 40) + "Theorem new_lesson."
        let host = NSHostingView(rootView: editor("Previous proof."))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 464, height: 160), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        defer { window.close() }
        host.layoutSubtreeIfNeeded()
        host.rootView = editor(source)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        let text = try XCTUnwrap(descendant(host, type: NSTextView.self))
        let manager = try XCTUnwrap(text.layoutManager)
        let container = try XCTUnwrap(text.textContainer)
        let end = manager.boundingRect(forGlyphRange: NSRange(location: (source as NSString).length - 1, length: 1), in: container).maxY + text.textContainerInset.height
        XCTAssertEqual(text.selectedRange().location, (source as NSString).length)
        XCTAssertGreaterThanOrEqual(text.visibleRect.maxY, end, "Opening a lesson must reveal its newly appended text beside that lesson’s goals")
    }

    @MainActor
    func testFirstAppearanceAppliesTheAuthoredUnicodeCaret() throws {
        let previous = String(repeating: "Previous proof line.\n", count: 40)
        for (source, prefix) in [("p→q", "p→"), ("𝑝→q", "𝑝→"), (previous + "pq", previous + "p")] {
            var positions: [Int] = []
            let (host, window) = TestResources.hosted(editor(source, cursor: prefix.utf8.count, place: { positions.append($0) }), width: 300, height: 100)
            defer { window.close() }
            let text = try XCTUnwrap(descendant(host, type: NSTextView.self))
            XCTAssertEqual(text.string, source)
            XCTAssertEqual(text.selectedRange().location, prefix.utf16.count, "A new editor must apply its authored UTF-8 caret before any edit or epoch advance")
            XCTAssertTrue(positions.isEmpty, "Applying incoming state must not report a user caret movement")
            let manager = try XCTUnwrap(text.layoutManager), container = try XCTUnwrap(text.textContainer)
            let end = manager.boundingRect(forGlyphRange: NSRange(location: prefix.utf16.count - 1, length: 1), in: container).maxY + text.textContainerInset.height
            XCTAssertGreaterThanOrEqual(text.visibleRect.maxY, end, "The initial authored caret must be visible")
        }
    }

    @MainActor
    func testAuthoredUtf8CaretLandsAfterTheImplicationSymbol() throws {
        let (host, window) = TestResources.hosted(editor("pq"), width: 300, height: 100)
        defer { window.close() }
        for (source, prefix) in [("p→q", "p→"), ("𝑝→q", "𝑝→"), ("α∧β→γ", "α∧β")] {
            host.rootView = editor(source, cursor: prefix.utf8.count)
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
            XCTAssertEqual(try XCTUnwrap(descendant(host, type: NSTextView.self)).selectedRange().location, prefix.utf16.count)
        }
    }

    @MainActor
    func testNativeSelectionReportsUtf8BytesForMathematicalSymbols() throws {
        var positions: [Int] = []
        let (host, window) = TestResources.hosted(editor("p→q", cursor: 0, place: { positions.append($0) }), width: 300, height: 100)
        defer { window.close() }
        for (source, prefix) in [("p→q", "p→"), ("𝑝→q", "𝑝→"), ("α∧β→γ", "α∧β")] {
            host.rootView = editor(source, cursor: 0, place: { positions.append($0) })
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
            let text = try XCTUnwrap(descendant(host, type: NSTextView.self))
            text.setSelectedRange(NSRange(location: prefix.utf16.count, length: 0))
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
            XCTAssertEqual(positions.last, prefix.utf8.count)
        }
    }

    private func focusNode(_ epoch: Any? = nil, value: String = "p→q") throws -> Node {
        var doc: [String: Any] = ["kind": "editor", "label": "Formula", "value": value, "inline": true, "cursor": value.utf8.count, "marks": [], "event": 0, "placeEvent": 1]
        if let epoch { doc["focusEpoch"] = epoch }
        return try JSONDecoder().decode(Node.self, from: JSONSerialization.data(withJSONObject: doc))
    }

    @MainActor
    private func showEpoch(_ host: NSHostingView<EditorFocusFixture>, _ epoch: Int, value: String = "p→q") throws {
        host.rootView = EditorFocusFixture(node: try focusNode(epoch, value: value))
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
    }

    @MainActor
    private func activeFocusHost(_ epoch: Int) throws -> (NSHostingView<EditorFocusFixture>, NSWindow) {
        FontRegistration.configureTextRendering()
        let window = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        return TestResources.hosted(EditorFocusFixture(node: try focusNode(epoch)), width: 400, height: 200, suppliedWindow: window)
    }

    @MainActor
    private func showRegion(_ host: NSHostingView<FocusRegionFixture>, key: String) throws -> NSTextView {
        host.rootView = FocusRegionFixture(key: key, store: host.rootView.store)
        host.layoutSubtreeIfNeeded()
        host.displayIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        return try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
    }

    @MainActor
    func testOnlyAnAuthoredEpochAdvanceReturnsFocusToTheEditor() throws {
        let (host, window) = try activeFocusHost(7)
        defer { window.close() }
        let other = try XCTUnwrap(descendant(host, type: NSTextField.self))
        let editor = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertTrue(window.makeFirstResponder(other))
        XCTAssertTrue(window.isKeyWindow, "Focus restoration is tested in the active window")
        try showEpoch(host, 7, value: "p→q→p")
        XCTAssertFalse(window.firstResponder === editor, "Ordinary text patches must not steal focus")
        try showEpoch(host, 8, value: "p→q→p")
        XCTAssertTrue(window.firstResponder === editor, "A retained editor must honor an authored focus advance")
        XCTAssertTrue(window.makeFirstResponder(other))
        try showEpoch(host, 8)
        XCTAssertFalse(window.firstResponder === editor, "An unchanged epoch must not refocus")
        try showEpoch(host, 6)
        XCTAssertFalse(window.firstResponder === editor, "A decreased epoch must not refocus")
        try showEpoch(host, 8)
        XCTAssertFalse(window.firstResponder === editor, "Replaying an already observed epoch must not refocus after a decrease")
    }

    @MainActor
    func testARecreatedEditorEstablishesItsFocusBaseline() throws {
        let (host, window) = try activeFocusHost(1)
        defer { window.close() }
        host.rootView = EditorFocusFixture(node: .spacer)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        let other = try XCTUnwrap(descendant(host, type: NSTextField.self))
        XCTAssertTrue(window.makeFirstResponder(other))
        try showEpoch(host, 9)
        let editor = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertFalse(window.firstResponder === editor, "A first appearance is a baseline, not a request")
    }

    @MainActor
    func testChangingTheLessonRegionResetsTheEditorsFocusBaseline() throws {
        FontRegistration.configureTextRendering()
        let store = RegionStore(library: Library(entry: LibraryEntry(main: focusRegionMain, start: focusRegionStart, dispatch: focusRegionDispatch)), context: HostContext(platform: "test", home: ""), capabilities: CapabilityRunner(adapters: []))
        store.start()
        let window = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        let (host, _) = TestResources.hosted(FocusRegionFixture(key: "lesson-a/formula", store: store), width: 400, height: 200, suppliedWindow: window)
        defer { window.close() }
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(descendant(host, type: NSTextField.self))))
        let editor = try showRegion(host, key: "lesson-b/formula")
        XCTAssertEqual(editor.string, "lesson-b/formula")
        XCTAssertFalse(window.firstResponder === editor, "A different lesson starts at its own baseline")
        store.lifecycle("active")
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        XCTAssertNil(store.failure)
        XCTAssertTrue(editor.window === window)
        XCTAssertTrue(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.contains { window.firstResponder === $0 }, "The current lesson editor must receive focus")
        XCTAssertTrue(window.firstResponder === editor, "Lesson B epoch 1 must work after lesson A epoch 8")
    }

    @MainActor
    func testAnEpochRequestNeverActivatesAnInactiveWindow() throws {
        let (host, window) = try activeFocusHost(0)
        defer { window.close() }
        let other = try XCTUnwrap(descendant(host, type: NSTextField.self))
        let editor = try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
        XCTAssertTrue(window.makeFirstResponder(other))
        try XCTUnwrap(window as? FocusTestWindow).keyState = false
        window.orderOut(nil)
        XCTAssertFalse(window.isKeyWindow)
        try showEpoch(host, 1)
        XCTAssertFalse(window.isKeyWindow)
        XCTAssertFalse(window.firstResponder === editor)
    }

    func testEditorFocusEpochRequiresAnExplicitBoundedIntegerWhenPresent() throws {
        XCTAssertNoThrow(try focusNode())
        for value in [0, 9_007_199_254_740_991] { XCTAssertNoThrow(try focusNode(value)) }
        for value: Any in [NSNull(), "1", true, -1, 1.5, 9_007_199_254_740_992] {
            XCTAssertThrowsError(try focusNode(value), "Invalid focus epoch: \(value)")
        }
    }

    private func editor(_ value: String, cursor: Int? = nil, place: @escaping (Int) -> Void = { _ in }) -> CodeText {
        CodeText(value: value, cursor: cursor ?? value.utf8.count, underlines: [], textSize: 20, inline: false, ink: .black, edit: { _ in }, place: place)
    }

    private func descendant<T: NSView>(_ view: NSView, type: T.Type) -> T? {
        (view as? T) ?? view.subviews.lazy.compactMap { self.descendant($0, type: type) }.first
    }

    @MainActor
    func testMathematicalBodyUsesTheReferencesAvailableLineWidth() throws {
        let font = TestResources.assets.appendingPathComponent("fonts/STIXTwoText.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let style = try TestResources.frame(["width": 464, "font": "serif", "textSize": 19, "lineHeight": 28.5])
        let inner = try TestResources.frame(["font": "serif", "textSize": 19, "hug": true])
        let node = Node.framed(inner, .text("Review the mathematics before adding it to your document.", .body, .primary))
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#242923", dark: "#242923")], spacing: [], feedback: 0, settle: 0)
        let view = FramedView(style: style, content: node, drawing: Drawing(region: "paragraph", revision: 1, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).background(Color.white)
        let bitmap = try rendered(view)
        XCTAssertEqual(CGFloat(rightmostInk(bitmap)), referenceRightEdge(), accuracy: 2, "The reference’s first line ends with ‘your’, within its 464-point paragraph")
        XCTAssertEqual(inkCoverage(bitmap, bounds: NSRect(x: 0, y: 0, width: 464, height: 80)), try referenceCoverage(), accuracy: 75, "Regular mathematical prose must retain the reference’s stroke weight")
        XCTAssertEqual(firstInkRow(bitmap), 4, "The authored line box centers its leading around the text baseline")
    }

    @MainActor
    private func rendered<V: View>(_ view: V) throws -> NSBitmapImageRep {
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

    private func referenceRightEdge() -> CGFloat {
        let font = CTFontCreateWithName("STIXTwoText" as CFString, 19, nil)
        let text = NSAttributedString(string: "Review the mathematics before adding it to your", attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font])
        return CTLineGetBoundsWithOptions(CTLineCreateWithAttributedString(text), .useGlyphPathBounds).maxX
    }

    private func rightmostInk(_ bitmap: NSBitmapImageRep) -> Int {
        (0..<bitmap.pixelsWide).filter { x in
            (0..<min(28, bitmap.pixelsHigh)).contains { y in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.redComponent < 0.4
            }
        }.max() ?? 0
    }

    private func referenceCoverage() throws -> Double {
        let path = TestResources.apple.appendingPathComponent("../../../../docs/designs/proof-workbench/screens/transcription.png")
        let decoded = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: path)))
        let bitmap = try XCTUnwrap(decoded.retagging(with: .sRGB))
        return inkCoverage(bitmap, bounds: NSRect(x: 967, y: 437, width: 464, height: 64))
    }

    private func firstInkRow(_ bitmap: NSBitmapImageRep) -> Int {
        (0..<bitmap.pixelsHigh).first { y in
            (0..<bitmap.pixelsWide).contains { x in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return color.alphaComponent > 0.5 && color.redComponent < 0.4
            }
        } ?? bitmap.pixelsHigh
    }

    private func inkCoverage(_ bitmap: NSBitmapImageRep, bounds: NSRect) -> Double {
        (Int(bounds.minX)..<Int(bounds.maxX)).reduce(0.0) { total, x in
            total + (Int(bounds.minY)..<Int(bounds.maxY)).reduce(0.0) { column, y in
                guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return column }
                return column + color.alphaComponent * (1 - min(color.redComponent, min(color.greenComponent, color.blueComponent)))
            }
        }
    }
}

private struct EditorFocusFixture: View {
    let node: Node

    var body: some View {
        VStack {
            TextField("Other input", text: .constant("Keep focus here"))
            NodeView(node: node, drawing: Drawing(region: "focus", revision: 1, send: { _, _ in }))
        }
    }
}

private struct FocusRegionFixture: View {
    let key: String
    let store: RegionStore
    var body: some View {
        VStack {
            TextField("Other input", text: .constant("Keep focus here"))
            RegionView(key: key).environmentObject(store)
        }
    }
}

private enum FocusRegionWire {
    static let initial = envelope([(key: "lesson-a/formula", epoch: 8), (key: "lesson-b/formula", epoch: 0)])
    static let changed = envelope([(key: "lesson-b/formula", epoch: 1)])

    static func envelope(_ items: [(key: String, epoch: Int)]) -> NSString? {
        let patches: [[String: Any]] = items.map { item in
            ["key": item.key, "revision": item.epoch + 1, "node": ["kind": "editor", "label": "Formula", "value": item.key, "inline": true, "cursor": item.key.utf8.count, "marks": [], "event": 0, "placeEvent": 1, "focusEpoch": item.epoch]]
        }
        let document: [String: Any] = ["version": 1, "state": "focus-fixture", "patches": patches, "removed": [], "requests": [], "slices": [], "diagnostics": []]
        guard let bytes = try? JSONSerialization.data(withJSONObject: document) else { return nil }
        return String(data: bytes, encoding: .utf8).map { $0 as NSString }
    }
}

private func focusRegionMain() -> Int32 { 0 }
private func focusRegionStart(_ context: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { FocusRegionWire.initial?.utf8String }
private func focusRegionDispatch(_ state: UnsafePointer<CChar>?, _ event: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { FocusRegionWire.changed?.utf8String }

