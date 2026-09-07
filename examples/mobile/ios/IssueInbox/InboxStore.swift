import Foundation
import Combine

struct InboxItem: Identifiable, Decodable {
    let id: String
    let number: Int64
    let title: String
    let url: String
    let author: String
    let comments: Int64
    let bookmarked: Bool
    let body: String
    let labels: String
    let note: String
    let priority: String
}

struct InboxSnapshot: Decodable {
    let repo: String
    let search: String
    let filter: String
    let status: String
    let error: String
    let loading: Bool
    let items: [InboxItem]
    let total: Int64
    let bookmarked: Int64
    let selected: String
    let detail: InboxItem?

    static let empty = InboxSnapshot(repo: "", search: "", filter: "all", status: "Starting Osprey",
                                     error: "", loading: false, items: [], total: 0, bookmarked: 0, selected: "", detail: nil)
}

private struct InboxEnvelope: Decodable {
    let model: String
    let view: InboxSnapshot
    let ui: InboxNode
    let commands: [HostCommand]
}

// Implements [MOBILE-REACTIVE-HOST]: retain opaque state and execute Osprey commands.
@MainActor
final class InboxStore: ObservableObject {
    @Published private(set) var snapshot = InboxSnapshot.empty
    @Published private(set) var ui: InboxNode?
    @Published private(set) var hostError: String?
    private var model = ""
    private var started = false
    private var commands: [HostCommand] = []
    private var work: Task<Void, Never>?
    private var services: HostServices?

    var loading: Bool { hostError == nil && snapshot.loading }
    var error: String { hostError ?? snapshot.error }
    var status: String { snapshot.status }

    init(services: HostServices? = nil) {
        do { self.services = try services ?? HostServices() }
        catch { hostError = error.localizedDescription }
    }

    func start() {
        guard !started, services != nil else { return }
        started = true
        do {
            let result = osprey_main()
            guard result == 0 else { throw HostFailure.invalid("Osprey initialization failed: \(result)") }
            try accept(copyString(osprey_mobile_start()))
        } catch { fail(error) }
    }

    func send(_ event: [String: Any]) {
        guard started, hostError == nil else { return }
        do {
            let data = try JSONSerialization.data(withJSONObject: event, options: [.sortedKeys])
            guard let json = String(data: data, encoding: .utf8) else { throw HostFailure.invalid("Event is not UTF-8 JSON") }
            let next = try model.withCString { state in
                try json.withCString { event in try copyString(osprey_mobile_dispatch(state, event)) }
            }
            try accept(next)
        } catch { fail(error) }
    }

    func waitUntilIdle() async { await work?.value }

    private func accept(_ json: String) throws {
        let envelope = try JSONDecoder().decode(InboxEnvelope.self, from: Data(json.utf8))
        guard envelope.commands.allSatisfy(validCommand) else { throw HostFailure.invalid("Osprey returned an invalid host command") }
        try writeDiagnostics(json)
        model = envelope.model
        snapshot = envelope.view
        ui = envelope.ui
        commands.append(contentsOf: envelope.commands)
        if work == nil, !commands.isEmpty { work = Task { await drain() } }
    }

    private func drain() async {
        while !commands.isEmpty, hostError == nil, let services {
            let command = commands.removeFirst()
            send(await services.execute(command))
        }
        work = nil
    }

    private func validCommand(_ command: HostCommand) -> Bool {
        guard !command.id.isEmpty else { return false }
        switch command.kind {
        case "sql": return command.sql != nil && command.params != nil
        case "http": return command.url != nil
        default: return false
        }
    }

    private func copyString(_ pointer: UnsafePointer<CChar>?) throws -> String {
        guard let pointer, let value = String(validatingUTF8: pointer) else {
            throw HostFailure.invalid("Osprey returned a null or invalid UTF-8 envelope")
        }
        return value
    }

    private func writeDiagnostics(_ json: String) throws {
        guard ProcessInfo.processInfo.arguments.contains("--inbox-diagnostics") else { return }
        let directory = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask,
                                                    appropriateFor: nil, create: true)
        try json.write(to: directory.appendingPathComponent("inbox-state.json"), atomically: true, encoding: .utf8)
    }

    private func fail(_ error: Error) {
        hostError = "Osprey host: \(error.localizedDescription)"
        commands.removeAll()
    }
}
