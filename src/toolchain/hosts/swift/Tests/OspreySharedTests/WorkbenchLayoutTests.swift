import AppKit
import SwiftUI
import XCTest

@testable import OspreyShared

// The real Workbench, linked into this suite by `make -C src/toolchain/targets/macos/tests
// workbench`, driven through the shared frame exactly as its window drives it: typed control
// events, capability completions and slices ([0053-NativeSharedViews]). Another application's
// archive cannot answer these, so `agent-headless` leaves this one class out.

@MainActor
final class WorkbenchLayoutTests: RenderingTestCase, DrivesWorkbench {
    override class func setUp() {
        super.setUp()
        XCTAssertEqual(setenv("OSPREY_ARC_DEBUG", "1", 1), 0)
    }

    func testActualApplicationImportDispatchAndCancellationMemoryPlateau() throws {
        let bench = Workbench()
        for revision in 0..<200 { try bench.progress(revision) }
        XCTAssertTrue(try bench.completedLessonShows(), "The actual app must consume the imported progress")
        let baseline = try ImportAccounting.sample()
        XCTAssertGreaterThan(baseline.objects, 0, "ARC counters must be armed")
        for revision in 0..<20_000 { try bench.progress(revision) }
        try ImportAccounting.assertPlateau(baseline)
        for _ in 0..<20_000 { bench.store.shared.send(Touch.completed("lessons:progress", failed: "cancelled")) }
        try ImportAccounting.assertPlateau(baseline)
        XCTAssertNil(bench.store.failure)
    }

    func testRealAppKeepsPastedProofWhenSelectionAndTextNotificationsArriveTogether() throws {
        let bench = try pasted("Theorem self. For all p, p → p.\nProof.\nAssume h : p.\nFrom h.\n")
        // A whole theorem leaves the editor for the checked paper: every pasted line is typeset there.
        let words = bench.words("document")
        for line in ["self", "For all propositions p.", "p → p", "Proof.", "Assume h : p.", "From h.", "Checked by the Osprey kernel"] {
            XCTAssertTrue(words.contains(line), "Actual App dispatch must retain every pasted line: \(line) in \(words)")
        }
        XCTAssertTrue(bench.store.diagnostics.isEmpty, bench.store.diagnostics.joined(separator: "; "))
    }

    func testRealAppKeepsSubsequentTypingInTheCurrentFormulaEditor() throws {
        try exerciseTyping("p → q\n", append: true)
    }

    func testRealAppKeepsTheCaretAfterMiddleUnicodeInsertion() throws {
        try middleUnicodeInsertion(waitForSelection: true)
    }

    func testRealAppKeepsUnicodeInsertionImmediatelyAfterMovingTheCaret() throws {
        try middleUnicodeInsertion(waitForSelection: false)
    }

    func testActualImportedWordSeedStatementHasAReachableRightHandSide() throws {
        let bench = try Workbench.importedWordSeed()
        try bench.choose("Theorem", "OAI.LowSpaceSubsetSum.WordSeedPowerExecution.run_work", in: "library")
        let result = try XCTUnwrap(bench.framed("proof-library-result"), "The chosen theorem's result must be drawn")
        try assertReachable(result, theme: bench.store.theme)
    }

    func testOpeningAnExampleRevealsTheProofInsteadOfTheImportedLibrary() throws {
        let bench = try Workbench.importedWordSeed()
        XCTAssertNotNil(bench.framed("proof-library-inspection", in: "root"))
        try bench.press("Examples")
        try bench.press("Open conjunction example")
        XCTAssertNotNil(bench.framed("example-context", in: "root"))
        XCTAssertNil(bench.framed("proof-library-inspection", in: "root"), "Opening the example must reveal its proof instead of leaving the imported-library surface over it")
    }

    func testActualApplicationKeepsTheGuidedCaptionBesideItsReturnAction() throws {
        let bench = Workbench()
        try bench.press("Examples")
        try bench.press("Open conjunction example")
        let context = try XCTUnwrap(bench.framed("example-context", in: "root"))
        XCTAssertGreaterThan(try captionInk(context, theme: bench.store.theme, columns: 195..<400), 100,
                             "The real Osprey context must keep Guided example beside Return, rather than at the window's far edge")
    }
}

// MARK: typing into the real document editor

/// A suite that drives the real Workbench's document editor and draws its frames.
@MainActor
protocol DrivesWorkbench {}

extension DrivesWorkbench {
    /// The Workbench after `source` was pasted into its empty editor with one newline typed after it.
    func pasted(_ source: String) throws -> Workbench {
        let bench = Workbench()
        let (host, window) = try bench.document()
        defer { window.close() }
        let text = try bench.editor(host)
        XCTAssertTrue(window.makeFirstResponder(text))
        text.insertText(String(source.dropLast()), replacementRange: NSRange(location: 0, length: 0))
        text.insertText("\n", replacementRange: text.selectedRange())
        bench.settle { bench.find { $0.text("kind") == "editor" } == nil }
        return bench
    }

    func exerciseTyping(_ source: String, append: Bool) throws {
        let bench = Workbench()
        let (host, window) = try bench.document()
        defer { window.close() }
        let text = try bench.editor(host)
        XCTAssertTrue(window.makeFirstResponder(text))
        text.insertText(String(source.dropLast()), replacementRange: NSRange(location: 0, length: 0))
        text.insertText("\n", replacementRange: text.selectedRange())
        try bench.assertAccepted(source)
        if append {
            let refreshed = try bench.editor(host)
            XCTAssertEqual((refreshed.delegate as? CodeText.Coordinator)?.parent.value, source)
            refreshed.insertText("q", replacementRange: refreshed.selectedRange())
            try bench.assertAccepted(source + "q")
        }
    }

    func middleUnicodeInsertion(waitForSelection: Bool) throws {
        let bench = Workbench()
        let (host, window) = try bench.document()
        defer { window.close() }
        try bench.editor(host).insertText("𝑝q\n", replacementRange: NSRange(location: 0, length: 0))
        try bench.assertAccepted("𝑝q\n")
        try bench.editor(host).setSelectedRange(NSRange(location: 2, length: 0))
        if waitForSelection { try bench.assertAccepted("𝑝q\n", caret: 4) }
        let positioned = try bench.editor(host)
        positioned.insertText("→", replacementRange: positioned.selectedRange())
        try bench.assertAccepted("𝑝→q\n", caret: 7)
    }

    func assertReachable(_ node: Tree, theme: Theme?) throws {
        let view = NodeView(node: node, drawing: Fixtures.drawing("library"))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 430, height: 500, alignment: .topLeading)
        let (host, window) = TestResources.hosted(view, width: 430, height: 500)
        defer { window.close() }
        let scroll = try XCTUnwrap(TestResources.scrollViews(host).first { ($0.documentView?.bounds.width ?? 0) > $0.contentView.bounds.width },
                                   "The actual checked statement must expose horizontal inspection instead of painting beyond its pane")
        try TestResources.assertInspectableEnd(scroll)
    }

    /// Ink in `columns` of the context strip drawn at the window's full width.
    func captionInk(_ context: Tree, theme: Theme?, columns: Range<Int>) throws -> Int {
        let view = NodeView(node: context, drawing: Fixtures.drawing("root"))
            .environment(\.ospreyTheme, ThemeResolver(theme: theme)).frame(width: 1488, height: 45).background(Color.white)
        let bitmap = try TestResources.rendered(view)
        return (0..<45).reduce(0) { count, y in
            count + columns.filter { x in
                guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return colour.alphaComponent > 0.9 && max(colour.redComponent, colour.greenComponent, colour.blueComponent) < 0.7
            }.count
        }
    }
}

/// One running Workbench: its regions, and the controls a person would press.
@MainActor
final class Workbench {
    let store: RegionStore
    private let folder = FileManager.default.temporaryDirectory.appendingPathComponent("workbench-layout-\(UUID().uuidString)")

    /// The Workbench from its own folder, as its launcher names it, with `chosen` answering the file panel.
    init(chosen: String = "") {
        Bundled.appFolder = Repo.src.appendingPathComponent("osprey/workbench", isDirectory: true)
        let panel = DocumentAdapter(panel: { _ in chosen })
        store = RegionStore(shared: SharedStore(adapters: Adapters.standard(store: StoreAdapter(root: folder)) + [panel]))
        store.start(dark: false)
        _ = settle { (self.store.cell("catalogue").map { Workbench.labels($0.node) } ?? []).contains("Open") }
    }

    deinit { try? FileManager.default.removeItem(at: folder) }

    /// The Workbench after Notation opened the pinned word-seed proof library.
    static func importedWordSeed() throws -> Workbench {
        let library = Repo.src.appendingPathComponent("osprey/examples/proof/openai/word-seed.ndjson").path
        let bench = Workbench(chosen: library)
        try bench.press("Notation")
        try bench.press("Open proof library")
        XCTAssertTrue(bench.settle { bench.framed("proof-library-inspection", in: "root") != nil && bench.store.cell("library") != nil },
                      "The chosen library must load: \(bench.store.diagnostics)")
        return bench
    }

    /// Presses the enabled button labelled `label`, wherever it is drawn.
    func press(_ label: String) throws {
        let (key, node) = try XCTUnwrap(find { $0.text("kind") == "button" && $0.text("label") == label && $0.flag("enabled") }, "No enabled \(label)")
        store.send(region: key, revision: 0, ordinal: node.count("event"), value: .null)
        _ = settle { true }
    }

    /// Chooses `value` in the picker titled `label` in `region`.
    func choose(_ label: String, _ value: String, in region: String) throws {
        let picker = try XCTUnwrap(store.cell(region).flatMap { Workbench.first(in: $0.node) { $0.text("kind") == "picker" && $0.text("label") == label } })
        store.send(region: region, revision: 0, ordinal: picker.count("event"), value: .string(value))
        XCTAssertTrue(settle { self.framed("proof-library-result") != nil }, "Choosing \(value) must draw its result")
    }

    /// The frame named `name` in `region`, or in any region.
    func framed(_ name: String, in region: String? = nil) -> Tree? {
        let named: (Tree) -> Bool = { $0.text("kind") == "framed" && ($0["style"] ?? .null).text("name") == name }
        if let region { return store.cell(region).flatMap { Workbench.first(in: $0.node, where: named) } }
        return find(named)?.1
    }

    func find(_ test: (Tree) -> Bool) -> (String, Tree)? {
        store.cells.keys.sorted().compactMap { key in self.store.cell(key).flatMap { Workbench.first(in: $0.node, where: test) }.map { (key, $0) } }.first
    }

    /// One lesson-progress import: the store read's text in its slice, then its completion.
    func progress(_ revision: Int) throws {
        try autoreleasepool {
            let prefix = "{\"completed\":[\"what-is-a-proof/write-identity\",\"what-is-a-proof/prove-identity\",\"what-is-a-proof/judge-identity\"],\"padding\":\"\(revision) é→"
            SliceStore.shared.write("lessons:progress", prefix + String(repeating: "x", count: 8192 - prefix.utf8.count - 2) + "\"}")
            let before = store.diagnostics.count
            store.shared.send(Touch.completed("lessons:progress", result: ""))
            XCTAssertEqual(store.diagnostics.count, before, store.diagnostics.joined(separator: "; "))
        }
    }

    func completedLessonShows() throws -> Bool {
        Workbench.labels(try XCTUnwrap(store.cell("catalogue")).node, kind: "badge").contains("Complete")
    }

    /// The document region alone in a keyboard window, as its pane draws it.
    func document() throws -> (NSHostingView<some View>, NSWindow) {
        let cell = try XCTUnwrap(store.cell("document"))
        let view = CellView(cell: cell, store: store).environmentObject(store).environment(\.ospreyTheme, ThemeResolver(theme: store.theme))
        return TestResources.hosted(view, width: 900, height: 800, keyboard: true)
    }

    func editor(_ host: NSView) throws -> NSTextView {
        host.layoutSubtreeIfNeeded()
        host.displayIfNeeded()
        return try XCTUnwrap(TestResources.scrollViews(host).compactMap { $0.documentView as? NSTextView }.first)
    }

    /// The application's own editor holds `proof` with its caret at `caret` UTF-8 bytes.
    func assertAccepted(_ proof: String, caret: Int? = nil) throws {
        let expected = caret ?? proof.utf8.count
        let held = { self.find { $0.text("kind") == "editor" }?.1 }
        _ = settle { held()?.text("value") == proof && held()?.count("cursor") == expected }
        XCTAssertEqual(held()?.text("value"), proof, "Actual App dispatch must retain every edited character")
        XCTAssertEqual(held()?.count("cursor"), expected)
        XCTAssertTrue(store.diagnostics.isEmpty, store.diagnostics.joined(separator: "; "))
    }

    /// Runs the main loop until `done` holds, for at most three seconds.
    @discardableResult
    func settle(_ done: () -> Bool) -> Bool {
        let deadline = Date(timeIntervalSinceNow: 3)
        repeat { TestResources.settle(0.02) } while !done() && Date() < deadline
        return done()
    }

    /// Every word `region` draws, joined as it reads.
    func words(_ region: String) -> String {
        func spoken(_ node: Tree) -> [String] {
            [node.text("text"), node.text("value"), node.text("label"), node.text("spoken")].filter { !$0.isEmpty } + Workbench.children(node).flatMap(spoken)
        }
        return store.cell(region).map { spoken($0.node).joined() } ?? ""
    }

    static func labels(_ node: Tree, kind: String = "button") -> [String] {
        (node.text("kind") == kind ? [node.text("label")] : []) + children(node).flatMap { labels($0, kind: kind) }
    }

    static func first(in node: Tree, where test: (Tree) -> Bool) -> Tree? {
        if test(node) { return node }
        for child in children(node) { if let found = first(in: child, where: test) { return found } }
        return nil
    }

    static func children(_ node: Tree) -> [Tree] {
        guard case let .object(members) = node else { return [] }
        return members.keys.sorted().compactMap { members[$0] }.flatMap { value -> [Tree] in
            switch value {
            case let .array(items): return items
            case .object: return [value]
            default: return []
            }
        }
    }
}

@_silgen_name("osp_arc_live_objects") private func liveObjects() -> UInt
@_silgen_name("osp_arc_live_bytes") private func liveBytes() -> UInt

/// The application's live ARC objects and bytes, and the process's resident memory.
enum ImportAccounting {
    struct Sample { let objects: UInt; let bytes: UInt; let resident: UInt64 }

    static func sample() throws -> Sample {
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<integer_t>.size)
        let capacity = Int(count)
        let status = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: capacity) { task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count) }
        }
        guard status == KERN_SUCCESS else { throw NSError(domain: "NativeImportMemory", code: Int(status)) }
        return Sample(objects: liveObjects(), bytes: liveBytes(), resident: info.resident_size)
    }

    static func assertPlateau(_ baseline: Sample) throws {
        let current = try sample()
        XCTAssertEqual(current.objects, baseline.objects)
        XCTAssertEqual(current.bytes, baseline.bytes)
        XCTAssertLessThanOrEqual(current.resident, baseline.resident + 16 * 1024 * 1024)
        XCTAssertEqual(SliceStore.shared.borrowedBytes, 0)
    }
}
