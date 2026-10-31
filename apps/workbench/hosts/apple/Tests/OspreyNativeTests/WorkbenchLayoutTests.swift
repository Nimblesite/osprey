import AppKit
import OspreyApp
import SwiftUI
import XCTest
@testable import OspreyNative

// The real application uses this same host-held slice boundary.
@_cdecl("workbench_slice")
func layoutSlice(_ key: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    let name = key.map { String(cString: $0) } ?? ""
    if name == "lessons:progress" { NativeImportAccounting.reads += 1 }
    return SliceStore.shared.pointer(name)
}

private struct LayoutSnapshot {
    let envelope: Envelope
    let regions: [String: Patch]
}

@MainActor
final class WorkbenchLayoutTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        XCTAssertEqual(setenv("OSPREY_ARC_DEBUG", "1", 1), 0)
    }

    func testActualApplicationImportDispatchAndCancellationMemoryPlateau() async throws {
        let library = workbenchLibrary()
        var state = try Envelope.decode(library.start(context: #"{"platform":"macos","now":1,"offset":0,"home":"","arguments":[]}"#)).state
        for revision in 0..<200 { state = try importedProgress(library, state: state, revision: revision) }
        let baseline = try NativeImportAccounting.sample()
        XCTAssertGreaterThan(baseline.objects, 0, "ARC counters must be armed")
        let reads = NativeImportAccounting.reads
        for revision in 0..<20_000 { state = try importedProgress(library, state: state, revision: revision) }
        try assertImportPlateau(baseline)
        try await cancelledImports(library, state: state)
        try assertImportPlateau(baseline)
        XCTAssertEqual(NativeImportAccounting.reads - reads, 40_000)
        XCTAssertEqual(SliceStore.shared.borrowedBytes, 0)
    }

    private func importedProgress(_ library: Library, state: String, revision: Int) throws -> String {
        try autoreleasepool {
            let prefix = "{\"completed\":[\"soak-held\"],\"padding\":\"\(revision) é→"
            let payload = prefix + String(repeating: "x", count: 8192 - prefix.utf8.count - 2) + "\"}"
            SliceStore.shared.write("lessons:progress", payload)
            let event = #"{"kind":"completed","ticket":"lessons:progress","ok":true,"result":{},"now":1}"#
            let next = try Envelope.decode(library.dispatch(state: state, event: event))
            XCTAssertTrue(next.diagnostics.isEmpty, next.diagnostics.joined(separator: "; "))
            XCTAssertTrue(next.state.contains("soak-held"), "The actual app must consume the imported progress")
            return next.state
        }
    }

    private func cancelledImports(_ library: Library, state: String) async throws {
        for revision in 0..<20_000 {
            let task = Task {
                _ = try importedProgress(library, state: state, revision: revision)
                try Task.checkCancellation()
            }
            task.cancel()
            do { try await task.value; XCTFail("Cancellation must remain a failure") }
            catch is CancellationError { }
        }
    }

    private func assertImportPlateau(_ baseline: NativeImportAccounting.Sample) throws {
        let current = try NativeImportAccounting.sample()
        XCTAssertEqual(current.objects, baseline.objects)
        XCTAssertEqual(current.bytes, baseline.bytes)
        XCTAssertLessThanOrEqual(current.resident, baseline.resident + 16 * 1024 * 1024)
        XCTAssertEqual(SliceStore.shared.borrowedBytes, 0)
    }

    func testRealAppKeepsPastedProofWhenSelectionAndTextNotificationsArriveTogether() async throws {
        try await exerciseTyping("Theorem self. For all p, p → p.\nProof.\nAssume h : p.\nFrom h.\n", append: false)
    }

    func testRealAppKeepsSubsequentTypingInTheCurrentFormulaEditor() async throws {
        try await exerciseTyping("p → q\n", append: true)
    }

    func testRealAppKeepsTheCaretAfterMiddleUnicodeInsertion() async throws {
        try await middleUnicodeInsertion(waitForSelection: true)
    }

    func testRealAppKeepsUnicodeInsertionImmediatelyAfterMovingTheCaret() async throws {
        try await middleUnicodeInsertion(waitForSelection: false)
    }

    private func middleUnicodeInsertion(waitForSelection: Bool) async throws {
        let store = typingStore()
        let (host, window) = try documentHost(store)
        defer { window.close() }
        let text = try currentEditor(host)
        text.insertText("𝑝q\n", replacementRange: NSRange(location: 0, length: 0))
        try await assertAccepted("𝑝q\n", store: store)
        let refreshed = try currentEditor(host)
        refreshed.setSelectedRange(NSRange(location: 2, length: 0))
        if waitForSelection { try await assertAccepted("𝑝q\n", store: store, caret: 4) }
        let positioned = try currentEditor(host)
        positioned.insertText("→", replacementRange: positioned.selectedRange())
        try await assertAccepted("𝑝→q\n", store: store, caret: 7)
    }

    private func typingStore() -> RegionStore {
        NativeTypingWire.latest = ""
        let library = workbenchLibrary(start: nativeTypingStart, dispatch: nativeTypingDispatch)
        let store = RegionStore(library: library, context: HostContext(platform: "test", home: ""), capabilities: CapabilityRunner(adapters: []))
        store.start()
        XCTAssertNil(store.failure)
        return store
    }

    private func exerciseTyping(_ source: String, append: Bool) async throws {
        let store = typingStore()
        let (host, window) = try documentHost(store)
        defer { window.close() }
        let text = try currentEditor(host)
        XCTAssertTrue(window.makeFirstResponder(text))
        text.insertText(String(source.dropLast()), replacementRange: NSRange(location: 0, length: 0))
        text.insertText("\n", replacementRange: text.selectedRange())
        try await assertAccepted(source, store: store)
        if append {
            let refreshed = try currentEditor(host)
            XCTAssertEqual((refreshed.delegate as? CodeText.Coordinator)?.parent.value, source)
            refreshed.insertText("q", replacementRange: refreshed.selectedRange())
            try await assertAccepted(source + "q", store: store)
        }
    }

    private func currentEditor(_ host: NSView) throws -> NSTextView {
        host.layoutSubtreeIfNeeded()
        host.displayIfNeeded()
        return try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
    }

    private func assertAccepted(_ proof: String, store: RegionStore, caret: Int? = nil) async throws {
        let deadline = Date(timeIntervalSinceNow: 1)
        while try (NativeTypingWire.document() != proof || NativeTypingWire.savedStore()?["caret"] as? Int != (caret ?? proof.utf8.count)) && Date() < deadline {
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertEqual(try NativeTypingWire.document(), proof, "Actual App dispatch must retain every edited character")
        XCTAssertEqual(try NativeTypingWire.savedStore()?["caret"] as? Int, caret ?? proof.utf8.count)
        XCTAssertTrue(store.diagnostics.isEmpty, store.diagnostics.joined(separator: "; "))
        await Task.yield()
    }

    // Mount the actual issued document region directly, isolating input from RootView viewport materialization.
    private func documentHost(_ store: RegionStore) throws -> (NSHostingView<some View>, NSWindow) {
        let document = try XCTUnwrap(store.cell("document"))
        let view = CellView(cell: document, store: store).environmentObject(store)
        let (host, window) = TestResources.hosted(view, width: 900, height: 800, keyboard: true)
        host.layoutSubtreeIfNeeded()
        host.displayIfNeeded()
        return (host, window)
    }

    func testActualImportedWordSeedStatementHasAReachableRightHandSide() throws {
        let (library, loaded) = try importedWordSeed()
        let selected = try selectTheorem("OAI.LowSpaceSubsetSum.WordSeedPowerExecution.run_work", library: library, previous: loaded)
        let patch = try XCTUnwrap(selected.regions["library"])
        let result = try XCTUnwrap(framed("proof-library-result", patch.node))
        try assertReachable(result, revision: patch.revision)
    }

    func testOpeningAnExampleRevealsTheProofInsteadOfTheImportedLibrary() throws {
        let (library, loaded) = try importedWordSeed()
        XCTAssertNotNil(framed("proof-library-inspection", try XCTUnwrap(loaded.regions["root"]).node))
        let examples = try activate("Examples", library: library, previous: loaded)
        let opened = try activate("Open conjunction example", library: library, previous: examples)
        let root = try XCTUnwrap(opened.regions["root"])
        XCTAssertNotNil(framed("example-context", root.node))
        XCTAssertNil(framed("proof-library-inspection", root.node), "Opening the example must reveal its proof instead of leaving the imported-library surface over it")
    }

    private func importedWordSeed() throws -> (Library, LayoutSnapshot) {
        let library = workbenchLibrary()
        let initial = try snapshot(library.start(context: #"{"platform":"macos","now":1,"offset":0,"home":"","arguments":[]}"#))
        let visible = try activate("Notation", library: library, previous: initial)
        let requested = try activate("Open proof library", library: library, previous: visible)
        let file = TestResources.apple.appendingPathComponent(".build/Osprey Workbench.app/Contents/Resources/proof-libraries/word-seed.ndjson")
        SliceStore.shared.write("proof:import", try String(contentsOf: file, encoding: .utf8))
        let reply = #"{"kind":"completed","ticket":"proof:opened","ok":true,"result":{"path":"word-seed.ndjson"},"now":3}"#
        let loaded = try snapshot(library.dispatch(state: requested.envelope.state, event: reply), previous: requested)
        return (library, loaded)
    }

    private func selectTheorem(_ name: String, library: Library, previous: LayoutSnapshot) throws -> LayoutSnapshot {
        let patch = try XCTUnwrap(previous.regions["library"])
        let ordinal = try XCTUnwrap(theoremPicker(patch.node))
        let event: [String: Any] = ["kind": "ui", "region": patch.key, "revision": patch.revision, "ordinal": ordinal, "value": name, "now": 4]
        let bytes = try JSONSerialization.data(withJSONObject: event)
        return try snapshot(library.dispatch(state: previous.envelope.state, event: String(decoding: bytes, as: UTF8.self)), previous: previous)
    }

    private func theoremPicker(_ node: Node) -> Ordinal? {
        if case let .picker(label, _, _, ordinal) = node, label == "Theorem" { return ordinal }
        return children(node).compactMap(theoremPicker).first
    }

    private func assertReachable(_ node: Node, revision: Int) throws {
        let view = NodeView(node: node, drawing: Drawing(region: "library", revision: revision, send: { _, _ in }))
            .frame(width: 430, height: 500, alignment: .topLeading)
        let (host, window) = TestResources.hosted(view, width: 430, height: 500)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first, "The actual checked statement must expose horizontal inspection instead of painting beyond its pane")
        try TestResources.assertInspectableEnd(scroll)
    }

    func testActualApplicationKeepsTheGuidedCaptionBesideItsReturnAction() throws {
        let library = workbenchLibrary()
        let initial = try snapshot(library.start(context: #"{"platform":"macos","now":1,"offset":0,"home":"","arguments":[]}"#))
        let examples = try activate("Examples", library: library, previous: initial)
        let opened = try activate("Open conjunction example", library: library, previous: examples)
        let root = try XCTUnwrap(opened.regions["root"])
        let context = try XCTUnwrap(framed("example-context", root.node))
        let view = NodeView(node: context, drawing: Drawing(region: "root", revision: root.revision, send: { _, _ in }))
            .environment(\.ospreyTheme, ThemeResolver(theme: opened.envelope.theme ?? initial.envelope.theme))
            .frame(width: 1488, height: 45).background(Color.white)
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
        let ink = (0..<45).reduce(0) { count, y in count + (195..<400).filter { isInk(bitmap, x: $0, y: y) }.count }
        XCTAssertGreaterThan(ink, 100, "The real Osprey context must keep Guided example beside Return, rather than at the window's far edge")
    }

    private func snapshot(_ text: String, previous: LayoutSnapshot? = nil) throws -> LayoutSnapshot {
        let envelope = try Envelope.decode(text)
        XCTAssertTrue(envelope.diagnostics.isEmpty, envelope.diagnostics.joined(separator: "; "))
        envelope.slices.forEach { SliceStore.shared.write($0.key, $0.text) }
        let retained = previous?.regions.filter { !envelope.removed.contains($0.key) } ?? [:]
        let patches = Dictionary(uniqueKeysWithValues: envelope.patches.map { ($0.key, $0) })
        return LayoutSnapshot(envelope: envelope, regions: retained.merging(patches) { _, next in next })
    }

    private func activate(_ label: String, library: Library, previous: LayoutSnapshot) throws -> LayoutSnapshot {
        let issued = try XCTUnwrap(previous.regions.values.first { button(label, $0.node) != nil })
        let ordinal = try XCTUnwrap(button(label, issued.node))
        let event: [String: Any] = ["kind": "ui", "region": issued.key, "revision": issued.revision, "ordinal": ordinal, "value": NSNull(), "now": 2]
        let bytes = try JSONSerialization.data(withJSONObject: event)
        let result = try library.dispatch(state: previous.envelope.state, event: String(decoding: bytes, as: UTF8.self))
        return try snapshot(result, previous: previous)
    }

    private func button(_ label: String, _ node: Node) -> Ordinal? {
        if case let .button(name, _, _, enabled, ordinal) = node, name == label, enabled { return ordinal }
        return children(node).compactMap { button(label, $0) }.first
    }

    private func framed(_ name: String, _ node: Node) -> Node? {
        if case let .framed(style, _) = node, style.name == name { return node }
        return children(node).compactMap { framed(name, $0) }.first
    }

    private func children(_ node: Node) -> [Node] {
        switch node {
        case let .stack(_, nodes), let .layer(nodes), let .section(_, _, nodes), let .disclosure(_, _, _, nodes): return nodes
        case let .framed(_, content), let .themed(_, content), let .dialog(_, content, _): return [content]
        case let .aside(pane, _, aside, _): return [pane, aside]
        default: return []
        }
    }

    private func isInk(_ bitmap: NSBitmapImageRep, x: Int, y: Int) -> Bool {
        guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
        return color.alphaComponent > 0.9 && max(color.redComponent, color.greenComponent, color.blueComponent) < 0.7
    }
}

private enum NativeTypingWire {
    static var latest = ""

    static func capture(_ pointer: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
        if let pointer { latest = String(cString: pointer) }
        return pointer
    }

    static func document() throws -> String? {
        try savedStore()?["text"] as? String
    }

    static func savedStore() throws -> [String: Any]? {
        let envelope = try Envelope.decode(latest)
        let session = try JSONSerialization.jsonObject(with: Data(envelope.state.utf8)) as? [String: Any]
        return session?["store"] as? [String: Any]
    }
}

@_silgen_name("osp_arc_live_objects") private func nativeLiveObjects() -> UInt
@_silgen_name("osp_arc_live_bytes") private func nativeLiveBytes() -> UInt

private enum NativeImportAccounting {
    static var reads = 0
    struct Sample { let objects: UInt; let bytes: UInt; let resident: UInt64 }

    static func sample() throws -> Sample {
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<integer_t>.size)
        let capacity = Int(count)
        let status = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: capacity) {
                task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
            }
        }
        guard status == KERN_SUCCESS else { throw NSError(domain: "NativeImportMemory", code: Int(status)) }
        return Sample(objects: nativeLiveObjects(), bytes: nativeLiveBytes(), resident: info.resident_size)
    }
}

@MainActor
private func workbenchLibrary(start: StartFunction = osprey_app_start, dispatch: DispatchFunction = osprey_app_dispatch) -> Library {
    Library(entry: LibraryEntry(main: osprey_main, start: start, dispatch: dispatch,
        ownershipVersion: osprey_host_abi_version, releaseString: osprey_release_string))
}

private func nativeTypingStart(_ context: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    NativeTypingWire.capture(osprey_app_start(context))
}

private func nativeTypingDispatch(_ saved: UnsafePointer<CChar>?, _ event: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    NativeTypingWire.capture(osprey_app_dispatch(saved, event))
}
