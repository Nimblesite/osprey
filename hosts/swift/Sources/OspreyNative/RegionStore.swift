#if os(macOS)
import AppKit
#endif
import Combine
import Foundation

// Implements [UI-REGIONS], [UI-EVENTS], [UI-HOST] and [UI-DIAGNOSTICS]: keeps
// each region as an independently observable cell, delivers events with
// framework-issued identities and runs the requests of each envelope. It holds
// no application state: Osprey's opaque `state` is returned unchanged.

@MainActor
public final class RegionCell: ObservableObject, Identifiable {
    public let key: String
    @Published public private(set) var revision: Int
    @Published public private(set) var node: JSONValue

    init(key: String, revision: Int, node: JSONValue) {
        self.key = key
        self.revision = revision
        self.node = node
    }

    func replace(revision: Int, node: JSONValue) {
        self.revision = revision
        self.node = node
    }
}

public struct HostContext: Sendable {
    public let platform: String
    public let home: String
    public let arguments: [String]

    public init(platform: String, home: String, arguments: [String] = []) {
        self.platform = platform
        self.home = home
        self.arguments = arguments
    }

    var now: Int { Int(Date().timeIntervalSince1970) }

    var startJSON: String {
        JSONValue.object(["now": .number(Double(now)), "offset": .number(Double(TimeZone.current.secondsFromGMT())), "platform": .string(platform), "home": .string(home), "arguments": .array(arguments.map { .string($0) })]).text
    }
}

@MainActor
public final class RegionStore: ObservableObject {
    @Published public private(set) var cells: [String: RegionCell] = [:]
    @Published public private(set) var theme: Theme?
    @Published public private(set) var diagnostics: [String] = []
    @Published public private(set) var failure: String?
    private var state = ""
    private let library: Library
    private let context: HostContext
    private let capabilities: CapabilityRunner
    private let slices: SliceStore

    public init(library: Library, context: HostContext, capabilities: CapabilityRunner, slices: SliceStore = .shared) {
        self.library = library
        self.context = context
        self.capabilities = capabilities
        self.slices = slices
    }

    public var root: RegionCell? { cells["root"] }

    public func cell(_ key: String) -> RegionCell? { cells[key] }

    public func start() {
        guard state.isEmpty, failure == nil else { return }
        do { try accept(library.start(context: context.startJSON)) } catch { fail("Osprey could not start: \(error)") }
    }

    /// A control event: the region, the revision it was drawn with, the control's ordinal and its value.
    public func send(region: String, revision: Int, ordinal: Int, value: JSONValue) {
        deliver(.object(["kind": "ui", "region": .string(region), "revision": .number(Double(revision)), "ordinal": .number(Double(ordinal)), "value": value, "now": .number(Double(context.now))]))
    }

    /// A capability completion for `ticket`.
    public func complete(ticket: String, outcome: Result<JSONValue, CapabilityError>) {
        switch outcome {
        case let .success(result):
            deliver(.object(["kind": "completed", "ticket": .string(ticket), "ok": true, "result": result, "now": .number(Double(context.now))]))
        case let .failure(problem):
            Log.events.error("Request \(ticket, privacy: .public) failed: \(problem.description, privacy: .private)")
            deliver(.object(["kind": "completed", "ticket": .string(ticket), "ok": false, "error": .string(problem.description), "now": .number(Double(context.now))]))
        }
    }

    private func deliver(_ event: JSONValue) {
        guard !state.isEmpty else { return }
        do { try accept(library.dispatch(state: state, event: event.text)) } catch { fail("Osprey could not handle an event: \(error)") }
    }

    private func accept(_ text: String) throws {
        let envelope = try Envelope.decode(text)
        state = envelope.state
        envelope.slices.forEach { slices.write($0.key, $0.text) }
        if let theme = envelope.theme { self.theme = theme }
        for patch in envelope.patches {
            if let cell = cells[patch.key] { cell.replace(revision: patch.revision, node: patch.node) } else { cells[patch.key] = RegionCell(key: patch.key, revision: patch.revision, node: patch.node) }
        }
        envelope.removed.forEach { cells.removeValue(forKey: $0) }
        if !envelope.diagnostics.isEmpty {
            diagnostics.append(contentsOf: envelope.diagnostics)
            envelope.diagnostics.forEach { Log.regions.error("Osprey: \($0, privacy: .public)") }
        }
        envelope.requests.forEach { request in
            capabilities.run(request) { [weak self] outcome in self?.complete(ticket: request.ticket, outcome: outcome) }
        }
    }

    private func fail(_ problem: String) {
        Log.regions.fault("\(problem, privacy: .public)")
        failure = problem
        diagnostics.append(problem)
    }
}

/// The standard adapters this host answers ([GPUI-HOST-CAPABILITIES] names the same kinds).
public enum Adapters {
    public static func standard(store: StoreAdapter) -> [CapabilityAdapter] {
        #if os(macOS)
        [RunAdapter(), ProcessAdapter(), HttpAdapter(), store, ChooseFolderAdapter()]
        #else
        [HttpAdapter(), store, DocumentAdapter()]
        #endif
    }
}

/// The system's folder picker; the person's choice comes back as a path, or `cancelled`.
#if os(macOS)
public struct ChooseFolderAdapter: CapabilityAdapter {
    public let kinds = ["choose-folder"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        await MainActor.run {
            let panel = NSOpenPanel()
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.allowsMultipleSelection = false
            let chosen = panel.runModal() == .OK ? panel.url?.path : nil
            return JSONValue.object(["path": .string(chosen ?? ""), "cancelled": .bool(chosen == nil)])
        }
    }
}

#endif
