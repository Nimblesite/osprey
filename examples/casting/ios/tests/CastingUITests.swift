import XCTest

// [CAST-MOBILE-DELIVERY] Drive the real native shell and resident Osprey library.
final class CastingUITests: XCTestCase {
    @MainActor
    func testNativeAppearanceSurvivesProcessRestart() {
        // [CAST-MOBILE-SKIN-PERSISTENCE] A new process must read the saved choice.
        let app = XCUIApplication()
        defer { app.terminate() }
        app.launchArguments = ["--skin", "midnight"]
        app.launch()
        for (trigger, label) in [("Skin", "Poolside Pop"), ("Skin", "Sherbet Signal"), ("Appearance", "Midnight Mint")] {
            XCTAssertTrue(app.buttons[trigger].waitForExistence(timeout: 15), app.debugDescription)
            app.buttons[trigger].tap()
            app.buttons[label].tap()
            if label != "Sherbet Signal" { XCTAssertEqual(app.buttons["Skin"].value as? String, label) }
            app.terminate()
            app.launchArguments = []
            app.launch()
            XCTAssertTrue(app.buttons["Appearance"].waitForExistence(timeout: 15), app.debugDescription)
            if label == "Sherbet Signal" { XCTAssertFalse(app.buttons["Skin"].exists, app.debugDescription) }
            else { XCTAssertEqual(app.buttons["Skin"].value as? String, label) }
        }
    }

    @MainActor
    func testNativeBonjourDiscoveryAndRescan() throws {
        let app = XCUIApplication()
        defer { app.terminate() }
        app.launchArguments = ["--skin", "midnight"]
        app.launchEnvironment["CAST_TEST_AUDIO"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"])
        let host = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_NATIVE_HOST"])
        let port = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_NATIVE_PORT"])
        let receiver = "Osprey native test receiver (\(host):\(port))"
        app.launch()
        XCTAssertTrue(app.buttons["Receiver"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Receiver"].tap()
        XCTAssertTrue(app.buttons[receiver].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.buttons["Osprey native test receiver (\(host):1)"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["Test receiver"].exists, "Native discovery must not use the injected transcript")
        app.buttons[receiver].tap()
        XCTAssertEqual(app.buttons["Receiver"].value as? String, receiver, "The selected receiver must be announced")
        app.buttons["Search again"].tap()
        XCTAssertTrue(app.buttons["Receiver"].waitForExistence(timeout: 10))
        app.buttons["Receiver"].tap()
        XCTAssertTrue(app.buttons[receiver].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(app.buttons["Receiver"].value as? String, receiver, "Rescanning must retain and announce the selected endpoint")
        app.buttons[receiver].tap()
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
    }

    @MainActor
    private func touchRegions(_ elements: [XCUIElement]) {
        for element in elements {
            // AX snapshots can represent 44 points as 43.99999999999999.
            XCTAssertGreaterThanOrEqual(element.frame.width + 1e-9, 44, "\(element.label): \(element.frame)")
            XCTAssertGreaterThanOrEqual(element.frame.height + 1e-9, 44, "\(element.label): \(element.frame)")
        }
    }

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
        app.launchArguments = ["--skin", "sherbet"]
        defer { app.terminate() }
        app.launchEnvironment["CAST_TEST_RECEIVER"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_RECEIVER"], "Use the native test entry point to start the receiver fixture")
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"])
        app.launch()
        let open = app.buttons["Open file…"]
        XCTAssertTrue(open.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].exists)
        XCTAssertTrue(app.buttons["Receiver: Test receiver"].waitForExistence(timeout: 10))
        touchRegions([app.buttons["Play"], open, app.buttons["Receiver: Test receiver"], app.buttons["Appearance"]])
        let initial = XCTAttachment(screenshot: app.screenshot())
        initial.name = "casting-ios-ready"
        initial.lifetime = .keepAlways
        add(initial)
        open.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.85)).tap()
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
        app.launchArguments = ["--capture-envelope", "--skin", "sherbet"]
        app.launch()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 15))
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        // [CAST-MOBILE-DELIVERY] Returning from Home keeps the same live cast.
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5) || app.state == .runningBackgroundSuspended)
        app.activate()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.sliders["Position"].exists, app.debugDescription)
        XCTAssertTrue(app.sliders["Volume"].exists, app.debugDescription)
        touchRegions([app.buttons["Stop"], app.sliders["Position"], app.sliders["Volume"], app.buttons["Speed: 1×"]])
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
            touchRegions([app.buttons["Play"], app.buttons["Open file…"], app.buttons["Receiver"], app.buttons["Skin"]])
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
        app.launchArguments = ["--skin", "sherbet"]
        defer { app.terminate() }
        app.launchEnvironment["CAST_TEST_RECEIVER"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_RECEIVER"])
        app.launchEnvironment["CAST_TEST_AUDIO"] = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"])
        app.launch()
        XCTAssertTrue(app.buttons["Appearance"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Appearance"].tap()
        app.buttons["Midnight Mint"].tap()
        XCTAssertTrue(app.buttons["Skin"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(app.buttons["Skin"].value as? String, "Midnight Mint")
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        touchRegions([app.buttons["Pause"], app.buttons["Stop"], app.buttons["Speed"], app.buttons["Volume 100%"], app.buttons["Forward 10 seconds"]])
        XCTAssertEqual(app.buttons["Speed"].value as? String, "1×")
        app.buttons["Pause"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5))
        app.buttons["Forward 10 seconds"].tap()
        app.buttons["Volume 100%"].tap()
        app.buttons["50%"].tap()
        XCTAssertEqual(app.buttons["Volume 50%"].value as? String, "50%")
        app.buttons["Speed"].tap()
        app.buttons["1.5×"].tap()
        XCTAssertEqual(app.buttons["Speed"].value as? String, "1.5×")
        app.buttons["Skin"].tap()
        app.buttons["Poolside Pop"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(app.buttons["Skin"].value as? String, "Poolside Pop")
        XCTAssertTrue(app.staticTexts["Casting mobile test"].exists)
        app.buttons["Back 10 seconds"].tap()
        app.buttons["Volume 50%"].tap()
        app.buttons["25%"].tap()
        XCTAssertEqual(app.buttons["Volume 25%"].value as? String, "25%")
        app.buttons["Speed"].tap()
        app.buttons["0.75×"].tap()
        XCTAssertEqual(app.buttons["Speed"].value as? String, "0.75×")
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
        app.buttons["Receiver"].tap()
        app.buttons["Unavailable receiver"].tap()
        app.buttons["Play"].tap()
        let failure = app.staticTexts["127.0.0.1:1: connection refused or timed out"]
        XCTAssertTrue(failure.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].exists)
        app.buttons["Skin"].tap()
        app.buttons["Midnight Mint"].tap()
        XCTAssertTrue(failure.waitForExistence(timeout: 5), app.debugDescription)
        for (name, element) in [("loaded-title", app.staticTexts["Casting mobile test"]), ("search", app.buttons["Search again"]), ("receiver", app.buttons["Receiver"])] {
            let image = XCTAttachment(screenshot: element.screenshot())
            image.name = "casting-ios-midnight-\(name)"; image.lifetime = .keepAlways; add(image)
        }
        let refused = XCTAttachment(screenshot: app.screenshot())
        refused.name = "casting-ios-connection-refused"; refused.lifetime = .keepAlways; add(refused)
        app.buttons["Receiver"].tap()
        app.buttons["Test receiver"].tap()
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
    }

}
