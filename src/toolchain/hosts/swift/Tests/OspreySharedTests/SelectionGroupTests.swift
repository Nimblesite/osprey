import AppKit
import Foundation
import XCTest
@testable import OspreyShared

// A selection group's own rules, without an application: members order as the person
// reads them, a drag paints the span between its endpoints either way it runs, and the
// joined words carry one newline within a line and a blank line between blocks further
// apart ([UI-COMPONENTS-FRAMED]).
@MainActor
final class SelectionGroupTests: RenderingTestCase {
    private var window: NSWindow?

    override func setUp() {
        window = NSWindow(contentRect: NSRect(x: -2000, y: -2000, width: 400, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
    }

    override func tearDown() {
        window?.orderOut(nil)
        window = nil
    }

    private func member(_ words: String, at origin: NSPoint, in group: SelectionGroup) -> SelectableTextView {
        let view = SelectableTextView(attributed: NSAttributedString(string: words, attributes: [.font: NSFont.systemFont(ofSize: 13)]))
        view.setFrameSize(NSSize(width: 300, height: 20))
        view.setFrameOrigin(origin)
        window?.contentView?.addSubview(view)
        group.register(view)
        view.group = group
        return view
    }

    func testMembersOrderTopToBottomThenLeftToRight() {
        let group = SelectionGroup()
        _ = member("lower", at: NSPoint(x: 0, y: 100), in: group)
        _ = member("left", at: NSPoint(x: 0, y: 400), in: group)
        let right = member("right", at: NSPoint(x: 120, y: 400), in: group)
        XCTAssertEqual(group.ordered.map(\.string), ["left", "right", "lower"])
        // A member that left the window is no longer part of the reading order.
        right.removeFromSuperview()
        XCTAssertEqual(group.ordered.map(\.string), ["left", "lower"])
    }

    func testADragPaintsTheSpanBetweenItsEndpointsEitherWayItRuns() {
        let group = SelectionGroup()
        let top = member("one two three", at: NSPoint(x: 0, y: 500), in: group)
        let bottom = member("four five six", at: NSPoint(x: 0, y: 480), in: group)
        group.begin(top, index: 4)
        group.drag(to: bottom, index: 5)
        XCTAssertEqual(group.selectedText, "two three\nfour ")
        // The same span, dragged upwards from the lower member.
        group.begin(bottom, index: 5)
        group.drag(to: top, index: 4)
        XCTAssertEqual(group.selectedText, "two three\nfour ")
        // A press at the lower member's end drags to a middle of the upper one.
        group.begin(bottom, index: 9)
        group.drag(to: top, index: 8)
        XCTAssertEqual(group.selectedText, "three\nfour five")
        group.selectAll()
        XCTAssertEqual(group.selectedText, "one two three\nfour five six")
    }

    func testBlocksFurtherApartThanALineJoinWithABlankLine() {
        let group = SelectionGroup()
        _ = member("one two three", at: NSPoint(x: 0, y: 500), in: group)
        _ = member("four five six", at: NSPoint(x: 0, y: 440), in: group)
        group.selectAll()
        XCTAssertEqual(group.selectedText, "one two three\n\nfour five six")
    }

    func testCopyLandsOnTheNamedPasteboardAndAPressWithoutADragClears() {
        let group = SelectionGroup()
        _ = member("one two three", at: NSPoint(x: 0, y: 500), in: group)
        _ = member("four five six", at: NSPoint(x: 0, y: 480), in: group)
        let board = NSPasteboard(name: NSPasteboard.Name("SelectionGroupTests"))
        board.clearContents()
        SelectionGroup.pasteboard = board
        defer { SelectionGroup.pasteboard = nil }
        group.selectAll()
        group.copySelection()
        XCTAssertEqual(board.string(forType: .string), "one two three\nfour five six")
        // A press without a drag leaves nothing selected, so a copy takes nothing new.
        group.begin(group.ordered[0], index: 2)
        XCTAssertEqual(group.selectedText, "")
        group.copySelection()
        XCTAssertEqual(board.string(forType: .string), "one two three\nfour five six")
    }

    func testTheNearestMemberBelowThePointerBoundsTheDrag() {
        let group = SelectionGroup()
        let top = member("one two three", at: NSPoint(x: 0, y: 500), in: group)
        let bottom = member("four five six", at: NSPoint(x: 0, y: 470), in: group)
        group.begin(top, index: 4)
        // A point between the two blocks, nearest the lower member, entered from above it.
        let target = group.target(at: NSPoint(x: 10, y: 493))
        XCTAssertEqual(target?.view === bottom, true)
        XCTAssertEqual(target?.index, 0)
        group.drag(to: target!.view, index: target!.index)
        XCTAssertEqual(group.selectedText, "two three")
    }
}
