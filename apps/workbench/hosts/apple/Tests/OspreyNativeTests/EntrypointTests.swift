import Foundation
import XCTest
@testable import OspreyNative

final class EntrypointTests: XCTestCase {
    func testNativeEntrypointIncludesEverySharedApplicationModule() throws {
        let apple = TestResources.apple
        let manifest = try String(contentsOf: apple.appendingPathComponent("app/osprey.toml"), encoding: .utf8)
        let line = try XCTUnwrap(manifest.split(separator: "\n").first { $0.hasPrefix("source_roots = ") })
        let array = try XCTUnwrap(line.firstIndex(of: "["))
        let roots = try JSONDecoder().decode([String].self, from: Data(line[array...].utf8))
        let shared = try FileManager.default.contentsOfDirectory(at: apple.appendingPathComponent("../../src"), includingPropertiesForKeys: nil)
        for file in shared where file.pathExtension == "ospml" && file.lastPathComponent != "main.ospml" {
            XCTAssertTrue(roots.contains("../../../src/" + file.lastPathComponent), "The native app omits shared module \(file.lastPathComponent)")
        }
    }
}

// Implements [UI-EVENTS]: a control sends the revision it was drawn with, so
// a delayed control cannot publish as a newer revision.
private final class DrawingWire {
    static var calls: [(state: String, event: String)] = []
    private static var answers: [String] = []
    /// The library copies every result before it returns, so one managed buffer lives until the next call.
    private static var issued: NSString?

    static func reset(_ script: [String]) {
        calls = []
        answers = script
        issued = nil
    }

    static let entry = LibraryEntry(
        main: { 0 },
        start: { _ in next() },
        dispatch: { state, event in
            calls.append((String(cString: state!), String(cString: event!)))
            return next()
        }
    )

    private static func next() -> UnsafePointer<CChar>? {
        guard !answers.isEmpty else { return nil }
        let text: NSString = answers.removeFirst() as NSString
        issued = text
        return text.utf8String
    }

    static func envelope(_ revision: Int) -> String {
        "{\"version\":1,\"state\":\"s\(revision)\",\"slices\":[],\"patches\":[{\"key\":\"root\",\"revision\":\(revision),\"node\":{\"kind\":\"toggle\",\"label\":\"Control\",\"on\":false,\"event\":0}}],\"removed\":[],\"requests\":[],\"diagnostics\":[],\"theme\":null}"
    }
}

@MainActor
final class DrawingRevisionTests: XCTestCase {
    private func store() -> RegionStore {
        let slices = SliceStore()
        return RegionStore(library: Library(entry: DrawingWire.entry), context: HostContext(platform: "macos", home: "/fixture", clock: { Date(timeIntervalSince1970: 42) }), capabilities: CapabilityRunner(adapters: [], slices: slices), slices: slices)
    }

    private func assertSent(_ index: Int, revision: Int, value: Bool) throws {
        let call = try XCTUnwrap(DrawingWire.calls.dropFirst(index).first)
        XCTAssertEqual(call.state, index == 0 ? "s1" : "s2")
        XCTAssertEqual(JSONValue.parse(call.event), .object(["kind": "ui", "region": "root", "revision": .number(Double(revision)), "ordinal": 0, "value": .bool(value), "now": 42]))
    }

    func testDelayedDrawingRetainsIssuedRevisionWhileFreshDrawingUsesNewRevision() throws {
        DrawingWire.reset([1, 2, 2, 2].map(DrawingWire.envelope))
        let store = store()
        store.start()
        let cell = try XCTUnwrap(store.root)
        let issued = CellView(cell: cell, store: store).drawing
        issued.send(0, .bool(true))
        XCTAssertTrue(store.root === cell)
        XCTAssertEqual(cell.revision, 2)
        let fresh = CellView(cell: cell, store: store).drawing
        issued.send(0, .bool(false))
        fresh.send(0, .bool(true))
        XCTAssertEqual(issued.revision, 1)
        XCTAssertEqual(fresh.revision, 2)
        XCTAssertEqual(DrawingWire.calls.count, 3)
        try assertSent(0, revision: 1, value: true)
        try assertSent(1, revision: 1, value: false)
        try assertSent(2, revision: 2, value: true)
    }
}
