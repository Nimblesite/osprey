import SwiftUI

@MainActor
enum BankRuntime {
    private static var initialized = false
    static func start() throws -> String {
        if !initialized {
            let status = osprey_main()
            guard status == 0 else { throw BankFailure.invalid("Osprey initialization failed: \(status)") }
            initialized = true
        }
        return try copy(osprey_talonmobile_start())
    }
    static func dispatch(_ payload: String) throws -> String {
        try payload.withCString { try copy(osprey_talonmobile_dispatch($0)) }
    }
    private static func copy(_ pointer: UnsafePointer<CChar>?) throws -> String {
        guard let pointer, let value = String(validatingUTF8: pointer) else {
            throw BankFailure.invalid("Osprey returned an invalid envelope")
        }
        return value
    }
}

@MainActor
final class BankStore: ObservableObject {
    @Published private(set) var view: BankNode?
    @Published private(set) var hostError: String?
    @Published private(set) var focusID = ""
    @Published private(set) var serverAddress: String
    @Published private(set) var busy = false
    @Published private(set) var route = "overview"
    // Not published: a keystroke that re-rendered every node lost the keystrokes that followed it.
    private(set) var drafts: [String: String] = [:]
    private(set) var model = ""
    private var commands: [BankCommand] = []
    private var work: Task<Void, Never>?
    private var generation = 0
    private var formScope = ""
    private var previousModal = "closed"

    init() {
        let args = ProcessInfo.processInfo.arguments
        let index = args.firstIndex(of: "--talon-server-url")
        serverAddress = index.flatMap { args.indices.contains($0 + 1) ? args[$0 + 1] : nil }
            ?? UserDefaults.standard.string(forKey: "talon-server") ?? "http://127.0.0.1:18790"
    }

    func start() {
        guard view == nil, hostError == nil else { return }
        do { _ = try BankProtocol.server(serverAddress); try accept(BankRuntime.start()) }
        catch { fail(error) }
    }

    func connect(_ address: String) throws {
        serverAddress = try BankProtocol.server(address).absoluteString
        UserDefaults.standard.set(serverAddress, forKey: "talon-server")
        generation += 1
        work?.cancel(); work = nil; commands = []; view = nil; hostError = nil; drafts = [:]
        start()
    }

    func send(_ event: [String: Any]) {
        guard view != nil, hostError == nil else { return }
        do {
            var payload = event
            payload["model"] = model
            try accept(BankRuntime.dispatch(BankProtocol.json(payload)))
        } catch { fail(error) }
    }

    func click(_ id: String) { send(["kind": "click", "id": id]) }

    func submit(_ form: BankNode) {
        do { send(["kind": "submit", "id": form.id, "data": try BankProtocol.json(BankProtocol.form(form, drafts: drafts))]) }
        catch { fail(error) }
    }

    func value(_ node: BankNode) -> String { drafts[node.id] ?? node.initialValue }

    func edit(_ node: BankNode, _ value: String) {
        drafts[node.id] = value
        if let event = node.props["event"] {
            send(["kind": event, "id": node.id, "name": node.props["name"] ?? "", "value": value])
        }
    }

    // A picker shows the stored choice, so choosing is the one edit that republishes.
    func choose(_ node: BankNode, _ value: String) { objectWillChange.send(); edit(node, value) }

    func waitUntilIdle() async { await work?.value }

    private func accept(_ json: String) throws {
        let envelope = try JSONDecoder().decode(BankEnvelope.self, from: Data(json.utf8))
        guard envelope.commands.allSatisfy({ ["http", "focus", "navigate"].contains($0.kind) }) else {
            throw BankFailure.invalid("Unknown Osprey host command")
        }
        try resetFields(envelope.model)
        model = envelope.model
        view = envelope.view
        commands.append(contentsOf: envelope.commands)
        if work == nil, !commands.isEmpty {
            let current = generation
            work = Task { await drain(current) }
        }
    }

    private func resetFields(_ next: String) throws {
        let state = try JSONDecoder().decode([String: String].self, from: Data(next.utf8))
        let scope = ["route", "moveMode"].map { state[$0] ?? "" }.joined(separator: "|")
        let nextBusy = state["busy"] == "true"
        if scope != formScope { drafts = [:] }
        if previousModal != state["modal"] { drafts.removeValue(forKey: "new-owner") }
        formScope = scope
        previousModal = state["modal"] ?? "closed"
        busy = nextBusy
        route = state["route"] ?? "overview"
    }

    private func drain(_ current: Int) async {
        while !commands.isEmpty, hostError == nil, generation == current, !Task.isCancelled {
            let command = commands.removeFirst()
            switch command.kind {
            case "http":
                let event = await request(command)
                if generation == current, !Task.isCancelled { send(event) }
            case "focus": focusID = command.id ?? ""
            case "navigate": break // Shared Update already changed the route; no browser history is needed.
            default: fail(BankFailure.invalid("Unknown host command: \(command.kind)"))
            }
        }
        if generation == current { work = nil }
    }

    private func request(_ command: BankCommand) async -> [String: Any] {
        let id = command.id ?? ""
        do {
            let request = try BankProtocol.request(command, server: BankProtocol.server(serverAddress))
            let (bytes, response) = try await URLSession.shared.data(for: request)
            guard let http = response as? HTTPURLResponse, let data = String(data: bytes, encoding: .utf8) else {
                throw BankFailure.invalid("Ledger returned a non-HTTP or invalid UTF-8 response")
            }
            return ["kind": "http", "id": id, "status": http.statusCode, "data": data]
        } catch {
            let detail = (try? BankProtocol.json(["error": "Could not reach the ledger: \(error.localizedDescription)"])) ?? "{}"
            return ["kind": "http", "id": id, "status": 0, "data": detail]
        }
    }

    private func fail(_ error: Error) {
        hostError = error.localizedDescription
        commands = []
    }
}
