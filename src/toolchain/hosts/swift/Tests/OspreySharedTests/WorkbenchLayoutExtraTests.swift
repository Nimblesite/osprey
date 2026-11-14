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

    func testRealAppKeepsPastedProofWhenSelectionAndTextNotificationsArriveTogether_extraRenamedTheoremKeepsItsOwnName() throws {
        let bench = try pasted("Theorem mine. For all p, p → p.\nProof.\nAssume h : p.\nFrom h.\n")
        let words = bench.words("document")
        for line in ["mine", "For all propositions p.", "Assume h : p.", "From h.", "Checked by the Osprey kernel"] {
            XCTAssertTrue(words.contains(line), "The pasted theorem must be typeset under its own name: \(line) in \(words)")
        }
        XCTAssertFalse(words.contains("self"), "No other theorem's name may be drawn: \(words)")
        XCTAssertTrue(bench.store.diagnostics.isEmpty, bench.store.diagnostics.joined(separator: "; "))
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

    func testOpeningAnExampleRevealsTheProofInsteadOfTheImportedLibrary_extraReturningLeavesTheExample() throws {
        let bench = try Workbench.importedWordSeed()
        try bench.press("Examples")
        try bench.press("Open conjunction example")
        XCTAssertNotNil(bench.framed("example-context", in: "root"))
        try bench.press("Return to working")
        XCTAssertTrue(bench.settle { bench.framed("example-context", in: "root") == nil }, "Return must leave the guided example")
        XCTAssertNil(bench.framed("proof-library-inspection", in: "root"), "The library stays put away after the example")
    }

    func testActualApplicationKeepsTheGuidedCaptionBesideItsReturnAction_extraNothingDrawsAtTheFarEdge() throws {
        let bench = Workbench()
        try bench.press("Examples")
        try bench.press("Open conjunction example")
        let context = try XCTUnwrap(bench.framed("example-context", in: "root"))
        XCTAssertEqual(try captionInk(context, theme: bench.store.theme, columns: 1000..<1456), 0,
                       "The strip's right side must stay empty: its caption sits beside Return")
    }
}
