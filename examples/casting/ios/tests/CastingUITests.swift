import XCTest

// [CAST-MOBILE-DELIVERY] Drive the real native shell and resident Osprey library.
final class CastingUITests: XCTestCase {
    @MainActor
    func testNativeLaunchDocumentCancellationAndRecovery() throws {
        let app = XCUIApplication()
        app.launch()
        let open = app.buttons["Open file…"]
        XCTAssertTrue(open.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].exists)
        let initial = XCTAttachment(screenshot: app.screenshot())
        initial.name = "casting-ios-ready"
        initial.lifetime = .keepAlways
        add(initial)
        open.tap()
        let cancel = app.buttons["Cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 10), app.debugDescription)
        cancel.tap()
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Play"].exists)
        open.tap()
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        cancel.tap()
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        app.terminate()
        app.launch()
        XCTAssertTrue(app.buttons["Open file…"].waitForExistence(timeout: 15))
        let restarted = XCTAttachment(screenshot: app.screenshot())
        restarted.name = "casting-ios-restarted"
        restarted.lifetime = .keepAlways
        add(restarted)
    }
}
