import AppKit
import XCTest

@testable import OspreyShared

// One more use case beside each WorkbenchLayoutTests case, on the same real Workbench archive
// ([0053-NativeSharedViews]); `agent-headless` leaves it out with its sibling.

@MainActor
final class WorkbenchLayoutExtraTests: RenderingTestCase, DrivesWorkbench {
    override class func setUp() {
        super.setUp()
        XCTAssertEqual(setenv("OSPREY_ARC_DEBUG", "1", 1), 0)
    }

    func testActualApplicationImportDispatchAndCancellationMemoryPlateau_extraFailedReadsKeepTheCompletedLesson() throws {
        let bench = Workbench()
        try bench.progress(0)
        XCTAssertTrue(try bench.completedLessonShows())
        try bench.progress(1)
        let baseline = try ImportAccounting.sample()
        for (index, reason) in ["denied", "missing", "é→ broken", ""].enumerated() {
            for _ in 0..<2_000 { bench.store.shared.send(Touch.completed("lessons:progress", failed: reason)) }
            try ImportAccounting.assertPlateau(baseline)
            XCTAssertTrue(try bench.completedLessonShows(), "A failed read \(index) must not forget the imported progress")
        }
        XCTAssertNil(bench.store.failure)
    }

    func testRealAppKeepsSubsequentTypingInTheCurrentFormulaEditor_extraConjunctionKeepsTheNextKey() throws {
        try exerciseTyping("p ∧ q\n", append: true)
    }

    func testRealAppKeepsTheCaretAfterMiddleUnicodeInsertion_extraCaretAtTheStart() throws {
        let bench = Workbench()
        let (host, window) = try bench.document()
        defer { window.close() }
        try bench.editor(host).insertText("𝑝q\n", replacementRange: NSRange(location: 0, length: 0))
        try bench.assertAccepted("𝑝q\n")
        try bench.editor(host).setSelectedRange(NSRange(location: 0, length: 0))
        try bench.assertAccepted("𝑝q\n", caret: 0)
        let positioned = try bench.editor(host)
        positioned.insertText("¬", replacementRange: positioned.selectedRange())
        try bench.assertAccepted("¬𝑝q\n", caret: 2)
    }

    func testRealAppKeepsUnicodeInsertionImmediatelyAfterMovingTheCaret_extraJustBeforeTheNewline() throws {
        let bench = Workbench()
        let (host, window) = try bench.document()
        defer { window.close() }
        try bench.editor(host).insertText("𝑝q\n", replacementRange: NSRange(location: 0, length: 0))
        try bench.assertAccepted("𝑝q\n")
        let positioned = try bench.editor(host)
        positioned.setSelectedRange(NSRange(location: 3, length: 0))
        positioned.insertText("∨", replacementRange: positioned.selectedRange())
        try bench.assertAccepted("𝑝q∨\n", caret: 8)
    }

    func testActualImportedWordSeedStatementHasAReachableRightHandSide_extraEveryChosenTheoremIsSelected() throws {
        let bench = try Workbench.importedWordSeed()
        let picker = { try XCTUnwrap(bench.store.cell("library").flatMap { Workbench.first(in: $0.node) { $0.text("kind") == "picker" && $0.text("label") == "Theorem" } }) }
        let choices = try picker().list("choices").map { $0.text("key") }
        XCTAssertGreaterThan(choices.count, 1, "The word-seed library must offer more than one theorem")
        for choice in choices.suffix(3).reversed() {
            try bench.choose("Theorem", choice, in: "library")
            XCTAssertTrue(bench.settle { (try? picker().text("selected")) == choice }, "Choosing \(choice) must select it")
            XCTAssertNotNil(bench.framed("proof-library-result"))
        }
        XCTAssertTrue(bench.store.diagnostics.isEmpty, bench.store.diagnostics.joined(separator: "; "))
    }

    func testLessonsAndBackKeepTheImportedLibrary_extraNoEnglishProofEntryIsOffered() throws {
        let bench = Workbench()
        let labels = bench.store.cells.keys.flatMap { key in bench.store.cell(key).map { Workbench.labels($0.node) } ?? [] }
        for label in ["Open conjunction example", "Open workspace", "Save", "Source", "Read", "Open in the workbench", "Show solution"] {
            XCTAssertFalse(labels.contains(label), "The workbench must offer no English proof control: \(label) in \(labels)")
        }
        XCTAssertNil(bench.store.cell("document"), "No English proof document region may exist")
        XCTAssertNotNil(bench.framed("proof-library-inspection", in: "root"))
    }
}
