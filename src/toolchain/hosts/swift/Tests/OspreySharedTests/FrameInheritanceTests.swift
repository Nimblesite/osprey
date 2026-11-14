import AppKit
import CoreText
import SwiftUI
import XCTest

@testable import OspreyShared

// What a frame's subtree inherits ([UI-COMPONENTS-FRAMED]): authored typography through plain
// and nested frames, alignment, scroll bounds, dialog focus — and the modal focus boundaries
// that keep a sheet's editors the only ones on the keyboard.

final class FrameInheritanceTests: RenderingTestCase {
    override func setUpWithError() throws {
        try Repo.registerFonts(["STIXTwoText.ttf", "STIXTwoText-Italic.ttf", "STIXTwoMath-Regular.ttf", "Geist.ttf"])
    }

    /// A plain frame must preserve every painted pixel of the italic f.
    @MainActor
    func testPlainFramesPreserveItalicGlyphOverhang() throws {
        let resolver = Fixtures.resolver([("one", "#000000")])
        let text = Fixtures.text("Proof", "callout", "one")
        let italic = Inherited().merging(["font": .string("serif"), "textSize": .number(54), "italic": .bool(true), "hug": .bool(true), "tracking": .number(-0.6)])
        let reference = NodeView(node: text, drawing: Fixtures.drawing()).environment(\.ospreyInherited, italic)
        let framed = NodeView(node: Fixtures.framed(["font": .string("serif"), "textSize": .number(54), "italic": .bool(true), "hug": .bool(true), "tracking": .number(-0.6)], text),
                              drawing: Fixtures.drawing())
        let plain = try darkPixels(framed.environment(\.ospreyTheme, resolver))
        XCTAssertEqual(plain, try darkPixels(reference.environment(\.ospreyTheme, resolver)),
                       "A plain frame must preserve every painted pixel of the italic f")
        let nested = NodeView(node: Fixtures.framed(["radius": .number(5), "hug": .bool(true)],
                                                    Fixtures.framed(["font": .string("serif"), "textSize": .number(54), "italic": .bool(true), "hug": .bool(true), "tracking": .number(-0.6)], text)),
                              drawing: Fixtures.drawing()).environment(\.ospreyTheme, resolver)
        XCTAssertEqual(try darkPixels(nested), plain, "Rounded backgrounds must not clip foreground ink or keyboard outlines")
    }

    @MainActor
    private func darkPixels<Content: View>(_ content: Content) throws -> Int {
        let view = content.fixedSize().padding(30).frame(width: 300, height: 150, alignment: .topLeading)
            .environment(\.colorScheme, .light).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 300, height: 150)
        defer { window.close() }
        return TestResources.pixels(try TestResources.raster(host)) {
            $0.redComponent < 0.5 && $0.greenComponent < 0.5 && $0.blueComponent < 0.5
        }
    }

    /// Keyboard focus paints a two-point accent outline three points outside the button, on the
    /// authored accent — and an unfocused button draws none.
    @MainActor
    func testKeyboardFocusPaintsTheAuthoredOutlineOutsideTheButton() throws {
        let resolver = Fixtures.resolver([("accent", "#FF0000")])
        for focused in [false, true] {
            let face = Button("Close") { Label("Close", systemImage: "xmark") }
                .buttonStyle(Surface(role: "plain", quiet: true, round: true, height: 32, padding: 0, theme: resolver, dark: false, focused: focused))
                .frame(width: 32, height: 32).padding(30).frame(width: 100, height: 100, alignment: .topLeading)
                .environment(\.colorScheme, .light).background(Color.white)
            let (host, window) = TestResources.hosted(face, width: 100, height: 100)
            let ring = TestResources.pixels(try TestResources.raster(host)) { $0.redComponent > 0.9 && $0.greenComponent < 0.2 }
            window.close()
            if focused { XCTAssertGreaterThan(ring, 20, "Keyboard focus needs a two-point accent outline with three-point offset") }
            else { XCTAssertLessThan(ring, 5, "an unfocused button draws no focus ring") }
        }
    }

    /// A panel casts one outer shadow, not shadows from its children onto its own surface.
    @MainActor
    func testFloatingPanelShadowDoesNotShadeItsOwnContents() throws {
        let inner = Fixtures.framed(["width": .number(40), "height": .number(40), "fill": "one"], Fixtures.text(""))
        let node = Fixtures.framed(["width": .number(100), "height": .number(100), "top": .number(30), "left": .number(30),
                                    "fill": "surface", "shadow": .string("floating")], inner)
        let view = NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#000000"), ("surface", "#FFFFFF")]))
            .padding(30).frame(width: 160, height: 160).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 160, height: 160)
        defer { window.close() }
        let inside = try TestResources.pixel(try TestResources.raster(host), 55, 80)
        XCTAssertEqual(inside.redComponent, 1, accuracy: 0.005, "A panel casts one outer shadow, not shadows from its children onto its own surface")
    }

    /// A scrollable frame with no authored height is bounded by its pane while the short proof
    /// inside it keeps its natural height, unclipped and uncollapsed.
    @MainActor
    func testViewportScrollBoundsKeepShortContentAtItsNaturalHeight() throws {
        let child = Fixtures.framed(["height": .number(200)], Fixtures.text("Short proof"))
        let node = Fixtures.framed(["scrollY": .bool(true)], child)
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 600)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first)
        XCTAssertEqual(scroll.contentView.bounds.height, 600, "the scroll bound is the pane it was given")
        XCTAssertEqual(try XCTUnwrap(scroll.documentView).bounds.height, 200, "a short proof must retain its natural height")
    }

    /// Authored vertical overflow keeps the end of the content reachable.
    @MainActor
    func testAuthoredVerticalOverflowKeepsTheEndReachable() throws {
        let child = Fixtures.framed(["height": .number(500)], Fixtures.text("Last proof detail", "caption", "one"))
        let node = Fixtures.framed(["height": .number(200), "scrollY": .bool(true)], child)
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 200)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first)
        let document = try XCTUnwrap(scroll.documentView)
        XCTAssertEqual(scroll.contentView.bounds.height, 200)
        XCTAssertGreaterThanOrEqual(document.bounds.height, 500)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: document.bounds.maxY))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertGreaterThan(scroll.contentView.bounds.minY, 0)
        XCTAssertGreaterThanOrEqual(scroll.contentView.bounds.maxY, document.bounds.maxY)
    }

    /// Unequal-width lines share a center when their parent frame requests center alignment.
    @MainActor
    func testColumnChildrenShareTheirAuthoredCenterAlignment() throws {
        let node = Fixtures.framed(["cross": "center", "hug": .bool(true), "textSize": .number(20)],
                                   .object(["kind": .string("stack"), "axis": .string("vertical"),
                                            "children": .array([Fixtures.text("I", "caption", "one"), Fixtures.text("MMMMMMMM", "caption", "two")])]))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000"), ("two", "#0000FF")]))
            .frame(width: 200, height: 100, alignment: .topLeading).background(Color.white), width: 200, height: 100)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        XCTAssertEqual(try inkCenter(bitmap, red: true), try inkCenter(bitmap, red: false), accuracy: 2,
                       "Unequal-width lines must share a center when their parent frame requests center alignment")
    }

    private func inkCenter(_ bitmap: NSBitmapImageRep, red: Bool) throws -> CGFloat {
        let xs = (0..<bitmap.pixelsWide).filter { x in (0..<bitmap.pixelsHigh).contains { y in
            guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
            return red ? colour.redComponent > 0.5 && colour.blueComponent < 0.3 : colour.blueComponent > 0.5 && colour.redComponent < 0.3
        } }
        return CGFloat(try XCTUnwrap(xs.first) + XCTUnwrap(xs.last)) / 2
    }

    /// A newly opened dialog receives Escape and dispatches its own dismissal exactly once.
    @MainActor
    func testNativeDialogReceivesEscapeWhenItOpens() throws {
        var events: [Int] = []
        let body = Tree.object(["kind": .string("button"), "label": .string("Close preview"), "symbol": .string("dismiss"), "role": .string("plain"), "enabled": .bool(true), "event": .number(2)])
        let node = Tree.object(["kind": .string("dialog"), "title": .string("Preview"), "body": body, "event": .number(1)])
        let (_, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing("dialog") { ordinal, _ in events.append(ordinal) }),
                                              width: 600, height: 400, keyboard: true)
        defer { window.close() }
        let escape = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 1,
            windowNumber: window.windowNumber, context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
        NSApp.sendEvent(escape)
        TestResources.settle(0.1)
        XCTAssertEqual(events, [1], "A newly opened dialog must receive Escape and dispatch its own dismissal exactly once")
    }

    /// A background editor's focus epoch must not steal focus from a keyboard-contained dialog.
    @MainActor
    func testDialogFocusSurvivesBackgroundEditorEpochAdvance() throws {
        let supplied = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: .titled, backing: .buffered, defer: false)
        let (host, window) = TestResources.hosted(ModalFocusFixture(focusEpoch: 0), width: 600, height: 400, keyboard: true, suppliedWindow: supplied)
        defer { window.close() }
        let background = try XCTUnwrap(editor(host, named: "background"))
        let modal = try XCTUnwrap(editor(host, named: "modal"))
        XCTAssertTrue(window.isKeyWindow)
        XCTAssertTrue(window.makeFirstResponder(modal))
        XCTAssertTrue(window.firstResponder === modal, "The test must begin with focus inside the real dialog")
        host.rootView = ModalFocusFixture(focusEpoch: 1)
        TestResources.settle(0.1)
        XCTAssertFalse(window.firstResponder === background, "A background editor epoch must not steal focus from a keyboard-contained dialog")
        try assertModalEditorEpochAdvances(host, window, modal)
        try assertDismissalRestoresFocusAndReleasesGuard(host, window, background)
    }

    @MainActor
    private func assertModalEditorEpochAdvances(_ host: NSHostingView<ModalFocusFixture>, _ window: NSWindow, _ modal: NSTextView) throws {
        XCTAssertTrue(window.makeFirstResponder(nil))
        let focusWindow = try XCTUnwrap(window as? FocusTestWindow)
        focusWindow.keyState = false
        host.rootView = ModalFocusFixture(focusEpoch: 1, modalEpoch: 1)
        host.layoutSubtreeIfNeeded()
        focusWindow.keyState = true
        TestResources.settle(0.1)
        XCTAssertTrue(window.firstResponder === modal, "An authored epoch must still focus an editor inside the active dialog; actual: \(String(describing: window.firstResponder))")
        update(host, ModalFocusFixture(focusEpoch: 1, modalEpoch: 1))
        XCTAssertTrue(window.firstResponder === modal, "A later render without a new epoch must preserve the dialog editor's focus")
    }

    @MainActor
    private func assertDismissalRestoresFocusAndReleasesGuard(_ host: NSHostingView<ModalFocusFixture>, _ window: NSWindow, _ background: NSTextView) throws {
        update(host, ModalFocusFixture(focusEpoch: 1, modalEpoch: 1, showsModal: false))
        XCTAssertTrue(window.makeFirstResponder(background))
        update(host, ModalFocusFixture(background: "ordinary", focusEpoch: 1, modalEpoch: 1))
        let reopened = try editor(host, named: "modal")
        XCTAssertTrue(window.makeFirstResponder(reopened))
        update(host, ModalFocusFixture(background: "ordinary", focusEpoch: 1, modalEpoch: 1, showsModal: false))
        XCTAssertTrue(window.firstResponder === background, "Removing a dialog must restore its previous responder")
        XCTAssertTrue(window.makeFirstResponder(nil))
        update(host, ModalFocusFixture(background: "ordinary", focusEpoch: 2, modalEpoch: 1, showsModal: false))
        XCTAssertTrue(window.firstResponder === background, "Dismissal must release the background editor's focus guard")
    }

    @MainActor
    private func editor(_ host: NSView, named value: String) throws -> NSTextView {
        try XCTUnwrap(allEditors(host).first { $0.string == value })
    }

    /// Every editor under a host — in its window and in any sheet that window shows.
    @MainActor
    private func allEditors(_ host: NSView) -> [NSTextView] {
        var editors = TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }
        host.window?.sheets.forEach { sheet in
            if let content = sheet.contentView { editors += TestResources.scrollViews(content).compactMap { $0.documentView as? NSTextView } }
        }
        return editors
    }

    @MainActor
    private func update(_ host: NSHostingView<ModalFocusFixture>, _ fixture: ModalFocusFixture) {
        host.rootView = fixture
        TestResources.settle(0.1)
    }

    /// A child boundary's dismissal retains its open parent's background-focus guard.
    @MainActor
    func testNestedModalBoundaryDetachPreservesParentGuard() {
        let baseline = NSTextView(), nested = NSTextView(), childEditor = NSTextView()
        let window = TestResources.focusWindow([baseline, nested, childEditor])
        defer { window.close() }
        let parent = ModalFocusBoundary(), child = ModalFocusBoundary()
        XCTAssertTrue(window.makeFirstResponder(baseline))
        parent.attach(to: window)
        XCTAssertTrue(window.makeFirstResponder(nested))
        child.attach(to: window)
        XCTAssertTrue(window.makeFirstResponder(childEditor))
        child.detach()
        XCTAssertFalse(ModalFocusBoundary.permits(nil, in: window), "A child dismissal must retain its open parent's background-focus guard")
        XCTAssertTrue(ModalFocusBoundary.permits(parent, in: window))
        XCTAssertTrue(window.firstResponder === nested, "A child dismissal returns to its parent control")
        parent.detach()
        XCTAssertTrue(ModalFocusBoundary.permits(nil, in: window))
        XCTAssertTrue(window.firstResponder === baseline, "Outermost removal restores the original responder")
    }


    /// Restoration must target the original control, not its mutable shared field editor.
    @MainActor
    func testModalRestoresTheOriginalTextFieldAfterFieldEditorReuse() throws {
        let original = NSTextField(frame: NSRect(x: 0, y: 0, width: 100, height: 24))
        let other = NSTextField(frame: NSRect(x: 0, y: 40, width: 100, height: 24))
        let window = TestResources.focusWindow([original, other])
        defer { window.close() }
        XCTAssertTrue(window.makeFirstResponder(original))
        let shared = try XCTUnwrap(original.currentEditor())
        let boundary = ModalFocusBoundary()
        boundary.attach(to: window)
        XCTAssertTrue(window.makeFirstResponder(other))
        XCTAssertTrue(other.currentEditor() === shared, "AppKit reuses one field editor across controls")
        XCTAssertNil(original.currentEditor())
        boundary.detach()
        XCTAssertTrue(original.currentEditor() === shared, "Restoration must target the original control, not its mutable shared editor")
        XCTAssertNil(other.currentEditor())
    }

    /// Moving a boundary must not leave a stale guard in its previous window.
    @MainActor
    func testMovingModalBoundaryClearsItsPreviousWindowGuard() {
        let first = TestResources.focusWindow(), second = TestResources.focusWindow()
        defer { first.close(); second.close() }
        let boundary = ModalFocusBoundary()
        boundary.attach(to: first)
        boundary.attach(to: second)
        XCTAssertTrue(ModalFocusBoundary.permits(nil, in: first), "Moving a boundary must not leave a stale guard in its previous window")
        XCTAssertFalse(ModalFocusBoundary.permits(nil, in: second))
        XCTAssertTrue(ModalFocusBoundary.permits(boundary, in: second))
        boundary.detach()
        XCTAssertTrue(ModalFocusBoundary.permits(nil, in: second))
    }

    /// Compact mathematical tools retain their authored symbol size through plain frames: a
    /// nested zero does not reset it, and the authored size differs from the unauthored default.
    @MainActor
    func testNestedPlainFrameRetainsAuthoredSymbolDimensions() throws {
        let icon = Tree.object(["kind": .string("icon"), "symbol": .string("minus")])
        let nested = iconWidth(icon, Inherited().merging(["symbolSize": .number(13)]).merging(["symbolSize": .number(0)]))
        let direct = iconWidth(icon, Inherited().merging(["symbolSize": .number(13)]))
        let unauthored = iconWidth(icon, Inherited())
        XCTAssertEqual(nested, direct, accuracy: 0.5, "Compact mathematical tools retain their authored symbol size through plain frames")
        XCTAssertNotEqual(nested, unauthored, accuracy: 0.5, "the authored size must actually author the glyph")
    }

    @MainActor
    private func iconWidth(_ icon: Tree, _ inherited: Inherited) -> CGFloat {
        var observed = CGSize.zero
        let host = NSHostingView(rootView: NodeView(node: icon, drawing: Fixtures.drawing())
            .environment(\.ospreyInherited, inherited)
            .onGeometryChange(for: CGSize.self, of: \.size) { observed = $0 })
        host.layoutSubtreeIfNeeded()
        TestResources.settle()
        return observed.width
    }

    /// An ink-only child frame must preserve its parent's quiet, trailing-symbol control styling.
    @MainActor
    func testNestedPlainFrameRetainsAuthoredControlPresentation() throws {
        var observed: [Int] = []
        let parent: [String: Tree] = ["quietControls": .bool(true), "trailingSymbols": .bool(true), "controlHeight": .number(38)]
        let nested: [String: Tree] = ["quietControls": .bool(false), "trailingSymbols": .bool(false), "controlHeight": .number(0)]
        let view = ControlProbe { observed = $0 }
            .environment(\.ospreyInherited, Inherited().merging(parent).merging(nested))
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        TestResources.settle()
        XCTAssertEqual(observed, [1, 1, 38], "An ink-only child frame must preserve its parent’s quiet, trailing-symbol control styling")
    }
    // Extra: a nested frame that authors its own symbol size overrides its parent's.
    @MainActor
    func testNestedPlainFrameRetainsAuthoredSymbolDimensions_extraANestedFrameReauthorsItsSymbolSize() throws {
        let icon = Tree.object(["kind": .string("icon"), "symbol": .string("minus")])
        let inherited = iconWidth(icon, Inherited().merging(["symbolSize": .number(13)]).merging(["symbolSize": .number(28)]))
        let direct = iconWidth(icon, Inherited().merging(["symbolSize": .number(28)]))
        XCTAssertEqual(inherited, direct, accuracy: 0.5, "the nearest frame that names a size authors it")
        XCTAssertGreaterThan(inherited, iconWidth(icon, Inherited().merging(["symbolSize": .number(13)])))
    }

    // Extra: a nested frame that authors its own control height overrides its parent's, while
    // presentation the child does not name keeps flowing from the parent.
    @MainActor
    func testNestedPlainFrameRetainsAuthoredControlPresentation_extraANestedFrameReauthorsItsControlHeight() throws {
        var observed: [Int] = []
        let parent: [String: Tree] = ["quietControls": .bool(true), "controlHeight": .number(38)]
        let nested: [String: Tree] = ["controlHeight": .number(20)]
        let host = NSHostingView(rootView: ControlProbe { observed = $0 }
            .environment(\.ospreyInherited, Inherited().merging(parent).merging(nested)))
        host.layoutSubtreeIfNeeded()
        TestResources.settle()
        XCTAssertEqual(observed, [1, 0, 20], "quiet flows down; the named height re-authors")
    }

    // Extra: bold upright text keeps its glyph through a rounded nested frame too.
    @MainActor
    func testAuthoredVerticalOverflowKeepsTheEndReachable_extraBoldUprightTextSurvivesRoundedFrames() throws {
        let resolver = Fixtures.resolver([("one", "#000000")])
        let words = Fixtures.text("Theorem", "heading", "one")
        let bold: [String: Tree] = ["font": .string("serif"), "textSize": .number(30)]
        let reference = NodeView(node: words, drawing: Fixtures.drawing()).environment(\.ospreyInherited, Inherited().merging(bold))
        let nested = NodeView(node: Fixtures.framed(["radius": .number(9)], Fixtures.framed(bold, words)), drawing: Fixtures.drawing())
        XCTAssertEqual(try darkPixels(nested.environment(\.ospreyTheme, resolver)),
                       try darkPixels(reference.environment(\.ospreyTheme, resolver)),
                       "rounded nesting must not clip upright headings")
    }

    // Extra: an authored italic flows down to the words inside, and a nested frame that does
    // not re-author the family keeps it.
    @MainActor
    func testPlainFramesPreserveItalicGlyphOverhang_extraAuthoredItalicFlowsDownThroughFrames() throws {
        let resolver = Fixtures.resolver([("one", "#000000")])
        let words = Fixtures.text("Slope", "body", "one")
        let serif = ["font": .string("serif"), "textSize": .number(24), "italic": .bool(true)] as [String: Tree]
        let reference = NodeView(node: words, drawing: Fixtures.drawing()).environment(\.ospreyInherited, Inherited().merging(serif))
        let nested = NodeView(node: Fixtures.framed(["hug": .bool(true)], Fixtures.framed(serif, words)), drawing: Fixtures.drawing())
        XCTAssertEqual(try darkPixels(nested.environment(\.ospreyTheme, resolver)),
                       try darkPixels(reference.environment(\.ospreyTheme, resolver)),
                       "the italic a frame authors must reach its words through nested frames")
    }

    // Extra: the focus ring sits three to five points outside the content — nothing further out.
    @MainActor
    func testKeyboardFocusPaintsTheAuthoredOutlineOutsideTheButton_extraTheFocusRingOffsetIsAuthored() throws {
        let resolver = Fixtures.resolver([("accent", "#FF0000")])
        let face = Button("Close") { Label("Close", systemImage: "xmark") }
            .buttonStyle(Surface(role: "plain", quiet: true, round: true, height: 32, padding: 0, theme: resolver, dark: false, focused: true))
            .frame(width: 40, height: 32).padding(40).frame(width: 120, height: 112, alignment: .topLeading)
            .environment(\.colorScheme, .light).background(Color.white)
        let (host, window) = TestResources.hosted(face, width: 120, height: 112)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        var xs: [Int] = []
        for x in 0..<120 where (0..<112).contains(where: { y in
            guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
            return colour.redComponent > 0.7 && colour.greenComponent < 0.3
        }) { xs.append(x) }
        XCTAssertFalse(xs.isEmpty, "the ring must draw")
        XCTAssertLessThan(xs.min() ?? 99, 45, "the ring sits outside the button's ink")
        XCTAssertGreaterThan(xs.max() ?? 0, 75, "the ring wraps the button's ink")
    }

    // Extra: the panel's surface keeps its authored colour at every corner inside it.
    @MainActor
    func testFloatingPanelShadowDoesNotShadeItsOwnContents_extraAPanelsSurfaceKeepsItsColourAtEveryCorner() throws {
        let inner = Fixtures.framed(["width": .number(20), "height": .number(20), "fill": "one"], Fixtures.text(""))
        let node = Fixtures.framed(["width": .number(100), "height": .number(100), "top": .number(30), "left": .number(30),
                                    "fill": "surface", "shadow": .string("floating")], inner)
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#000000"), ("surface", "#FFFFFF")]))
            .padding(30).frame(width: 160, height: 160).background(Color.white), width: 160, height: 160)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        for point in [(80, 45), (145, 45), (80, 115), (145, 115)] {
            XCTAssertGreaterThan(try TestResources.pixel(bitmap, point.0, point.1).redComponent, 0.95, "the surface keeps its fill at \(point)")
        }
    }

    // Extra: a scroll frame without an authored height keeps the pane even for tall content.
    @MainActor
    func testViewportScrollBoundsKeepShortContentAtItsNaturalHeight_extraAScrollFrameWithoutHeightKeepsItsPaneForTallContent() throws {
        let child = Fixtures.framed(["height": .number(500)], Fixtures.text("Tall proof"))
        let node = Fixtures.framed(["scrollY": .bool(true)], child)
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 300)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first)
        XCTAssertEqual(scroll.contentView.bounds.height, 300, "the pane bounds the scroll")
        XCTAssertEqual(try XCTUnwrap(scroll.documentView).bounds.height, 500, "the content keeps its authored height")
    }

    // Extra: an authored cross of end places both lines at the right edge.
    @MainActor
    func testColumnChildrenShareTheirAuthoredCenterAlignment_extraAnAuthoredEndAlignmentPlacesLinesRight() throws {
        let node = Fixtures.framed(["cross": "end", "hug": .bool(true), "textSize": .number(36)],
                                   .object(["kind": .string("stack"), "axis": .string("vertical"),
                                            "children": .array([Fixtures.text("H", "caption", "one"), Fixtures.text("MMM", "caption", "two")])]))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000"), ("two", "#0000FF")]))
            .frame(width: 200, height: 100, alignment: .topLeading).background(Color.white), width: 200, height: 100)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        let reds = try inkBounds(bitmap, red: true), blues = try inkBounds(bitmap, red: false)
        XCTAssertEqual(CGFloat(reds.max), CGFloat(blues.max), accuracy: 2, "both lines end together at the right")
        XCTAssertGreaterThan(blues.max - blues.min, reds.max - reds.min, "the long line really is longer")
    }

    @MainActor
    private func inkBounds(_ bitmap: NSBitmapImageRep, red: Bool) throws -> (min: Int, max: Int) {
        let xs = (0..<bitmap.pixelsWide).filter { x in (0..<bitmap.pixelsHigh).contains { y in
            guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
            return red ? colour.redComponent > 0.5 && colour.blueComponent < 0.3 : colour.blueComponent > 0.5 && colour.redComponent < 0.3
        } }
        XCTAssertFalse(xs.isEmpty, "the line must draw"); return (xs.min() ?? 0, xs.max() ?? 0)
    }


    // Extra: once the dialog is gone, a background editor's focus request is honoured again.
    @MainActor
    func testDialogFocusSurvivesBackgroundEditorEpochAdvance_extraDismissalReleasesTheBackgroundKeyboard() throws {
        let supplied = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: .titled, backing: .buffered, defer: false)
        let (host, window) = TestResources.hosted(ModalFocusFixture(focusEpoch: 0), width: 600, height: 400, keyboard: true, suppliedWindow: supplied)
        defer { window.close() }
        let background = try XCTUnwrap(editor(host, named: "background"))
        XCTAssertNotNil(try editor(host, named: "modal"))
        update(host, ModalFocusFixture(focusEpoch: 1, showsModal: false))
        update(host, ModalFocusFixture(focusEpoch: 2, showsModal: false))
        XCTAssertTrue(window.firstResponder === background, "with no dialog, the background editor's epoch focuses it again")
    }



}

/// A button that takes (or does not take) the keyboard, as the focus-ring probe needs.
private struct FocusedButtonProbe: View {
    let node: Tree
    let focused: Bool
    @FocusState private var takes: Bool

    var body: some View {
        ButtonView(node: node) {}.focused($takes).onAppear { takes = focused }
    }
}

/// Reads the control presentation a frame's subtree inherited.
private struct ControlProbe: View {
    let observed: ([Int]) -> Void
    @Environment(\.ospreyInherited) private var inherited

    var body: some View {
        Color.clear.frame(width: 1, height: 1).onAppear {
            observed([inherited.quiet ? 1 : 0, inherited.trailingSymbols ? 1 : 0, Int(inherited.controlHeight)])
        }
    }
}

/// A background editor, a dialog and its editor, for the modal focus tests.
private struct ModalFocusFixture: View {
    var background = "background"
    var focusEpoch = 0
    var modalEpoch = 0
    var showsModal = true

    private func editor(_ value: String, epoch: Int) -> Tree {
        .object(["kind": .string("editor"), "label": .string("Modal"), "value": .string(value), "inline": .bool(false),
                 "cursor": .number(Double(value.utf8.count)), "focusEpoch": .number(Double(epoch)), "marks": .array([]), "event": .number(0), "placeEvent": .number(1)])
    }

    var body: some View {
        ZStack {
            CodeText(value: background, cursor: background.utf8.count, focusEpoch: focusEpoch, underlines: [], textSize: 20, inline: false,
                     edit: { _ in }, place: { _ in })
            if showsModal {
                NodeView(node: .object(["kind": .string("dialog"), "title": .string("Clear board"), "event": .number(3),
                                        "body": editor("modal", epoch: modalEpoch)]), drawing: Fixtures.drawing("modal-focus"))
                    .id("modal")
            }
        }
    }
}

