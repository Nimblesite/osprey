import XCTest

// [CAST-MOBILE-DELIVERY] [CAST-MOBILE-GPUI] Drive the installed GPUI
// Casting.app that build.sh packages from the shared Rust host and native
// Swift cast services. The runner has no app target, so it attaches by
// bundle identifier like a third-party XCUITest would.
final class CastingUITests: XCTestCase {
    private func castingApp() -> XCUIApplication {
        XCUIApplication(bundleIdentifier: "org.ospreylang.casting")
    }

    @MainActor
    private func appearancePopup(_ app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Appearance: ")).firstMatch
    }

    @MainActor
    func testSharedSceneExposesAccessibleControlsAndOpensTheNativePicker() {
        // [UI-MACHINE-A11Y] Shared scene semantics must reach the real OS tree.
        let app = castingApp()
        defer { app.terminate() }
        app.launchArguments = ["--skin", "sherbet"]
        app.launch()
        let open = app.buttons["Open file…"]
        XCTAssertTrue(open.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.staticTexts["Choose an audio file"].exists, app.debugDescription)
        XCTAssertTrue(open.isHittable, app.debugDescription)
        let frame = app.windows.firstMatch.frame
        XCTAssertGreaterThan(open.frame.minY, frame.minY + frame.height * 0.65)
        XCTAssertLessThanOrEqual(open.frame.maxY, frame.maxY)
        open.tap()
        XCTAssertTrue(app.navigationBars.firstMatch.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(open.isHittable, "The native document picker must cover the player")
    }

    @MainActor
    func testSharedAppearanceMenuExposesSelectionAndSwitchesSkins() {
        // [CAST-MOBILE-SKINS] The same machine scene and choices as desktop.
        let app = castingApp()
        defer { app.terminate() }
        app.launchArguments = ["--skin", "sherbet"]
        app.launch()
        let appearance = appearancePopup(app)
        XCTAssertTrue(appearance.waitForExistence(timeout: 15), app.debugDescription)
        appearance.tap()
        attach(app, "casting-ios-appearance-menu")
        for label in ["Sherbet Signal", "Midnight Mint", "Poolside Pop"] {
            let choice = labeled(app, label)
            XCTAssertTrue(choice.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertTrue(choice.isHittable, app.debugDescription)
            XCTAssertGreaterThanOrEqual(choice.frame.width, 44)
            XCTAssertGreaterThanOrEqual(choice.frame.height, 44)
            XCTAssertEqual(choice.isSelected, label == "Sherbet Signal", app.debugDescription)
        }
        assertSelected(app, "Sherbet Signal", "The appearance menu must announce its current selection")
        for label in ["Midnight Mint", "Poolside Pop", "Sherbet Signal"] {
            labeled(app, label).tap()
            if label != "Sherbet Signal" {
                assertSelected(app, label, "Changing appearance must update the shared selected binding")
            }
        }
        XCTAssertTrue(appearance.waitForExistence(timeout: 5), app.debugDescription)
        attach(app, "casting-ios-appearance-closed")
    }

    // The shared scene names a picker's choice on the control itself. A canvas announces its
    // choices as menu items, which iOS exposes without a button trait, so resolve a choice by
    // the exact label the scene announces rather than by one element type.
    @MainActor
    private func labeled(_ app: XCUIApplication, _ label: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label)).firstMatch
    }

    // [CAST-MOBILE-SKIN-PERSISTENCE] A chosen skin is announced as the selected choice.
    @MainActor
    private func assertSelected(_ app: XCUIApplication, _ label: String, _ message: String,
                                file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(labeled(app, label).waitForExistence(timeout: 5), message, file: file, line: line)
        XCTAssertTrue(labeled(app, label).isSelected, "\(message): \(app.debugDescription)", file: file, line: line)
    }

    // [CAST-MOBILE-RECEIVER-IDENTITY] Equal display names are chosen by endpoint.
    private func nativeReceivers() throws -> (available: String, unavailable: String, host: String) {
        let host = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_NATIVE_HOST"],
                                 "Use the native test entry point to publish the receiver fixture")
        let port = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_NATIVE_PORT"])
        return ("Osprey native test receiver (\(host):\(port))",
                "Osprey native test receiver (\(host):1)", host)
    }

    // The Sherbet canvas announces the chosen endpoint in its popup's label.
    @MainActor
    private func receiverPopup(_ app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@ AND label != %@",
                                         "Receiver: ", "Receiver: none")).firstMatch
    }

    @MainActor
    private func selectReceiver(_ app: XCUIApplication, _ label: String) {
        let popup = receiverPopup(app)
        XCTAssertTrue(popup.waitForExistence(timeout: 10), app.debugDescription)
        popup.tap()
        let choice = labeled(app, label)
        XCTAssertTrue(choice.waitForExistence(timeout: 10), app.debugDescription)
        choice.tap()
        XCTAssertTrue(app.buttons["Receiver: \(label)"].waitForExistence(timeout: 10),
                      "The selected receiver must be announced: \(app.debugDescription)")
    }

    @MainActor
    func testNativeAppearanceSurvivesProcessRestart() {
        // [CAST-MOBILE-SKIN-PERSISTENCE] A new process must read the saved choice.
        let app = castingApp()
        defer { app.terminate() }
        app.launchArguments = ["--skin", "midnight"]
        app.launch()
        for label in ["Poolside Pop", "Sherbet Signal", "Midnight Mint"] {
            if appearancePopup(app).exists { appearancePopup(app).tap() }
            let choice = labeled(app, label)
            XCTAssertTrue(choice.waitForExistence(timeout: 15), app.debugDescription)
            choice.tap()
            if label != "Sherbet Signal" {
                assertSelected(app, label, "The shared GPUI picker must announce its selected skin")
            }
            app.terminate()
            app.launchArguments = []
            app.launch()
            if label == "Sherbet Signal" {
                XCTAssertTrue(appearancePopup(app).waitForExistence(timeout: 15), app.debugDescription)
                XCTAssertFalse(app.staticTexts["Skin"].exists, app.debugDescription)
                appearancePopup(app).tap()
                assertSelected(app, label, "The canvas appearance menu must restore its selected skin")
                labeled(app, label).tap()
            } else {
                XCTAssertTrue(labeled(app, label).waitForExistence(timeout: 15), app.debugDescription)
                assertSelected(app, label, "A new process must restore the selected skin")
            }
        }
    }

    // [CAST-MOBILE-APPEARANCE-RECOVERY] The shared host reads the appearance entry from
    // its app-scoped store (`$HOME/.local/share/osprey/osprey/casting-appearance.txt` on
    // iOS), so the fixture writes that real file instead of a renderer test hook.
    private func writeAppearance(_ text: String) throws {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_APPEARANCE_PATH"],
                                 "Use the native test entry point to publish the app appearance path")
        let url = URL(fileURLWithPath: path)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        try Data(text.utf8).write(to: url, options: .atomic)
    }

    @MainActor
    func testNativeInvalidAppearanceRecovery() throws {
        // [CAST-MOBILE-APPEARANCE-RECOVERY] The fixture writes the real app-scoped entry.
        let app = castingApp()
        defer { app.terminate() }
        for saved in ["removed-skin", "", "poolside & <v2>' ☄"] {
            try writeAppearance(saved)
            app.launch()
            let notice = app.alerts["Appearance restored"]
            XCTAssertTrue(notice.waitForExistence(timeout: 15), app.debugDescription)
            XCTAssertTrue(notice.staticTexts["The selected appearance is unavailable. The default appearance has been restored."].exists)
            notice.buttons["OK"].tap()
            XCTAssertTrue(app.buttons["Open file…"].waitForExistence(timeout: 5))
            XCTAssertFalse(app.buttons["Skin"].exists)
            app.terminate()
            app.launch()
            XCTAssertTrue(app.buttons["Open file…"].waitForExistence(timeout: 15), app.debugDescription)
            XCTAssertFalse(notice.exists, "The repaired choice must not warn on the next launch")
            XCTAssertFalse(app.buttons["Skin"].exists)
            appearancePopup(app).tap()
            labeled(app, "Midnight Mint").tap()
            assertSelected(app, "Midnight Mint", "Choosing a skin must announce the new selection")
            app.terminate()
        }
    }

    @MainActor
    func testNativeBonjourDiscoveryAndRescan() throws {
        // [CAST-MOBILE-NATIVE-DISCOVERY] Real Bonjour, with equal names kept apart by endpoint.
        let app = castingApp()
        defer { app.terminate() }
        let receivers = try nativeReceivers()
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO_PATH"],
                                  "Use the native test entry point to stage the imported audio")
        app.launchArguments = ["--skin", "midnight", "--open", audio]
        app.launch()
        XCTAssertTrue(app.staticTexts["Receiver"].waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons[receivers.available].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.buttons[receivers.unavailable].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["Test receiver"].exists, "Native discovery must not use the injected transcript")
        app.buttons[receivers.available].tap()
        XCTAssertTrue(app.buttons[receivers.available].isSelected, "The selected receiver must be announced")
        app.buttons["Search again"].tap()
        XCTAssertTrue(app.staticTexts["Receiver"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons[receivers.available].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.buttons[receivers.available].isSelected, "Rescanning must retain and announce the selected endpoint")
        app.buttons[receivers.available].tap()
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
    private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor
    func testNativeLaunchDocumentCancellationAndRecovery() throws {
        // [CAST-DESIGN-STATE] The native picker opens, cancels, imports and reopens a real MP3.
        let app = castingApp()
        app.launchArguments = ["--skin", "sherbet"]
        defer { app.terminate() }
        let receivers = try nativeReceivers()
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO_PATH"],
                                  "Use the native test entry point to stage the imported audio")
        app.launch()
        let open = app.buttons["Open file…"]
        XCTAssertTrue(open.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].exists)
        selectReceiver(app, receivers.available)
        // Preserve the original appearance target contract; the shared Sherbet scene
        // does not currently offer it, so this journey must remain red rather than omit it.
        touchRegions([app.buttons["Play"], open, app.buttons["Receiver: \(receivers.available)"], appearancePopup(app)])
        attach(app, "casting-ios-ready")
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
        XCTAssertTrue(receiverPopup(app).waitForExistence(timeout: 10), app.debugDescription)
        selectReceiver(app, receivers.available)
        attach(app, "casting-ios-restarted")
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
        // [CAST-MOBILE-GPUI] The shared host opens a file from its argument, not a launch environment.
        app.launchArguments = ["--skin", "sherbet", "--open", audio]
        app.launch()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 15))
        selectReceiver(app, receivers.available)
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
        XCTAssertTrue(labeled(app, "1.5×").waitForExistence(timeout: 5))
        labeled(app, "1.5×").tap()
        XCTAssertTrue(app.buttons["Speed: 1.5×"].waitForExistence(timeout: 5), app.debugDescription)
        attach(app, "casting-ios-playing")
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15))
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
        for (skin, title) in ["midnight": "Midnight Mint", "poolside": "Poolside Pop"] {
            app.terminate()
            app.launchArguments = ["--skin", skin]
            app.launch()
            XCTAssertTrue(app.buttons["Open file…"].waitForExistence(timeout: 15), app.debugDescription)
            XCTAssertTrue(app.buttons[receivers.available].waitForExistence(timeout: 10), app.debugDescription)
            touchRegions([app.buttons["Play"], app.buttons["Open file…"],
                          app.buttons[receivers.available], labeled(app, title)])
            let empty = app.staticTexts["Choose an audio file"]
            XCTAssertTrue(empty.waitForExistence(timeout: 10), app.debugDescription)
            attach(app, "casting-ios-\(skin)-ready")
            let text = XCTAttachment(screenshot: empty.screenshot())
            text.name = "casting-ios-\(skin)-title"
            text.lifetime = .keepAlways
            add(text)
        }
    }

    @MainActor
    func testThemedPlaybackPreservesTheCastSession() throws {
        // [CAST-MOBILE-SKINS] Every skin renders and switches during playback without a reload.
        let app = castingApp()
        defer { app.terminate() }
        let receivers = try nativeReceivers()
        let audio = try XCTUnwrap(ProcessInfo.processInfo.environment["CAST_TEST_AUDIO_PATH"])
        app.launchArguments = ["--skin", "sherbet", "--open", audio]
        app.launch()
        XCTAssertTrue(labeled(app, "Midnight Mint").waitForExistence(timeout: 15),
                      "The shared appearance choice must reach every skin: \(app.debugDescription)")
        labeled(app, "Midnight Mint").tap()
        assertSelected(app, "Midnight Mint", "The chosen appearance must be announced")
        XCTAssertTrue(app.staticTexts["Receiver"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons[receivers.available].tap()
        XCTAssertTrue(app.buttons[receivers.available].isSelected, "The selected receiver must be announced")
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        touchRegions([app.buttons["Pause"], app.buttons["Stop"], app.buttons["1×"], app.buttons["100%"], app.buttons["Forward 10 seconds"]])
        assertSelected(app, "1×", "The playing rate must be announced")
        app.buttons["Pause"].tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5))
        app.buttons["Forward 10 seconds"].tap()
        app.buttons["50%"].tap()
        XCTAssertTrue(app.staticTexts["Volume 50%"].waitForExistence(timeout: 5), app.debugDescription)
        assertSelected(app, "50%", "The chosen volume must be announced")
        app.buttons["1.5×"].tap()
        assertSelected(app, "1.5×", "The chosen rate must be announced")
        labeled(app, "Poolside Pop").tap()
        XCTAssertTrue(app.buttons["Resume"].waitForExistence(timeout: 5), app.debugDescription)
        assertSelected(app, "Poolside Pop", "The chosen skin must be announced")
        XCTAssertTrue(app.staticTexts["Casting mobile test"].exists)
        app.buttons["Back 10 seconds"].tap()
        app.buttons["25%"].tap()
        XCTAssertTrue(app.staticTexts["Volume 25%"].waitForExistence(timeout: 5), app.debugDescription)
        assertSelected(app, "25%", "The chosen volume must be announced")
        app.buttons["0.75×"].tap()
        assertSelected(app, "0.75×", "The chosen rate must be announced")
        app.buttons["Resume"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5))
        XCTAssertTrue(labeled(app, "Sherbet Signal").waitForExistence(timeout: 5),
                      "Every skin must offer the shared appearance choice: \(app.debugDescription)")
        labeled(app, "Sherbet Signal").tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5), "The Sherbet skin must keep the live cast")
        XCTAssertTrue(labeled(app, "Poolside Pop").waitForExistence(timeout: 5),
                      "The Sherbet canvas must offer the shared appearance choice: \(app.debugDescription)")
        labeled(app, "Poolside Pop").tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 5), "Returning to Poolside must keep the live cast")
        app.buttons["1.25×"].tap()
        assertSelected(app, "1.25×", "The chosen rate must be announced")
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
        app.buttons[receivers.unavailable].tap()
        app.buttons["Play"].tap()
        let failure = app.staticTexts["\(receivers.host):1: connection refused or timed out"]
        XCTAssertTrue(failure.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(app.buttons["Play"].exists)
        labeled(app, "Midnight Mint").tap()
        XCTAssertTrue(failure.waitForExistence(timeout: 5), app.debugDescription)
        for (name, element) in [("loaded-title", app.staticTexts["Casting mobile test"]), ("search", app.buttons["Search again"]), ("receiver", app.staticTexts["Receiver"])] {
            let image = XCTAttachment(screenshot: element.screenshot())
            image.name = "casting-ios-midnight-\(name)"; image.lifetime = .keepAlways; add(image)
        }
        let refused = XCTAttachment(screenshot: app.screenshot())
        refused.name = "casting-ios-connection-refused"; refused.lifetime = .keepAlways; add(refused)
        app.buttons[receivers.available].tap()
        app.buttons["Play"].tap()
        XCTAssertTrue(app.buttons["Pause"].waitForExistence(timeout: 15), app.debugDescription)
        app.buttons["Stop"].tap()
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 5))
    }

}
