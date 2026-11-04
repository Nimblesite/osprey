import AppKit
import CoreText
import SwiftUI
import XCTest
@testable import OspreyNative

final class FrameInheritanceTests: XCTestCase {
    @MainActor
    func testPlainFramesPreserveItalicGlyphOverhang() throws {
        let font = TestResources.assets.appendingPathComponent("fonts/STIXTwoText-Italic.ttf")
        XCTAssertTrue(CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil))
        defer { CTFontManagerUnregisterFontsForURL(font as CFURL, .process, nil) }
        let style = try TestResources.frame(["font": "serif", "textSize": 54, "italic": true, "hug": true, "tracking": -0.6])
        let text = Node.text("Proof", .callout, .primary)
        let drawing = Drawing(region: "italic", revision: 1, send: { _, _ in })
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#000000", dark: "#000000")], spacing: [], feedback: 0, settle: 0)
        let reference = NodeView(node: text, drawing: drawing).environment(\.ospreyTypography, FrameTypography().applying(style))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme))
        let framed = NodeView(node: .framed(style, text), drawing: drawing).environment(\.ospreyTheme, ThemeResolver(theme: theme))
        XCTAssertEqual(try darkPixels(framed), try darkPixels(reference), "A plain frame must preserve every painted pixel of the italic f")
        let rounded = try TestResources.frame(["radius": 5, "hug": true])
        let nested = NodeView(node: .framed(rounded, .framed(style, text)), drawing: drawing).environment(\.ospreyTheme, ThemeResolver(theme: theme))
        XCTAssertEqual(try darkPixels(nested), try darkPixels(reference), "Rounded backgrounds must not clip foreground ink or keyboard outlines")
    }

    @MainActor
    private func darkPixels<Content: View>(_ content: Content) throws -> Int {
        let view = content.fixedSize().padding(30).frame(width: 300, height: 150, alignment: .topLeading)
            .environment(\.colorScheme, .light).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 300, height: 150)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        return (0..<bitmap.pixelsWide).reduce(0) { total, x in total + (0..<bitmap.pixelsHigh).filter { y in
            guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
            return color.redComponent < 0.5 && color.greenComponent < 0.5 && color.blueComponent < 0.5
        }.count }
    }

    @MainActor
    func testKeyboardFocusPaintsTheAuthoredOutlineOutsideTheButton() throws {
        let style = try TestResources.frame(["controlHeight": 32, "iconOnly": true, "quietControls": true, "symbolSize": 13])
        let theme = Theme(tokens: [ThemeToken(name: "accent", light: "#FF0000", dark: "#FF0000")], spacing: [], feedback: 0, settle: 0)
        let view = FocusedButtonProbe().modifier(FrameControls(style: style, padding: nil))
            .frame(width: 32, height: 32).padding(30).frame(width: 100, height: 100, alignment: .topLeading)
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).environment(\.colorScheme, .light).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 100, height: 100, keyboard: true)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        let ring = try XCTUnwrap(bitmap.colorAt(x: 25, y: 46)?.usingColorSpace(.sRGB))
        XCTAssertGreaterThan(ring.redComponent, 0.9)
        XCTAssertLessThan(ring.greenComponent, 0.2, "Keyboard focus needs a two-point accent outline with three-point offset")
    }

    @MainActor
    func testFloatingPanelShadowDoesNotShadeItsOwnContents() throws {
        let outer = try TestResources.frame(["width": 100, "height": 100, "top": 30, "left": 30, "fill": "surface", "shadow": "floating"])
        let inner = try TestResources.frame(["width": 40, "height": 40, "fill": "primary"])
        let node = Node.framed(outer, .framed(inner, .text("", .body, .primary)))
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#000000", dark: "#000000"), ThemeToken(name: "surface", light: "#FFFFFF", dark: "#FFFFFF")], spacing: [], feedback: 0, settle: 0)
        let view = NodeView(node: node, drawing: Drawing(region: "shadow", revision: 1, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).padding(30).frame(width: 160, height: 160).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 160, height: 160)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        let inside = try XCTUnwrap(bitmap.colorAt(x: 55, y: 80)?.usingColorSpace(.sRGB))
        XCTAssertEqual(inside.redComponent, 1, accuracy: 0.005, "A panel casts one outer shadow, not shadows from its children onto its own surface")
    }

    @MainActor
    func testViewportScrollBoundsKeepShortContentAtItsNaturalHeight() throws {
        let style = try TestResources.frame(["height": 100, "viewportHeightInset": 200, "scrollY": true])
        let child = Node.framed(try TestResources.frame(["height": 200]), .text("Short proof", .body, .primary))
        var measured = CGFloat.zero
        let view = ViewportContent {
            NodeView(node: .framed(style, child), drawing: Drawing(region: "natural-scroll", revision: 1, send: { _, _ in }))
                .onGeometryChange(for: CGFloat.self, of: \.size.height) { measured = $0 }
        }
        let (_, window) = TestResources.hosted(view, width: 400, height: 600)
        defer { window.close() }
        XCTAssertEqual(measured, 200, "A viewport scroll bound is a maximum; a short proof must retain its natural height")
    }

    @MainActor
    func testAuthoredVerticalOverflowKeepsTheEndReachable() throws {
        let style = try TestResources.frame(["height": 200, "scrollY": true])
        let child = Node.framed(try TestResources.frame(["height": 500]), .text("Last proof detail", .caption, .primary))
        let view = NodeView(node: .framed(style, child), drawing: Drawing(region: "overflow", revision: 1, send: { _, _ in }))
        let (host, window) = TestResources.hosted(view, width: 400, height: 200)
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

    @MainActor
    func testColumnChildrenShareTheirAuthoredCenterAlignment() throws {
        let style = try TestResources.frame(["align": "center", "hug": true, "textSize": 20])
        let node = Node.framed(style, .stack(.vertical, [.text("I", .caption, .primary), .text("MMMMMMMM", .caption, .secondary)]))
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#FF0000", dark: "#FF0000"),
            ThemeToken(name: "secondary", light: "#0000FF", dark: "#0000FF")], spacing: [0], feedback: 0, settle: 0)
        let view = NodeView(node: node, drawing: Drawing(region: "alignment", revision: 1, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 200, height: 100, alignment: .topLeading).background(Color.white)
        let (host, window) = TestResources.hosted(view, width: 200, height: 100)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        XCTAssertEqual(try inkCenter(bitmap, red: true), try inkCenter(bitmap, red: false), accuracy: 2,
            "Unequal-width lines must share a center when their parent frame requests center alignment")
    }

    private func inkCenter(_ bitmap: NSBitmapImageRep, red: Bool) throws -> CGFloat {
        let xs = (0..<bitmap.pixelsWide).filter { x in (0..<bitmap.pixelsHigh).contains { y in
            guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
            return red ? color.redComponent > 0.8 && color.blueComponent < 0.2 : color.blueComponent > 0.8 && color.redComponent < 0.2
        } }
        return CGFloat(try XCTUnwrap(xs.first) + XCTUnwrap(xs.last)) / 2
    }

    @MainActor
    func testNativeDialogReceivesEscapeWhenItOpens() throws {
        var events: [Ordinal] = []
        let node = Node.dialog(title: "Preview", body: .button(label: "Close preview", symbol: "xmark", role: .plain, enabled: true, event: 2), event: 1)
        let view = NodeView(node: node, drawing: Drawing(region: "dialog", revision: 1, send: { ordinal, _ in events.append(ordinal) }))
        let (_, window) = TestResources.hosted(view, width: 600, height: 400, keyboard: true)
        defer { window.close() }
        let escape = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 1,
            windowNumber: window.windowNumber, context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
        window.sendEvent(escape)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        XCTAssertEqual(events, [1], "A newly opened dialog must receive Escape and dispatch its own dismissal exactly once")
    }

    @MainActor
    func testDialogFocusSurvivesBackgroundEditorEpochAdvance() throws {
        let supplied = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: .titled, backing: .buffered, defer: false)
        let (host, window) = TestResources.hosted(ModalFocusFixture(focusEpoch: 0), width: 600, height: 400, keyboard: true, suppliedWindow: supplied)
        defer { window.close() }
        let editors = TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }
        let background = try XCTUnwrap(editors.first { $0.string == "background" })
        let modal = try XCTUnwrap(editors.first { $0.string == "modal" })
        XCTAssertTrue(window.isKeyWindow)
        XCTAssertTrue(window.makeFirstResponder(modal))
        XCTAssertTrue(window.firstResponder === modal, "The test must begin with focus inside the real dialog")
        host.rootView = ModalFocusFixture(focusEpoch: 1)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
        XCTAssertTrue(window.firstResponder === modal, "A background editor epoch must not steal focus from a keyboard-contained dialog")
        XCTAssertFalse(window.firstResponder === background)
        try assertModalEditorEpochAdvances(host, window, modal)
        try assertIndependentWindowAllowsEditorFocus()
        try assertDismissalRestoresFocusAndReleasesGuard(host, window, background, modal)
    }

    @MainActor
    func testNestedModalBoundaryDetachPreservesParentGuard() {
        let baseline = NSTextView(), nested = NSTextView(), childEditor = NSTextView()
        let window = focusWindow([baseline, nested, childEditor])
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

    @MainActor
    private func focusWindow(_ views: [NSView] = []) -> NSWindow {
        let window = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let content = NSView()
        views.forEach(content.addSubview)
        window.contentView = content
        return window
    }

    @MainActor
    func testModalRestoresTheOriginalTextFieldAfterFieldEditorReuse() throws {
        let original = NSTextField(frame: NSRect(x: 0, y: 0, width: 100, height: 24))
        let other = NSTextField(frame: NSRect(x: 0, y: 40, width: 100, height: 24))
        let window = focusWindow([original, other])
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

    @MainActor
    func testMovingModalBoundaryClearsItsPreviousWindowGuard() {
        let first = focusWindow(), second = focusWindow()
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

    @MainActor
    private func assertModalEditorEpochAdvances(_ host: NSHostingView<ModalFocusFixture>, _ window: NSWindow, _ modal: NSTextView) throws {
        XCTAssertTrue(window.makeFirstResponder(nil))
        update(host, ModalFocusFixture(focusEpoch: 1, modalEpoch: 1))
        XCTAssertTrue(window.firstResponder === modal, "An authored epoch must still focus an editor inside the active dialog; actual: \(String(describing: window.firstResponder))")
        XCTAssertTrue(TestResources.scrollViews(host).contains { $0.documentView === modal }, "The focused modal editor must retain its native identity")
        update(host, ModalFocusFixture(focusEpoch: 1, modalEpoch: 1))
        XCTAssertTrue(window.firstResponder === modal, "A later render without a new epoch must preserve the dialog editor's focus")
    }

    @MainActor
    private func assertIndependentWindowAllowsEditorFocus() throws {
        let supplied = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: .titled, backing: .buffered, defer: false)
        let (host, window) = TestResources.hosted(ModalFocusFixture(showsModal: false), width: 600, height: 400, suppliedWindow: supplied)
        defer { window.close() }
        let editor = try editor(host, named: "background")
        update(host, ModalFocusFixture(focusEpoch: 1, showsModal: false))
        XCTAssertTrue(window.firstResponder === editor, "A dialog in another window must not inhibit this window's authored focus")
    }

    @MainActor
    private func assertDismissalRestoresFocusAndReleasesGuard(_ host: NSHostingView<ModalFocusFixture>, _ window: NSWindow, _ background: NSTextView, _ modal: NSTextView) throws {
        update(host, ModalFocusFixture(focusEpoch: 1, modalEpoch: 1, showsModal: false))
        XCTAssertFalse(window.firstResponder === modal)
        XCTAssertTrue(window.makeFirstResponder(nil))
        update(host, ModalFocusFixture(background: "ordinary", focusEpoch: 1, modalEpoch: 1, showsModal: false))
        XCTAssertFalse(window.firstResponder === background, "An ordinary background update must not request focus")
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
    private func editor(_ host: NSHostingView<ModalFocusFixture>, named value: String) throws -> NSTextView {
        try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first { $0.string == value })
    }

    @MainActor
    private func update(_ host: NSHostingView<ModalFocusFixture>, _ fixture: ModalFocusFixture) {
        host.rootView = fixture
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))
    }

    @MainActor
    func testNestedPlainFrameRetainsAuthoredSymbolDimensions() throws {
        let parent = try TestResources.frame(["symbolSize": 13])
        let nested = try TestResources.frame(["symbolSize": 0])
        var observed = CGSize.zero
        let view = NativeSymbol(name: "minus").onGeometryChange(for: CGSize.self, of: \.size) { observed = $0 }
            .modifier(FrameControls(style: nested, padding: nil)).modifier(FrameControls(style: parent, padding: nil))
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        XCTAssertEqual(observed.width, 13, "Compact mathematical tools retain their authored symbol size through plain frames")
        XCTAssertEqual(observed.height, 13)
    }

    @MainActor
    func testNestedPlainFrameRetainsAuthoredControlPresentation() throws {
        let parent = try TestResources.frame(["quietControls": true, "trailingSymbols": true, "controlHeight": 38])
        let nested = try TestResources.frame(["quietControls": false, "trailingSymbols": false, "controlHeight": 0])
        var observed: [Int] = []
        let view = ControlProbe { observed = $0 }.modifier(FrameControls(style: nested, padding: nil))
            .modifier(FrameControls(style: parent, padding: nil))
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        XCTAssertEqual(observed, [1, 1, 38], "An ink-only child frame must preserve its parent’s quiet, trailing-symbol control styling")
    }
}

private struct ControlProbe: View {
    let observed: ([Int]) -> Void
    @Environment(\.ospreyQuietControls) private var quiet
    @Environment(\.ospreyTrailingSymbols) private var trailing
    @Environment(\.ospreyControlHeight) private var height

    var body: some View {
        Color.clear.frame(width: 1, height: 1).onAppear { observed([quiet ? 1 : 0, trailing ? 1 : 0, height]) }
    }
}

private struct FocusedButtonProbe: View {
    @FocusState private var focused: Bool

    var body: some View {
        ButtonView(label: "Close", symbol: "xmark", role: .plain, enabled: true, tap: {})
            .focused($focused).onAppear { focused = true }
    }
}

private struct ModalFocusFixture: View {
    var background = "background"
    var focusEpoch = 0
    var modalEpoch = 0
    var showsModal = true

    var body: some View {
        ZStack {
            CodeText(value: background, cursor: background.utf8.count, focusEpoch: focusEpoch, underlines: [], textSize: 20, inline: false, ink: .black, edit: { _ in }, place: { _ in })
            if showsModal {
                DialogView(title: "Clear board", content: .editor(label: "Modal", value: "modal", inline: false, cursor: 5, focusEpoch: modalEpoch, marks: [], event: 1, placeEvent: 2),
                    drawing: Drawing(region: "modal-focus", revision: 1, send: { _, _ in }), dismiss: {})
            }
        }
    }
}
