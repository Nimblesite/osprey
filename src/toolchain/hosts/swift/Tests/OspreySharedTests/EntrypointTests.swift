import Foundation
import XCTest

@testable import OspreyShared

// The entry point's half of the boundary: the host-held slices an application's imports borrow,
// the frame checker that refuses unusable rows, the revision a control was drawn with, and the
// modules the native application compiles. Every fact the Workbench's entry-point tests held,
// against the shared-memory frame that now carries it ([NATIVE-SHM]).

/// The frame a faked library tells: one theme record and one region record, as [NATIVE-SHM-NODES]
/// spells them, so a test can read the typed tree a host rebuilds.
private func toldFrame() throws -> Shown {
    let wire = FrameWire()
    wire.row(34, words: "", hint: "theme", weight: 0)                 // theme record
    wire.row(35, parent: 0, words: "", hint: "tokens", weight: 0)       // its tokens list
    wire.row(34, parent: 1, words: "", hint: "", weight: 0)             // one token record (unnamed)
    wire.row(33, parent: 2, words: "text", hint: "name", weight: 0)
    wire.row(34, parent: 0, words: "", hint: "region", weight: 0)       // one region record
    wire.row(33, parent: 4, words: "root", hint: "key", weight: 0)
    wire.row(34, parent: 4, words: "", hint: "node", weight: 0)         // the region's node record
    wire.row(33, parent: 6, words: "button", hint: "kind", weight: 0)
    wire.row(33, parent: 6, words: "", hint: "event", weight: 1, colour: 7)
    return try wire.read()
}

final class EntrypointTests: RenderingTestCase {
    // A cancelled import changes nothing: the pinned buffer it borrowed stays exact, the store
    // still answers every key, and a missing key answers empty — never a null pointer.
    func testCancelledSnapshotReleasesPinnedBuffers() async throws {
        let slices = SliceStore()
        slices.write("payload", String(repeating: "x", count: 8192))
        let pinned = slices.pointer("payload")
        let reader = Task { () -> String in
            let borrowed = slices.pointer("payload")
            try Task.checkCancellation()
            return String(cString: borrowed)
        }
        reader.cancel()
        _ = try? await reader.value
        XCTAssertEqual(String(cString: pinned), slices.text("payload"))
        XCTAssertEqual(slices.keys(), ["payload"])
        XCTAssertEqual(String(cString: nativeSlice(nil)), "", "a missing key answers empty, never null")
    }

    // The bytes an import borrowed stay exact while the same key is rewritten concurrently, and
    // an unchanged key hands out the one pinned buffer it holds.
    func testConcurrentWritesCannotInvalidatePinnedImportBuffers() throws {
        let slices = SliceStore()
        let original = "é→" + String(repeating: "x", count: 8192)
        slices.write("payload", original)
        withUnsafePointer(to: slices.pointer("payload")) { held in
            let pinned = held.pointee
            DispatchQueue.concurrentPerform(iterations: 1_000) { revision in
                slices.write("payload", "\(revision) → changed")
            }
            XCTAssertEqual(String(cString: pinned), original, "a pinned import buffer must never change underneath its reader")
        }
        let latest = slices.text("payload")
        let repinned = slices.pointer("payload")
        slices.write("payload", latest)
        XCTAssertEqual(String(cString: slices.pointer("payload")), latest)
        XCTAssertEqual(slices.pointer("payload"), repinned, "a rewrite of the same text keeps the one pinned buffer")
        XCTAssertEqual(slices.keys(), ["payload"])
    }

    // The ownership rule an entry point must meet, as the frame states it: a row that is not
    // usable — an unknown kind, an unknown flag, a weight outside 100 to 900, a negative extent,
    // a non-finite number — refuses the whole frame as a typed error instead of drawing.
    func testCopiedImportCapabilityRequiresExportReleaseEntryPoint() throws {
        for row in [(kind: Int64(17), flags: Int64(0), weight: Int64(400), size: 13.0),
                    (kind: Int64(3), flags: Int64(128), weight: Int64(400), size: 13.0),
                    (kind: Int64(3), flags: Int64(0), weight: Int64(99), size: 13.0),
                    (kind: Int64(3), flags: Int64(0), weight: Int64(901), size: 13.0),
                    (kind: Int64(3), flags: Int64(0), weight: Int64(400), size: -1),
                    (kind: Int64(3), flags: Int64(0), weight: Int64(400), size: .nan),
                    (kind: Int64(3), flags: Int64(-1), weight: Int64(400), size: 13.0)] {
            let wire = FrameWire()
            wire.row(row.kind, weight: row.weight, size: row.size, flags: row.flags)
            XCTAssertThrowsError(try wire.read(), "row \(row) must refuse the frame") { error in
                XCTAssertTrue(error is SharedRefusal, "\(error)")
            }
        }
        let told = try toldFrame()
        XCTAssertEqual(told.controls.compactMap(\.action), [], "a told row delivers no control number")
        let badWeight = FrameWire()
        badWeight.row(33, words: "x", weight: 5)
        XCTAssertThrowsError(try badWeight.read()) { error in XCTAssertTrue(error is SharedRefusal, "\(error)") }
    }

    // A control keeps the revision of the frame it was drawn with: a delayed click on a control
    // whose binding changed is refused, a click on the fresh control is honoured at the new
    // revision, and the drawing itself still names the revision that issued it.
    @MainActor
    func testDelayedDrawingRetainsIssuedRevisionWhileFreshDrawingUsesNewRevision() throws {
        let node = Fixtures.framed([:], .object(["kind": .string("button"), "label": .string("Control"), "enabled": .bool(true), "event": .number(3)]))
        let cell = RegionCell(key: "root", revision: 1, node: node)
        let issued = Fixtures.drawing { _, _ in }
        XCTAssertEqual(issued.revision, 1)
        XCTAssertEqual(cell.controlRevision(for: node["body"] ?? .null, value: .null), 1)
        cell.replace(revision: 2, node: node)
        XCTAssertEqual(cell.controlRevision(for: node["body"] ?? .null, value: .null), 2, "an unchanged binding keeps its control at the fresh revision")
        var changed = node.object ?? [:]
        changed["body"] = .object(["kind": .string("button"), "label": .string("Changed"), "enabled": .bool(true), "event": .number(3)])
        cell.replace(revision: 3, node: .object(changed))
        XCTAssertNil(cell.controlRevision(for: node["body"] ?? .null, value: .null), "a delayed click on a replaced binding must be refused")
        XCTAssertEqual(issued.revision, 1, "the issued drawing keeps its revision while the cell moves on")
        XCTAssertNil(cell.controlRevision(for: node["body"] ?? .null, value: .string("unexpected")), "a control event must carry the value kind it declared")
    }

    // The application compiles every module it shares: each declared source root exists, every
    // module under the application's own source is inside a declared root, and the shared
    // component vocabulary (`@osprey/native`, whose told rows and renderer this host reads)
    // is compiled in.
    func testNativeEntrypointIncludesEverySharedApplicationModule() throws {
        let app = Repo.src.appendingPathComponent("osprey/workbench")
        let manifest = try String(contentsOf: app.appendingPathComponent("osprey.toml"), encoding: .utf8)
        let line = try XCTUnwrap(manifest.split(separator: "\n").first { $0.hasPrefix("source_roots = ") })
        let roots = try line.dropFirst("source_roots = ".count).split(separator: ",").map { $0.trimmingCharacters(in: CharacterSet(charactersIn: " []\"")) }
        let resolved = roots.map { root -> URL in
            let url = app.appendingPathComponent(root).standardizedFileURL
            XCTAssertTrue(FileManager.default.fileExists(atPath: url.path), "declared source root does not exist: \(root)")
            return url
        }
        XCTAssertTrue(roots.contains("../packages/native/src"), "the application must compile the shared component vocabulary")
        let sources = try FileManager.default.contentsOfDirectory(at: app.appendingPathComponent("src"), includingPropertiesForKeys: nil)
        for file in sources where file.pathExtension == "ospml" {
            XCTAssertTrue(resolved.contains { file.path.hasPrefix($0.path + "/") }, "The native app omits shared module \(file.lastPathComponent)")
        }
    }

    // The frame's text is copied into checked values: once `read` answers, the words a row
    // carried survive the borrowed bytes being reused, exactly as borrowed inputs survive a call.
    func testOutputsCopyWhileContextStateAndEventInputsAreBorrowed() throws {
        let wire = FrameWire()
        wire.row(2, words: "state é→", hint: "event 𝑝")
        let shown = try wire.read()
        XCTAssertEqual(shown.controls[0].words, "state é→")
        XCTAssertEqual(shown.controls[0].hint, "event 𝑝")
        _ = try wire.read()
        XCTAssertEqual(shown.controls[0].words, "state é→", "a checked frame owns its words; the borrowed run can be reused")
        XCTAssertEqual(shown.controls[0].hint, "event 𝑝")
    }

    // Twenty thousand changed dispatch outputs through one slice key keep one pinned buffer and
    // the latest text; identical writes rebind nothing and add no key.
    func testTwentyThousandChangedDispatchBuffersRemainBounded() throws {
        let slices = SliceStore()
        for revision in 0..<20_000 {
            let prefix = "\(revision) → "
            slices.write("dispatch", prefix + String(repeating: "x", count: 8192 - prefix.utf8.count))
            XCTAssertEqual(slices.text("dispatch").utf8.count, 8192)
        }
        XCTAssertEqual(slices.keys(), ["dispatch"])
        let latest = slices.text("dispatch")
        let pinned = slices.pointer("dispatch")
        for _ in 0..<1000 { slices.write("dispatch", latest) }
        XCTAssertEqual(slices.text("dispatch"), latest)
        XCTAssertEqual(slices.pointer("dispatch"), pinned, "a rewrite of the same text keeps the one pinned buffer")
        XCTAssertEqual(String(cString: pinned).utf8.count, 8192)
        XCTAssertEqual(slices.keys(), ["dispatch"])
    }

    // Extra: identical concurrent writes of one text keep the store stable and never duplicate
    // the key, so a repeated dispatch of an unchanged output costs nothing.
    func testConcurrentWritesCannotInvalidatePinnedImportBuffers_extraIdenticalConcurrentWritesKeepTheStoreStable() throws {
        let slices = SliceStore()
        let text = "unchanged dispatch output é→"
        for round in 0..<10 { slices.write("key-\(round % 2)", text) }
        DispatchQueue.concurrentPerform(iterations: 500) { round in
            slices.write("key-\(round % 2)", text)
        }
        XCTAssertEqual(slices.keys(), ["key-0", "key-1"])
        XCTAssertEqual(slices.text("key-0"), text)
        XCTAssertEqual(slices.text("key-1"), text)
    }

    // Extra: every plain control kind the frame numbers reaches the checked view, with its
    // words, hint and control number, and its children hang under their parent row.
    func testOutputsCopyWhileContextStateAndEventInputsAreBorrowed_extraEveryPlainControlKindReadsBackWithItsWordsAndAction() throws {
        let wire = FrameWire()
        for kind in SharedKind.allCases where !kind.told {
            wire.row(kind.rawValue, action: Int64(kind.rawValue), words: "c\(kind.rawValue)", hint: "h\(kind.rawValue)")
        }
        let shown = try wire.read()
        XCTAssertEqual(shown.controls.count, SharedKind.allCases.count - 3)
        XCTAssertEqual(shown.roots, Array(0..<(SharedKind.allCases.count - 3)))
        XCTAssertEqual(shown.controls[3].kind, .press)
        XCTAssertEqual(shown.controls[3].words, "c3")
        XCTAssertEqual(shown.controls[3].hint, "h3")
        XCTAssertEqual(shown.controls[3].action, 3)
    }

    // Extra: told records rebuild their typed tree — lists, members by name, whole numbers,
    // reals, flags and negative reals — exactly as [NATIVE-SHM-NODES] reads them.
    func testDelayedDrawingRetainsIssuedRevisionWhileFreshDrawingUsesNewRevision_extraToldRowsRebuildTheTypedTree() throws {
        let wire = FrameWire()
        wire.row(34, words: "", hint: "region", weight: 0)
        wire.row(33, parent: 0, words: "root", hint: "key", weight: 0)
        wire.row(34, parent: 0, words: "", hint: "node", weight: 0)
        wire.row(35, parent: 2, words: "", hint: "items", weight: 0)
        wire.row(33, parent: 3, words: "", hint: "", weight: 3, colour: 1)
        wire.row(33, parent: 3, words: "", hint: "", weight: 1, colour: -7)
        wire.row(33, parent: 3, words: "", hint: "", weight: 2, size: 2.5)
        wire.row(33, parent: 3, words: "", hint: "", weight: 4, size: 1.5)
        wire.row(33, parent: 3, words: "text", hint: "", weight: 0)
        let frame = try XCTUnwrap(Told.frame(try wire.read()))
        XCTAssertEqual(frame.regions.map(\.key), ["root"])
        XCTAssertEqual(frame.regions[0].node, .object(["items": .array([.bool(true), .number(-7), .number(2.5), .number(-1.5), .string("text")])]))
    }

    // Extra: a parent that is not an earlier row refuses the frame, forward and self references
    // both, and a control number below zero delivers no action.
    func testCancelledSnapshotReleasesPinnedBuffers_extraBadParentsAndActionsRefuseOrVanish() throws {
        for parent in [Int64(5), Int64(1)] {
            let wire = FrameWire()
            wire.row(2, words: "first")
            wire.row(2, parent: parent, words: "second")
            XCTAssertThrowsError(try wire.read(), "parent \(parent)")
        }
        let wire = FrameWire()
        wire.row(3, action: -1, words: "no action")
        XCTAssertEqual(try wire.read().controls[0].action, nil)
    }

    // Extra: text ranges are bounds-checked on UTF-8 boundaries — a range past the run, below
    // zero or splitting a character refuses the frame whole.
    func testTwentyThousandChangedDispatchBuffersRemainBounded_extraTextRangesOutsideTheRunRefuseTheFrame() throws {
        for (offset, length) in [(Int64(100), Int64(1)), (Int64(0), Int64(100)), (Int64(-1), Int64(1)), (Int64(1), Int64(-1)), (Int64(0), Int64(1))] {
            let wire = FrameWire()
            wire.row(2, words: "é")
            wire.moveText(of: 0, to: offset, length)
            XCTAssertThrowsError(try wire.read(), "range \(offset)..\(offset + length)")
        }
    }

    // Extra: the application's declared entry module is exactly the one its manifest names.
    func testNativeEntrypointIncludesEverySharedApplicationModule_extraTheManifestNamesOneEntryPoint() throws {
        let app = Repo.src.appendingPathComponent("osprey/workbench")
        let manifest = try String(contentsOf: app.appendingPathComponent("osprey.toml"), encoding: .utf8)
        let entry = try XCTUnwrap(manifest.split(separator: "\n").first { $0.hasPrefix("entry = ") }).dropFirst("entry = ".count).trimmingCharacters(in: CharacterSet(charactersIn: " \""))
        XCTAssertTrue(FileManager.default.fileExists(atPath: app.appendingPathComponent(entry).path), "the manifest's entry module must exist")
    }

    // Extra: the ask rows a frame carries answer as typed asks — a capability request, a slice
    // write and a diagnostic each keep their code, ticket and fields, and an unknown code refuses.
    func testCopiedImportCapabilityRequiresExportReleaseEntryPoint_extraAskRowsAnswerAsTypedAsksAndRefuseUnknownCodes() throws {
        let wire = FrameWire()
        wire.ask(0, 1, "run:1")
        wire.ask(1, 0, "/bin/echo")
        wire.ask(3, 1000, "")
        wire.ask(0, 100, "slice-key")
        wire.ask(1, 0, "slice text")
        wire.ask(0, 101, "why it was refused")
        let shown = try wire.read()
        XCTAssertEqual(shown.requests.map(\.kind), ["run"])
        XCTAssertEqual(shown.requests[0].text(1), "/bin/echo")
        XCTAssertEqual(shown.requests[0].number(3), 1000)
        XCTAssertEqual(shown.slices.count, 1)
        XCTAssertEqual(shown.slices[0].key, "slice-key")
        XCTAssertEqual(shown.slices[0].text, "slice text")
        XCTAssertEqual(shown.diagnostics, ["why it was refused"])
        let bad = FrameWire()
        bad.ask(0, 99, "unknown")
        XCTAssertThrowsError(try bad.read()) { XCTAssertTrue($0 is SharedRefusal, "\($0)") }
    }
}
