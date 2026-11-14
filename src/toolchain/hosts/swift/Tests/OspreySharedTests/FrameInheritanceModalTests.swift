import AppKit
import SwiftUI
import XCTest

@testable import OspreyShared

// The modal-focus boundary's own rules ([UI-COMPONENTS-EDITOR]): boundaries stack per window
// and unwind in order, an empty baseline restores nothing, two windows guard independently,
// and a dismissed dialog's Escape handler leaves with it.

final class FrameInheritanceModalTests: RenderingTestCase {
    // Extra: three boundaries stack and unwind in order.
    @MainActor
    func testNestedModalBoundaryDetachPreservesParentGuard_extraThreeBoundariesUnwindInOrder() {
        let first = NSTextView(), second = NSTextView(), third = NSTextView()
        let window = TestResources.focusWindow([first, second, third])
        defer { window.close() }
        let boundaries = (0..<3).map { _ in ModalFocusBoundary() }
        for (boundary, view) in zip(boundaries, [first, second, third]) {
            boundary.attach(to: window)
            XCTAssertTrue(window.makeFirstResponder(view), "the level must take focus")
        }
        boundaries[2].detach()
        XCTAssertTrue(window.firstResponder === second, "the third dismissal returns to the second")
        boundaries[1].detach()
        XCTAssertTrue(window.firstResponder === first, "the second dismissal returns to the first")
        XCTAssertFalse(ModalFocusBoundary.permits(nil, in: window), "the outermost boundary still guards")
        boundaries[0].detach()
        XCTAssertTrue(ModalFocusBoundary.permits(nil, in: window))
        XCTAssertFalse(window.firstResponder === second, "unwinding releases every level's responder")
        XCTAssertFalse(window.firstResponder === third)
    }

    // Extra: attaching a boundary when nothing held focus restores no responder, harmlessly.
    @MainActor
    func testModalRestoresTheOriginalTextFieldAfterFieldEditorReuse_extraABoundaryWithNoResponderRestoresNothing() {
        let window = TestResources.focusWindow()
        defer { window.close() }
        let boundary = ModalFocusBoundary()
        boundary.attach(to: window)
        boundary.detach()
        XCTAssertTrue(ModalFocusBoundary.permits(nil, in: window))
        XCTAssertNil(window.firstResponder as? NSView)
    }

    // Extra: two boundaries in two windows guard independently.
    @MainActor
    func testMovingModalBoundaryClearsItsPreviousWindowGuard_extraTwoWindowsGuardIndependently() {
        let first = TestResources.focusWindow(), second = TestResources.focusWindow()
        defer { first.close(); second.close() }
        let one = ModalFocusBoundary(), two = ModalFocusBoundary()
        one.attach(to: first)
        two.attach(to: second)
        XCTAssertFalse(ModalFocusBoundary.permits(nil, in: first))
        XCTAssertFalse(ModalFocusBoundary.permits(nil, in: second))
        XCTAssertTrue(ModalFocusBoundary.permits(one, in: first))
        XCTAssertFalse(ModalFocusBoundary.permits(one, in: second), "a boundary guards only its own window")
        one.detach()
        XCTAssertTrue(ModalFocusBoundary.permits(nil, in: first))
        XCTAssertFalse(ModalFocusBoundary.permits(nil, in: second))
    }

    // Extra: once the dialog is gone, a later Escape belongs to no dialog — the monitor left
    // with it, and the dismissal count stays at exactly one.
    @MainActor
    func testNativeDialogReceivesEscapeWhenItOpens_extraEscapeAfterDismissalDeliversNothingMore() throws {
        var events: [Int] = []
        let body = Tree.object(["kind": .string("button"), "label": .string("Close preview"), "symbol": .string("dismiss"), "role": .string("plain"), "enabled": .bool(true), "event": .number(2)])
        let node = Tree.object(["kind": .string("dialog"), "title": .string("Preview"), "body": body, "event": .number(1)])
        let (host, window) = TestResources.hosted(DialogHost(node: node, shows: true) { events.append($0) }, width: 600, height: 400)
        defer { window.close() }
        let escape = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 1, windowNumber: window.windowNumber,
                                      context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53)!
        NSApp.sendEvent(escape)
        TestResources.settle(0.1)
        XCTAssertEqual(events, [1])
        host.rootView = DialogHost(node: node, shows: false) { events.append($0) }
        TestResources.settle(0.2)
        NSApp.sendEvent(escape)
        TestResources.settle(0.1)
        XCTAssertEqual(events, [1], "a dismissed dialog's Escape handler is gone with it")
    }
}

/// A dialog its test can show and hide, delivering its dismissal events.
private struct DialogHost: View {
    let node: Tree
    let shows: Bool
    let delivered: (Int) -> Void

    var body: some View {
        if shows {
            NodeView(node: node, drawing: Fixtures.drawing("dialog") { ordinal, _ in delivered(ordinal) })
        } else {
            Text("No dialog")
        }
    }
}
