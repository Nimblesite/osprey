import Foundation
import XCTest
@testable import OspreyNative

private enum BorrowedImportWire {
    static let key = "test:borrowed-import-ownership"
    static var releases = 0
    static let entry = LibraryEntry(main: { 0 }, start: { _ in copied() },
        dispatch: { _, _ in copied() }, ownershipVersion: { 1 }, releaseString: { pointer in
            if let pointer { free(UnsafeMutablePointer(mutating: pointer)) }
            releases += 1
        })

    static func copied() -> UnsafePointer<CChar>? {
        strdup(SliceStore.shared.pointer(key)).map { UnsafePointer($0) }
    }
}

@MainActor
final class BorrowedImportOwnershipTests: XCTestCase {
    func testCopiedImportCapabilityRequiresExportReleaseEntryPoint() {
        let entry = LibraryEntry(main: { 0 }, start: { $0 }, dispatch: { state, _ in state }, ownershipVersion: { 1 })
        XCTAssertThrowsError(try Library(entry: entry).initialize()) {
            XCTAssertEqual($0 as? LibraryError, .missingRelease)
        }
    }

    func testTwentyThousandChangedDispatchBuffersRemainBounded() throws {
        let slices = SliceStore.shared
        let baseline = slices.retainedBytes
        let releases = BorrowedImportWire.releases
        let library = Library(entry: BorrowedImportWire.entry)
        for revision in 0..<20_000 {
            let prefix = "\(revision) → "
            let text = prefix + String(repeating: "x", count: 8192 - prefix.utf8.count)
            slices.write(BorrowedImportWire.key, text)
            XCTAssertEqual(try library.dispatch(state: "", event: ""), text)
        }
        XCTAssertLessThanOrEqual(slices.retainedBytes - baseline, 8193)
        XCTAssertEqual(BorrowedImportWire.releases - releases, 20_000)
        XCTAssertEqual(slices.borrowedBytes, 0)
    }

    func testOutputsCopyWhileContextStateAndEventInputsAreBorrowed() throws {
        let state = "state é→"
        let event = "event 𝑝"
        let stateLibrary = Library(entry: LibraryEntry(main: { 0 }, start: { $0 }, dispatch: { state, _ in state }))
        let eventLibrary = Library(entry: LibraryEntry(main: { 0 }, start: { $0 }, dispatch: { _, event in event }))
        XCTAssertTrue(try stateLibrary.start(context: state).utf8.elementsEqual(state.utf8))
        XCTAssertTrue(try stateLibrary.dispatch(state: state, event: event).utf8.elementsEqual(state.utf8))
        XCTAssertTrue(try eventLibrary.dispatch(state: state, event: event).utf8.elementsEqual(event.utf8))
    }

    func testCancelledSnapshotReleasesPinnedBuffers() async throws {
        let slices = SliceStore()
        slices.write("payload", String(repeating: "x", count: 8192))
        let task = Task { () throws -> Void in
            for _ in 0..<20_000 {
                do {
                    try slices.withSnapshot {
                        XCTAssertEqual(strlen(slices.pointer("payload")), 8192)
                        try Task.checkCancellation()
                        XCTFail("Cancelled task must throw")
                    }
                } catch is CancellationError { }
                XCTAssertEqual(slices.borrowedBytes, 0)
            }
        }
        task.cancel()
        try await task.value
        XCTAssertEqual(slices.borrowedBytes, 0)
        XCTAssertEqual(slices.retainedBytes, 0)
    }

    func testConcurrentWritesCannotInvalidatePinnedImportBuffers() throws {
        let slices = SliceStore()
        let original = "é→" + String(repeating: "x", count: 8192)
        slices.write("payload", original)
        try slices.withSnapshot {
            let pointer = slices.pointer("payload")
            DispatchQueue.concurrentPerform(iterations: 1_000) { revision in
                slices.write("payload", "\(revision) → changed")
            }
            XCTAssertEqual(String(cString: pointer), original)
            XCTAssertEqual(slices.pointer("payload"), pointer)
        }
        let latest = slices.text("payload")
        try slices.withSnapshot { XCTAssertEqual(String(cString: slices.pointer("payload")), latest) }
        XCTAssertEqual(slices.borrowedBytes, 0)
        XCTAssertEqual(slices.retainedBytes, 0)
    }
}

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
