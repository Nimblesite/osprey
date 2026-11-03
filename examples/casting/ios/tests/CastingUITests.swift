import XCTest

// [CAST-MOBILE-DELIVERY] Drive the real native shell and resident Osprey library.
final class CastingUITests: XCTestCase {
    @MainActor
    private func drag(_ slider: XCUIElement, to fraction: CGFloat) {
        func coordinate(_ value: CGFloat) -> XCUICoordinate {
            slider.coordinate(withNormalizedOffset: CGVector(dx: min(0.999, max(0.001, value)), dy: 0.5))
        }
        coordinate(slider.normalizedSliderPosition).press(forDuration: 0.1, thenDragTo: coordinate(fraction), withVelocity: .slow, thenHoldForDuration: 0.3)
    }

    @MainActor
    func testNativeLaunchDocumentCancellationAndRecovery() throws {
        let app = XCUIApplication()
        app.launchEnvironment["CAST_TEST_RECEIVER"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_RECEIVER"], "Use the native test entry point to start the receiver fixture")
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"])
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
        open.tap()
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        app.buttons["Browse"].tap()
        app.buttons["Browse"].tap()
        let location = app.staticTexts["On My iPhone"]
        XCTAssertTrue(location.waitForExistence(timeout: 10), app.debugDescription)
        location.tap()
        let folder = app.staticTexts["Casting"]
        XCTAssertTrue(folder.waitForExistence(timeout: 10), app.debugDescription)
        folder.tap()
        let file = app.cells["Casting mobile test, mp3"]
        XCTAssertTrue(file.waitForExistence(timeout: 10), app.debugDescription)
        file.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.25)).tap()
        let dismissed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: cancel)
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 10), .completed, app.debugDescription)
        XCTAssertTrue(open.waitForExistence(timeout: 10))
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
        app.terminate()
        app.launchEnvironment["CAST_TEST_AUDIO"] = audio
        app.launchArguments = ["--capture-envelope"]
        app.launch()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 15))
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.sliders["Position"].exists, app.debugDescription)
        XCTAssertTrue(app.sliders["Volume"].exists, app.debugDescription)
        app.buttons["Pause"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5))
        app.buttons["Resume"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5))
        drag(app.sliders["Volume"], to: 0.5)
        drag(app.sliders["Position"], to: 0.25)
        app.buttons["Speed: 1×"].tap()
        XCTAssertTrue(app.buttons["1.5×"].waitForExistence(timeout: 5))
        app.buttons["1.5×"].tap()
        XCTAssertTrue(app.buttons["Speed: 1.5×"].waitForExistence(timeout: 5))
        let playing = XCTAttachment(screenshot: app.screenshot())
        playing.name = "casting-ios-playing"
        playing.lifetime = .keepAlways
        add(playing)
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15))
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
    }
}
