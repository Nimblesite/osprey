import XCTest
import UIKit

final class BankUITests: XCTestCase {
    private var app = XCUIApplication()
    private var server: String {
        if let value = ProcessInfo.processInfo.environment["TALON_TEST_SERVER_URL"], value.hasPrefix("http") { return value }
        return "http://127.0.0.1:18790"
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
        app.launchArguments = ["--talon-server-url", server]
        app.launch()
        XCTAssertTrue(app.buttons["account-1"].waitForExistence(timeout: 30), "Live bank accounts did not load")
    }

    func testNativeBankWorkflowAndLedgerBalances() throws {
        capture("overview")
        let owner = "iOS O'Reilly \(UUID().uuidString.prefix(8))"
        try openAccount(owner)
        let accounts = try rows("/api/accounts")
        let account = try XCTUnwrap(accounts.first { $0["owner"] as? String == owner })
        let id = try XCTUnwrap(account["id"] as? Int)
        let destinationBefore = try balance(1)
        try deposit(id, amount: "25.50", note: "iOS native deposit")
        try withdraw(id, amount: "2.25", note: "iOS native withdrawal", expected: "Withdrawal complete")
        try withdraw(id, amount: "999999.99", note: "iOS refused overdraft", expected: "Operation refused")
        XCTAssertEqual(try balance(id), 2325)
        try transfer(id, to: 1, amount: "10.00")
        XCTAssertEqual(try balance(id), 1325)
        XCTAssertEqual(try balance(1), destinationBefore + 1000)
        try verifyJournal(id)
        verifyActivityAndSecurity()
    }

    func testValidationModalCancellationAndRefresh() {
        tap("topbar-open")
        tap("submit-open-account-button")
        expectText("Owner name required")
        tap("close-modal")
        XCTAssertFalse(app.textFields["new-owner"].exists)
        route("move")
        tap("move-deposit")
        set("deposit-amount", "1.001")
        tap("submit-deposit-button")
        expectText("Check the deposit")
        tap("move-transfer")
        choose("transfer-from", account: 1)
        choose("transfer-to", account: 1)
        set("transfer-amount", "1.00")
        tap("submit-transfer-button")
        expectText("Choose two accounts")
        tap("topbar-refresh")
        route("overview")
        XCTAssertTrue(app.buttons["account-1"].waitForExistence(timeout: 20))
    }

    func testAccountSelectionCarriesIntoNativeMovementForm() throws {
        let account = try XCTUnwrap(rows("/api/accounts").first { $0["id"] as? Int == 2 })
        let owner = try XCTUnwrap(account["owner"] as? String)
        let amount = try XCTUnwrap(account["balance"] as? String)
        route("accounts")
        tap("account-2")
        expectText(owner)
        expectText(amount)
        capture("account-detail")
        tap("quick-deposit")
        let selection = app.buttons["deposit-account"]
        XCTAssertTrue(selection.waitForExistence(timeout: 5))
        XCTAssertEqual(selection.value as? String, "#2 · \(owner)")
    }

    func testServerFailureAndRecoveryRemainUsable() {
        configure("http://127.0.0.1:1")
        expectText("Bank data unavailable")
        capture("unavailable")
        configure(server)
        XCTAssertTrue(app.buttons["account-1"].waitForExistence(timeout: 30))
        route("accounts")
        capture("accounts")
        app.terminate()
        app.launch()
        XCTAssertTrue(app.buttons["account-1"].waitForExistence(timeout: 30))
    }

    func testPortraitLandscapeAndTabletLayout() {
        for orientation in [UIDeviceOrientation.portrait, .landscapeLeft] {
            XCUIDevice.shared.orientation = orientation
            route("move")
            tap("move-transfer")
            reveal(app.buttons["transfer-to"])
            XCTAssertTrue(app.buttons["transfer-to"].isHittable)
            capture(orientation == .portrait ? "move-portrait" : "move-landscape")
            route("overview")
        }
        XCUIDevice.shared.orientation = .portrait
    }

    private func openAccount(_ owner: String) throws {
        tap("topbar-open")
        XCTAssertTrue(app.textFields["new-owner"].waitForExistence(timeout: 5))
        set("new-owner", owner)
        capture("open-account")
        tap("submit-open-account-button")
        expectText("Account opened")
        dismissNotice()
    }

    private func deposit(_ id: Int, amount: String, note: String) throws {
        route("move")
        tap("move-deposit")
        choose("deposit-account", account: id)
        set("deposit-amount", amount)
        set("deposit-note", note)
        capture("deposit")
        tap("submit-deposit-button")
        expectText("Deposit complete")
        dismissNotice()
        XCTAssertEqual(try balance(id), 2550)
    }

    private func withdraw(_ id: Int, amount: String, note: String, expected: String) throws {
        tap("move-withdraw")
        choose("withdraw-account", account: id)
        set("withdraw-amount", amount)
        set("withdraw-note", note)
        tap("submit-withdraw-button")
        expectText(expected)
        dismissNotice()
    }

    private func transfer(_ from: Int, to: Int, amount: String) throws {
        tap("move-transfer")
        choose("transfer-from", account: from)
        choose("transfer-to", account: to)
        set("transfer-amount", amount)
        set("transfer-note", "iOS atomic transfer \(from)")
        tap("submit-transfer-button")
        expectText("Transfer complete")
        dismissNotice()
    }

    private func verifyJournal(_ id: Int) throws {
        let activity = try rows("/api/activity")
        let refused = activity.filter { $0["account"] as? Int == id && $0["kind"] as? String == "refused" }
        XCTAssertTrue(refused.contains { $0["note"] as? String == "iOS refused overdraft" })
        let transfer = activity.filter { $0["note"] as? String == "iOS atomic transfer \(id)" }
        XCTAssertEqual(transfer.compactMap { $0["kind"] as? String }.sorted(), ["credit", "debit"])
    }

    private func verifyActivityAndSecurity() {
        route("activity")
        set("activity-search", "iOS refused overdraft")
        tap("filter-refused")
        expectText("iOS refused overdraft")
        capture("activity")
        set("activity-search", "no-such-ledger-entry-zzzzz")
        expectText("No matching activity")
        route("security")
        expectText("Trust you can inspect")
        capture("security")
    }

    private func route(_ name: String) {
        dismissNotice()
        let destination = app.buttons["nav-\(name)"]
        if !destination.isHittable { tap("toggle-menu") }
        reveal(destination)
        destination.tap()
    }

    private func configure(_ url: String) {
        let settings = app.buttons["server-settings"]
        if !settings.isHittable { tap("toggle-menu") }
        reveal(settings); settings.tap()
        set("server-address", url)
        tap("server-connect")
    }

    private func choose(_ field: String, account: Int) {
        tap(field)
        let identified = app.buttons["\(field)-option-\(account)"]
        let labelled = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "#\(account) ·")).firstMatch
        let option = identified.exists ? identified : labelled
        XCTAssertTrue(option.waitForExistence(timeout: 5), "Missing native account option \(account)")
        option.tap()
    }

    private func set(_ id: String, _ value: String) {
        let field = app.textFields[id]
        reveal(field); field.tap()
        let current = field.value as? String ?? ""
        if !current.isEmpty && current != field.placeholderValue {
            field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count))
        }
        field.typeText(value)
        if app.buttons["keyboard-done"].exists { app.buttons["keyboard-done"].tap() }
    }

    private func tap(_ id: String) { let element = app.buttons[id]; reveal(element); element.tap() }
    private func dismissNotice() {
        if app.buttons["dismiss-notice"].exists { app.buttons["dismiss-notice"].tap() }
    }
    private func reveal(_ element: XCUIElement) {
        XCTAssertTrue(element.waitForExistence(timeout: 10))
        for _ in 0..<8 { if element.isHittable { return }; app.swipeUp() }
        for _ in 0..<8 { if element.isHittable { return }; app.swipeDown() }
        XCTAssertTrue(element.isHittable, "Native control is not reachable: \(element)")
    }
    private func expectText(_ text: String) { XCTAssertTrue(app.staticTexts[text].firstMatch.waitForExistence(timeout: 30), text) }
    private func capture(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    private func balance(_ id: Int) throws -> Int {
        let account = try XCTUnwrap(rows("/api/accounts").first { $0["id"] as? Int == id })
        return try XCTUnwrap(account["cents"] as? Int)
    }
    private func rows(_ path: String) throws -> [[String: Any]] {
        let url = try XCTUnwrap(URL(string: server + path))
        let complete = expectation(description: path)
        var outcome: Result<Data, Error> = .failure(URLError(.unknown))
        URLSession.shared.dataTask(with: url) { bytes, response, error in
            if let error { outcome = .failure(error) }
            else if let bytes, (response as? HTTPURLResponse)?.statusCode == 200 { outcome = .success(bytes) }
            else { outcome = .failure(URLError(.badServerResponse)) }
            complete.fulfill()
        }.resume()
        wait(for: [complete], timeout: 20)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: outcome.get()) as? [[String: Any]])
    }
}
