import AppKit
import SwiftUI
import XCTest
@testable import OspreyNative

private enum ViewportFixture {
    static let text: NSString? = encoded()
    static let nestedText: NSString? = encoded(nested: true)
    static let boundedText: NSString? = encoded(viewport: false)

    static func frame(_ values: [String: Any], body: [String: Any]? = nil) -> [String: Any] {
        ["kind": "framed", "style": TestResources.frameJSON(values),
         "body": body ?? ["kind": "text", "text": "", "role": "body", "tone": "primary"]]
    }

    static func encoded(nested: Bool = false, viewport: Bool = true) -> NSString? {
        let nodes = [frame(["height": 205]), frame(["height": 440, "viewportHeightInset": viewport ? 270 : -1, "fill": "surface"]), frame(["height": 109])]
        let middle: [String: Any] = ["kind": "layer", "children": [["kind": "slot", "key": "board"]]]
        let nestedNodes = [nodes[0], middle, frame(["height": 43])]
        let root = frame(["grow": true, "fill": "canvas"], body: ["kind": "stack", "axis": "vertical", "children": nested ? nestedNodes : nodes])
        let board: [String: Any] = ["kind": "stack", "axis": "vertical", "children": [nodes[1], frame(["height": 66])]]
        let theme: [String: Any] = ["tokens": [["name": "canvas", "light": "#FFFFFF", "dark": "#FFFFFF"],
            ["name": "surface", "light": "#FF0000", "dark": "#FF0000"]], "spacing": [0], "feedback": 0, "settle": 0]
        let revision = viewport ? 1 : 2
        let envelope: [String: Any] = ["version": 1, "state": "viewport-fixture", "patches": [["key": "root", "revision": revision, "node": root], ["key": "board", "revision": revision, "node": board]],
            "removed": [], "requests": [], "slices": [], "diagnostics": []]
        guard let bytes = try? JSONSerialization.data(withJSONObject: envelope.merging(viewport ? ["theme": theme] : [:]) { _, value in value }) else { return nil }
        return String(data: bytes, encoding: .utf8).map { $0 as NSString }
    }
}

private func viewportMain() -> Int32 { 0 }
private func viewportStart(_ context: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { ViewportFixture.text?.utf8String }
private func nestedViewportStart(_ context: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { ViewportFixture.nestedText?.utf8String }
private func viewportDispatch(_ state: UnsafePointer<CChar>?, _ event: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { ViewportFixture.boundedText?.utf8String }

@MainActor
final class ViewportLayoutTests: XCTestCase {
    func testFrameViewportInsetRejectsNullAndOutOfRangeMetrics() {
        let invalid: [Any] = [NSNull(), -2, 32769, 1.5, "270", true]
        for value in invalid {
            XCTAssertThrowsError(try TestResources.frame(["viewportHeightInset": value]), "Invalid viewport metric: \(value)")
        }
    }

    func testFrameScrollFlagRejectsNullAndMalformedValues() {
        let invalid: [Any] = [NSNull(), "false", 0, [Any](), [String: Any]()]
        for value in invalid {
            XCTAssertThrowsError(try TestResources.frame(["scrollY": value]), "Invalid scroll flag: \(value)")
        }
    }

    func testFrameViewportFieldsPreserveLegacyAbsenceAndValidBoundaries() throws {
        let legacy = try TestResources.frame([:])
        XCTAssertNil(legacy.viewportHeightInset)
        XCTAssertNil(legacy.scrollY)
        for inset in [-1, 0, 32768] {
            XCTAssertEqual(try TestResources.frame(["viewportHeightInset": inset]).viewportHeightInset, inset)
        }
        for flag in [false, true] {
            XCTAssertEqual(try TestResources.frame(["scrollY": flag]).scrollY, flag)
        }
    }

    func testViewportLayoutUpdatesWhenExistingRegionsReplaceTheirMetrics() throws {
        let store = fixture()
        store.start()
        let (host, window) = TestResources.hosted(RootView(store: store), width: 1488, height: 1058)
        defer { window.close() }
        XCTAssertFalse(TestResources.scrollViews(host).isEmpty)
        store.lifecycle("background")
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        XCTAssertNil(store.failure)
        XCTAssertTrue(TestResources.scrollViews(host).isEmpty, "A region-only update must restore bounded layout when viewport metrics are removed")
    }

    func testDefaultScenesRetainTheirExistingBoundedLayout() throws {
        let node = Node.framed(try TestResources.frame(["grow": true]), .text("Existing application", .body, .primary))
        XCTAssertFalse(node.authorsViewport)
        let (host, window) = TestResources.hosted(ViewportContent(active: node.authorsViewport) { Color.clear }, width: 400, height: 200)
        defer { window.close() }
        XCTAssertTrue(TestResources.scrollViews(host).isEmpty)
    }

    func testNestedRegionRetainsViewportHeightAndItsFollowingStrip() throws {
        let store = fixture(nested: true)
        store.start()
        let (host, window) = TestResources.hosted(RootView(store: store), width: 1488, height: 1058)
        defer { window.close() }
        XCTAssertNil(store.failure)
        let document = try XCTUnwrap(TestResources.scrollViews(host).first?.documentView)
        XCTAssertEqual(document.bounds.height, 1102)
        XCTAssertEqual(redRows(try TestResources.raster(document)), 788)
    }

    func testViewportHeightUsesTheContentViewportAndKeepsOverflowReachable() throws {
        let store = fixture()
        store.start()
        XCTAssertNil(store.failure)
        let (host, window) = TestResources.hosted(RootView(store: store), width: 1488, height: 1058)
        defer { window.close() }
        let bitmap = try TestResources.raster(host)
        XCTAssertEqual(redRows(bitmap), 788, "The authored viewport inset must use 1058 content points, independent of the overflowing page height")
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first, "The page must expose content extending past the viewport")
        let document = try XCTUnwrap(scroll.documentView)
        XCTAssertGreaterThanOrEqual(document.bounds.height, 1102)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: document.bounds.maxY))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertGreaterThan(scroll.contentView.bounds.minY, 0)
        XCTAssertGreaterThanOrEqual(scroll.contentView.bounds.maxY, document.bounds.maxY)
    }

    func testViewportHeightUpdatesOnResizeAndRetainsItsAuthoredMinimum() throws {
        let store = fixture()
        store.start()
        let (host, window) = TestResources.hosted(RootView(store: store), width: 1488, height: 1058)
        defer { window.close() }
        for (height, expected) in [(680, 440), (1400, 1130)] {
            window.setContentSize(NSSize(width: 1488, height: height))
            host.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
            XCTAssertEqual(redRows(try TestResources.raster(host)), expected, "Viewport sizing must follow a real window resize while retaining its minimum")
        }
    }

    private func fixture(nested: Bool = false) -> RegionStore {
        let entry = nested ? LibraryEntry(main: viewportMain, start: nestedViewportStart, dispatch: viewportDispatch)
            : LibraryEntry(main: viewportMain, start: viewportStart, dispatch: viewportDispatch)
        let library = Library(entry: entry)
        return RegionStore(library: library, context: HostContext(platform: "test", home: ""), capabilities: CapabilityRunner(adapters: []))
    }

    private func redRows(_ bitmap: NSBitmapImageRep) -> Int {
        (0..<bitmap.pixelsHigh).filter { y in
            guard let color = bitmap.colorAt(x: 100, y: y)?.usingColorSpace(.sRGB) else { return false }
            return color.redComponent > 0.9 && color.greenComponent < 0.1 && color.blueComponent < 0.1
        }.count
    }
}
