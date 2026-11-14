import AppKit
import SwiftUI
import XCTest

@testable import OspreyShared

// Viewport layout: the scroll frames an application authors, what the checker refuses, and
// that a bounded middle keeps its authored height with its following strip in reach.

final class ViewportLayoutTests: RenderingTestCase {
    /// Metrics the frame cannot carry — negative extents, non-finite numbers, colours outside
    /// `0xRRGGBBAA` — refuse the whole frame instead of laying out.
    func testFrameViewportInsetRejectsNullAndOutOfRangeMetrics() {
        for size in [-1.0, .nan, .infinity, -0.01] {
            let wire = FrameWire()
            wire.row(2, size: size)
            XCTAssertThrowsError(try wire.read(), "size \(size)") { XCTAssertTrue($0 is SharedRefusal, "\($0)") }
        }
        for colour in [Int64(-1), Int64(0x1_0000_0000)] {
            let wire = FrameWire()
            wire.row(2, colour: colour)
            XCTAssertThrowsError(try wire.read(), "colour \(colour)") { XCTAssertTrue($0 is SharedRefusal, "\($0)") }
        }
    }

    /// Flag values a row cannot carry — negative flags or any bit outside the known set —
    /// refuse the frame whole.
    func testFrameScrollFlagRejectsNullAndMalformedValues() {
        for flags in [Int64(-1), Int64(128), Int64(256), Int64(1 << 20)] {
            let wire = FrameWire()
            wire.row(2, flags: flags)
            XCTAssertThrowsError(try wire.read(), "flags \(flags)") { XCTAssertTrue($0 is SharedRefusal, "\($0)") }
        }
        for known in [Int64(1), 2, 4, 8, 16, 32, 64] {
            let wire = FrameWire()
            wire.row(2, flags: known)
            XCTAssertNoThrow(try wire.read(), "known flag \(known)")
        }
    }

    /// A frame with no scroll flag draws no scroll view; the scroll flag installs one; and the
    /// boundaries a row can carry — every fit field — read back exactly as they crossed.
    @MainActor
    func testFrameViewportFieldsPreserveLegacyAbsenceAndValidBoundaries() throws {
        for (scrollY, scrolls) in [(false, 0), (true, 1)] {
            let node = Fixtures.framed(scrollY ? ["scrollY": .bool(true)] : [:], Fixtures.text("Bounded"))
            let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 300)
            defer { window.close() }
            XCTAssertEqual(TestResources.scrollViews(host).count, scrolls, "legacy absence draws no scroll view")
        }
        let wire = FrameWire()
        wire.row(0, words: "Bounded", weight: 600, size: 17, flags: Int64(SharedFlag.disabled) | Int64(SharedFlag.chosen),
                 colour: 0x242_923_FF, background: 0x0A0_A0A_80, width: 464, height: 100, grow: 2.5,
                 border: 0x1234_56FF, padding: 12, spacing: 8, radius: 10)
        let row = try XCTUnwrap(try wire.read().controls.first)
        XCTAssertEqual(row.fit.width, 464)
        XCTAssertEqual(row.fit.height, 100)
        XCTAssertEqual(row.fit.grow, 2.5)
        XCTAssertEqual(row.fit.flags & SharedFlag.all, Int64(SharedFlag.disabled) | Int64(SharedFlag.chosen))
        XCTAssertEqual(row.look.size, 17)
        XCTAssertEqual(row.look.padding, 12)
        XCTAssertEqual(row.look.spacing, 8)
        XCTAssertEqual(row.look.radius, 10)
        XCTAssertEqual(row.look.weight, 600)
        XCTAssertEqual(row.look.ink, 0x242_923_FF)
        XCTAssertEqual(row.look.fill, 0x0A0_A0A_80)
        XCTAssertEqual(row.fit.border, 0x1234_56FF)
    }

    /// A region that replaces its metrics drops its scroll view: the same frame re-sent
    /// without the scroll flag restores bounded layout.
    @MainActor
    func testViewportLayoutUpdatesWhenExistingRegionsReplaceTheirMetrics() throws {
        for scrollY in [true, false] {
            let node = Fixtures.framed(scrollY ? ["scrollY": .bool(true)] : [:], Fixtures.text("Replaced metrics"))
            let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 300)
            defer { window.close() }
            XCTAssertEqual(TestResources.scrollViews(host).count, scrollY ? 1 : 0,
                           "A region-only update must restore bounded layout when viewport metrics are removed")
        }
    }

    /// A growing frame that authors no scroll keeps the page bounded: no scroll view appears.
    @MainActor
    func testDefaultScenesRetainTheirExistingBoundedLayout() throws {
        let node = Fixtures.framed(["grow": .bool(true)], Fixtures.text("Existing application"))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 200)
        defer { window.close() }
        XCTAssertTrue(TestResources.scrollViews(host).isEmpty, "a scene that authors no scroll stays bounded")
    }

    /// A bounded middle keeps its authored height and the strip after it stays in the page.
    @MainActor
    func testNestedRegionRetainsViewportHeightAndItsFollowingStrip() throws {
        let node = page(middleHeight: 440, contentHeight: 506, filler: 262)
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000"), ("page", "#FFFFFF")])), width: 1000, height: 754)
        defer { window.close() }
        XCTAssertEqual(try redRows(try TestResources.raster(host)), 440, "the nested region keeps its viewport height")
        let strip = try TestResources.pixel(try TestResources.raster(host), 100, 740)
        XCTAssertEqual(strip.redComponent, 1, accuracy: 0.1, "the following strip stays after the region")
    }

    /// The viewport an authored scroll uses is its authored height — independent of the window
    /// and of its overflowing content — and its overflow stays reachable to the last row.
    @MainActor
    func testViewportHeightUsesTheContentViewportAndKeepsOverflowReachable() throws {
        let red = Fixtures.framed(["height": .number(500), "fill": "one"], Fixtures.text("", "body", "page"))
        let end = Fixtures.framed(["height": .number(6)], Fixtures.text("end", "body", "page"))
        let node = Fixtures.framed(["scrollY": .bool(true), "height": .number(440), "fill": "page"],
                                   .object(["kind": .string("stack"), "axis": .string("vertical"), "children": .array([red, end])]))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000"), ("page", "#FFFFFF")])), width: 1000, height: 754)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first, "The region must expose content extending past the viewport")
        let document = try XCTUnwrap(scroll.documentView)
        XCTAssertEqual(scroll.contentView.bounds.height, 440, "the viewport is the height the application authored, not the window's")
        XCTAssertGreaterThanOrEqual(document.bounds.height, 506, "the overflowing content keeps its whole height")
        scroll.contentView.scroll(to: NSPoint(x: 0, y: document.bounds.maxY))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertGreaterThan(scroll.contentView.bounds.minY, 0)
        XCTAssertGreaterThanOrEqual(scroll.contentView.bounds.maxY, document.bounds.maxY)
        let bottom = try TestResources.pixel(try TestResources.raster(host), 100, 437)
        XCTAssertGreaterThan(bottom.redComponent, 0.9, "the last rows of the overflow become reachable")
        XCTAssertGreaterThan(bottom.greenComponent, 0.9, "the white end marker is the last thing in view")
    }

    /// A real resize keeps the middle at its authored height, whatever the window does.
    @MainActor
    func testViewportHeightUpdatesOnResizeAndRetainsItsAuthoredMinimum() throws {
        let node = Fixtures.framed(["scrollY": .bool(true), "height": .number(440), "fill": "one"],
                                   Fixtures.framed(["height": .number(506)], Fixtures.text("", "body", "page")))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000")])), width: 400, height: 500)
        defer { window.close() }
        for height in [300, 900] {
            window.setContentSize(NSSize(width: 400, height: height))
            host.layoutSubtreeIfNeeded()
            TestResources.settle()
            XCTAssertEqual(try redRows(try TestResources.raster(host)), 440, "Viewport sizing must follow a real window resize while retaining its minimum")
        }
    }

    // Extra: the boundaries of the valid range — zero extents and the largest colour — read back.
    func testFrameViewportInsetRejectsNullAndOutOfRangeMetrics_extraZeroExtentsAndMaximalColoursAreAccepted() throws {
        let wire = FrameWire()
        wire.row(2, size: 0, colour: 0xFFFF_FFFF, background: 0, width: 0, height: 0, grow: 0,
                 border: 0, padding: 0, spacing: 0, radius: 0)
        let row = try XCTUnwrap(try wire.read().controls.first)
        XCTAssertEqual(row.fit.width, 0)
        XCTAssertEqual(row.fit.height, 0)
        XCTAssertEqual(row.look.ink, 0xFFFF_FFFF)
        XCTAssertEqual(row.look.fill, 0)
    }

    // Extra: a parent that is not an earlier row refuses the frame, forward or self.
    func testFrameScrollFlagRejectsNullAndMalformedValues_extraBadParentsRefuseTheFrame() {
        for parent in [Int64(5), Int64(1)] {
            let wire = FrameWire()
            wire.row(0, words: "first")
            wire.row(0, parent: parent, words: "second")
            XCTAssertThrowsError(try wire.read(), "parent \(parent)") { XCTAssertTrue($0 is SharedRefusal) }
        }
        let wire = FrameWire()
        wire.row(0, words: "only")
        XCTAssertNoThrow(try wire.read())
    }

    // Extra: adding the scroll flag to a frame that lacked it installs the scroll view.
    @MainActor
    func testViewportLayoutUpdatesWhenExistingRegionsReplaceTheirMetrics_extraAddingTheScrollFlagInstallsAScrollView() throws {
        let node = Fixtures.framed(["scrollY": .bool(true)], Fixtures.text("Now scrollable"))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 300)
        defer { window.close() }
        XCTAssertEqual(TestResources.scrollViews(host).count, 1, "the scroll flag installs a scroll view")
    }

    // Extra: a scroll frame inside a growing frame keeps its authored height, not the pane's.
    @MainActor
    func testNestedRegionRetainsViewportHeightAndItsFollowingStrip_extraAScrollFrameInsideAGrowingFrameKeepsItsHeight() throws {
        let inner = Fixtures.framed(["scrollY": .bool(true), "height": .number(200), "fill": "one"],
                                    Fixtures.framed(["height": .number(400)], Fixtures.text("", "body", "page")))
        let node = Fixtures.framed(["grow": .bool(true)], inner)
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000")])), width: 400, height: 600)
        defer { window.close() }
        XCTAssertEqual(try redRows(try TestResources.raster(host)), 200, "the authored height wins over the growing pane")
    }

    // Extra: a middle taller than the window still shows its authored height.
    @MainActor
    func testViewportHeightUpdatesOnResizeAndRetainsItsAuthoredMinimum_extraATallerMiddleKeepsItsAuthoredHeight() throws {
        let node = Fixtures.framed(["scrollY": .bool(true), "height": .number(680), "fill": "one"],
                                   Fixtures.framed(["height": .number(900)], Fixtures.text("", "body", "page")))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("one", "#FF0000")])), width: 400, height: 600)
        defer { window.close() }
        XCTAssertEqual(try redRows(try TestResources.raster(host)), 680, "the frame keeps its authored height, whatever window it was given")
    }

    // Extra: content shorter than the viewport needs no scrolling — document equals viewport.
    @MainActor
    func testViewportHeightUsesTheContentViewportAndKeepsOverflowReachable_extraShortContentNeedsNoScrolling() throws {
        let node = Fixtures.framed(["scrollY": .bool(true), "height": .number(440)],
                                   Fixtures.framed(["height": .number(100)], Fixtures.text("Short")))
        let (host, window) = TestResources.hosted(NodeView(node: node, drawing: Fixtures.drawing()), width: 400, height: 600)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first)
        XCTAssertEqual(scroll.contentView.bounds.height, 440, "the viewport is the authored height")
        XCTAssertEqual(try XCTUnwrap(scroll.documentView).bounds.height, 100, "the content keeps its natural height")
    }

    // Extra: a control's action crosses as the control number a host event names.
    func testFrameViewportFieldsPreserveLegacyAbsenceAndValidBoundaries_extraAControlsActionCrossesAsItsEventNumber() throws {
        let wire = FrameWire()
        wire.row(3, action: 7, words: "Take one")
        let row = try XCTUnwrap(try wire.read().controls.first)
        XCTAssertEqual(row.action, 7, "the control number a host event names")
        XCTAssertEqual(row.words, "Take one")
        XCTAssertEqual(row.children, [])
    }


    // Extra: an authored width is honoured exactly by the layout.
    @MainActor
    func testDefaultScenesRetainTheirExistingBoundedLayout_extraAnAuthoredWidthIsHonouredExactly() {
        for width in [160, 464] {
            let node = Fixtures.framed(["width": .number(Double(width))], Fixtures.text("Measured"))
            let size = TestResources.measured(NodeView(node: node, drawing: Fixtures.drawing()), width: 1000, height: 100)
            XCTAssertEqual(size.width, CGFloat(width), "the authored width crosses to the layout")
        }
    }

    // MARK: the page fixture

    /// A white page: a white strip, a red bounded middle of `middleHeight` scrolling content
    /// `contentHeight` tall, then a white strip and `filler` more white page below.
    private func page(middleHeight: Int, contentHeight: Int, filler: Int) -> Tree {
        let middle = Fixtures.framed(["scrollY": .bool(true), "height": .number(Double(middleHeight)), "fill": "one"],
                                     Fixtures.framed(["height": .number(Double(contentHeight))], Fixtures.text("", "body", "page")))
        let strip = Fixtures.framed(["height": .number(300)], Fixtures.text("", "body", "page"))
        let tail = Fixtures.framed(["height": .number(Double(200 + filler))], Fixtures.text("", "body", "page"))
        return Fixtures.framed(["grow": .bool(true), "fill": "page"],
                               .object(["kind": .string("stack"), "axis": .string("vertical"), "children": .array([strip, middle, tail])]))
    }

    @MainActor
    private func redRows(_ bitmap: NSBitmapImageRep) throws -> Int {
        try (0..<bitmap.pixelsHigh).filter { y in
            let colour = try TestResources.pixel(bitmap, 100, y)
            return colour.redComponent > 0.9 && colour.greenComponent < 0.1 && colour.blueComponent < 0.1
        }.count
    }
}
