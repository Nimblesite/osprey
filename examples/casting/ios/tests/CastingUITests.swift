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
        defer { app.terminate() }
        app.launchEnvironment["CAST_TEST_RECEIVER"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_RECEIVER"], "Use the native test entry point to start the receiver fixture")
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"])
        app.launch()
        let open = app.buttons["Open file…"]
        XCTAssertTrue(open.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].exists)
        XCTAssertTrue(app.buttons["Receiver: Test receiver"].waitForExistence(timeout: 10))
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
        XCTAssertTrue(app.buttons["Receiver: Test receiver"].waitForExistence(timeout: 10))
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
        app.launchEnvironment.removeValue(forKey: "CAST_TEST_AUDIO")
        for skin in ["midnight", "poolside"] {
            app.terminate()
            app.launchArguments = ["--skin", skin]
            app.launch()
            XCTAssertTrue(app.buttons["Open file…"].waitForExistence(timeout: 15), app.debugDescription)
            let title = app.staticTexts["Choose an audio file"]
            XCTAssertTrue(title.waitForExistence(timeout: 10), app.debugDescription)
            let screen = XCTAttachment(screenshot: app.screenshot())
            screen.name = "casting-ios-\(skin)-ready"; screen.lifetime = .keepAlways; add(screen)
            let text = XCTAttachment(screenshot: title.screenshot())
            text.name = "casting-ios-\(skin)-title"; text.lifetime = .keepAlways; add(text)
        }
    }

    @MainActor
    func testThemedPlaybackPreservesTheCastSession() throws {
        let app = XCUIApplication()
        defer { app.terminate() }
        app.launchEnvironment["CAST_TEST_RECEIVER"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_RECEIVER"])
        app.launchEnvironment["CAST_TEST_AUDIO"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"])
        app.launch()
        XCTAssertTrue(app.buttons["Appearance"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Appearance"].tap()
        app.buttons["Midnight Mint"].tap()
        XCTAssertTrue(app.buttons["Skin"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Pause"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5))
        app.buttons["Forward 10 seconds"].tap()
        app.buttons["Volume 100%"].tap()
        app.buttons["50%"].tap()
        app.buttons["Speed"].tap()
        app.buttons["1.5×"].tap()
        app.buttons["Skin"].tap()
        app.buttons["Poolside Pop"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(app.staticTexts["Casting mobile test"].exists)
        app.buttons["Back 10 seconds"].tap()
        app.buttons["Volume 50%"].tap()
        app.buttons["25%"].tap()
        app.buttons["Speed"].tap()
        app.buttons["0.75×"].tap()
        app.buttons["Resume"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5))
        app.buttons["Appearance"].tap()
        app.buttons["Sherbet Signal"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5))
        app.buttons["Appearance"].tap()
        app.buttons["Poolside Pop"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5))
        app.buttons["Speed"].tap()
        app.buttons["1.25×"].tap()
        app.buttons["Pause"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5))
        let paused = XCTAttachment(screenshot: app.screenshot())
        paused.name = "casting-ios-poolside-paused"; paused.lifetime = .keepAlways; add(paused)
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15))
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
    }

}
