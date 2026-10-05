import XCTest
@testable import TalonBank

final class BankProtocolTests: XCTestCase {
    func testNativeFormKeepsDefaultsAndExactUserText() throws {
        let form = try node(#"{"tag":"form","props":{"id":"submit-deposit"},"children":[{"tag":"select","props":{"id":"account","name":"account","defaultValue":"2"},"children":[{"tag":"option","props":{"value":"1"},"text":"One"},{"tag":"option","props":{"value":"2"},"text":"Two"}]},{"tag":"input","props":{"id":"amount","name":"amount"}},{"tag":"input","props":{"id":"note","name":"note"}}]}"#)
        let values = BankProtocol.form(form, drafts: ["amount": "12.34", "note": "O'Reilly 🦉 \"bank\"\n"])
        XCTAssertEqual(values, ["account": "2", "amount": "12.34", "note": "O'Reilly 🦉 \"bank\"\n"])
        let event = try BankProtocol.json(["kind": "submit", "data": BankProtocol.json(values)])
        let decoded = try JSONDecoder().decode([String: String].self, from: Data(event.utf8))
        XCTAssertEqual(try JSONDecoder().decode([String: String].self, from: Data((decoded["data"] ?? "").utf8)), values)
    }

    func testSelectDefaultsToFirstOptionAndExplicitChoiceOverridesIt() throws {
        let form = try node(#"{"tag":"form","props":{},"children":[{"tag":"select","props":{"id":"account","name":"account"},"children":[{"tag":"option","props":{"value":"7"},"text":"Seven"}]},{"tag":"input","props":{"id":"disabled","name":"ignored","disabled":"true"}}]}"#)
        XCTAssertEqual(BankProtocol.form(form, drafts: [:]), ["account": "7"])
        XCTAssertEqual(BankProtocol.form(form, drafts: ["account": "9"]), ["account": "9"])
    }

    func testPostRequestPreservesBankJSONAndSetsHeaders() throws {
        let command = try command(#"{"kind":"http","id":"mutate-deposit","method":"POST","url":"/api/deposit","body":"{\"account\":1,\"cents\":1234}"}"#)
        let request = try BankProtocol.request(command, server: BankProtocol.server("http://127.0.0.1:18790"))
        XCTAssertEqual(request.url?.absoluteString, "http://127.0.0.1:18790/api/deposit")
        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(String(data: request.httpBody ?? Data(), encoding: .utf8), #"{"account":1,"cents":1234}"#)
        XCTAssertTrue(request.value(forHTTPHeaderField: "X-Osprey-Request-Id")?.hasPrefix("ios-mutate-deposit-") == true)
    }

    func testServerOriginValidationAndHTTPDestination() throws {
        for value in ["file:///tmp/bank", "https://user:password@example.com", "https://example.com/other", "https://example.com#bad", "http://"] {
            XCTAssertThrowsError(try BankProtocol.server(value), value)
        }
        for path in ["https://other.example/api/accounts", "//other.example/api/accounts", "/api/../private"] {
            let request = try command(BankProtocol.json(["kind": "http", "id": "a", "method": "GET", "url": path]))
            XCTAssertThrowsError(try BankProtocol.request(request, server: BankProtocol.server("https://bank.example")))
        }
    }

    func testGetNeverSendsBodyAndEnvelopeModelRemainsOpaque() throws {
        let get = try command(#"{"kind":"http","id":"accounts","method":"GET","url":"/api/accounts","body":"ignored"}"#)
        XCTAssertNil(try BankProtocol.request(get, server: BankProtocol.server("https://bank.example")).httpBody)
        let model = #"{"route":"move","selected":"42"}"#
        let data = try BankProtocol.json(["model": model, "view": ["tag": "div", "props": [:]], "commands": []])
        XCTAssertEqual(try JSONDecoder().decode(BankEnvelope.self, from: Data(data.utf8)).model, model)
    }

    func testSemanticTreePreservesAllNativeControlLabels() throws {
        let tree = try node(#"{"tag":"div","props":{},"children":[{"tag":"button","props":{"id":"quick-transfer","className":"button primary"},"children":[{"tag":"span","props":{},"text":"⇄"},{"tag":"span","props":{},"text":"Move money"}]}]}"#)
        let button = tree.first { $0.id == "quick-transfer" }
        XCTAssertEqual(button?.label, "⇄ Move money")
        XCTAssertTrue(button?.has("primary") == true)
        XCTAssertNil(tree.first { $0.id == "missing" })
    }

    func testAccessibleLabelsExcludeDecorativeIconsAndRespectExplicitLabels() throws {
        let close = try node(#"{"tag":"button","props":{"id":"close-modal"},"children":[{"tag":"span","props":{"aria-hidden":"true"},"text":"×"},{"tag":"span","props":{},"text":"Close"}]}"#)
        XCTAssertEqual(close.label, "× Close")
        XCTAssertEqual(close.accessibleLabel, "Close")
        let notifications = try node(#"{"tag":"button","props":{"aria-label":"Activity notifications"},"children":[{"tag":"span","props":{"aria-hidden":"true"},"text":"♢"}]}"#)
        XCTAssertEqual(notifications.accessibleLabel, "Activity notifications")
        XCTAssertEqual(notifications.nodes.first?.accessibleLabel, "")
    }

    func testSharedButtonIconClassIsVisualAndExcludedFromAccessibleLabel() throws {
        let close = try node(#"{"tag":"button","props":{"id":"close-modal","className":"icon-button modal-close","event":"click","type":"button"},"children":[{"tag":"span","text":"×","props":{"className":"button-icon"}},{"tag":"span","text":"Close","props":{"className":"button-label"}}]}"#)
        XCTAssertEqual(close.label, "× Close")
        XCTAssertEqual(close.accessibleLabel, "Close")
    }

    private func node(_ json: String) throws -> BankNode { try JSONDecoder().decode(BankNode.self, from: Data(json.utf8)) }
    private func command(_ json: String) throws -> BankCommand { try JSONDecoder().decode(BankCommand.self, from: Data(json.utf8)) }
}
