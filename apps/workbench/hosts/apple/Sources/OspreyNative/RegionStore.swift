import Foundation
import Combine

// Implements [UI-REGIONS], [UI-EVENTS], [UI-HOST] and [UI-DIAGNOSTICS]: keeps
// each region as an independently observable cell, delivers events with
// framework-issued identities, runs the capability requests of each envelope
// and logs every event and envelope. It holds no application state: Osprey's
// opaque `state` is returned unchanged.

@MainActor
public final class RegionCell: ObservableObject, Identifiable {
    public let key: String
    @Published public private(set) var revision: Int
    @Published public private(set) var node: Node

    init(key: String, revision: Int, node: Node) {
        self.key = key
        self.revision = revision
        self.node = node
    }

    func replace(revision: Int, node: Node) {
        self.revision = revision
        self.node = node
    }
}

public struct HostContext: Sendable {
    public let platform: String
    public let home: String
    public let arguments: [String]
    public let clock: @Sendable () -> Date

    public init(platform: String, home: String, arguments: [String] = [], clock: @escaping @Sendable () -> Date = { Date() }) {
        self.platform = platform
        self.home = home
        self.arguments = arguments
        self.clock = clock
    }

    var now: Int { Int(clock().timeIntervalSince1970) }

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
        Log.regions.info("Starting on \(self.context.platform, privacy: .public) with \(self.context.arguments.count) launch arguments")
        do { try accept(library.start(context: context.startJSON)) } catch { fail("Osprey could not start: \(error)") }
    }

    /// A control event: the region, the revision it was drawn with, the control's ordinal and its value.
    public func send(region: String, revision: Int, ordinal: Int, value: JSONValue) {
        Log.events.debug("Control \(ordinal) of \(region, privacy: .public)@\(revision) sent \(value.text, privacy: .private)")
        deliver(.object(["kind": "ui", "region": .string(region), "revision": .number(Double(revision)), "ordinal": .number(Double(ordinal)), "value": value, "now": .number(Double(context.now))]))
    }

    /// A capability completion for `ticket`.
    public func complete(ticket: String, outcome: Result<JSONValue, CapabilityError>) {
        switch outcome {
        case let .success(result):
            Log.events.debug("Request \(ticket, privacy: .public) completed")
            deliver(.object(["kind": "completed", "ticket": .string(ticket), "ok": true, "result": result, "now": .number(Double(context.now))]))
        case let .failure(problem):
            Log.events.error("Request \(ticket, privacy: .public) failed: \(problem.description, privacy: .private)")
            deliver(.object(["kind": "completed", "ticket": .string(ticket), "ok": false, "error": .string(problem.description), "now": .number(Double(context.now))]))
        }
    }

    public func lifecycle(_ phase: String) {
        Log.events.info("Lifecycle \(phase, privacy: .public)")
        deliver(.object(["kind": "lifecycle", "phase": .string(phase), "now": .number(Double(context.now))]))
    }

    private func deliver(_ event: JSONValue) {
        Trace.line("event " + event.text)
        guard !state.isEmpty else { return }
        do { try accept(library.dispatch(state: state, event: event.text)) } catch { fail("Osprey could not handle an event: \(error)") }
    }

    private func accept(_ text: String) throws {
        let envelope = try Envelope.decode(text)
        state = envelope.state
        envelope.slices.forEach { slices.write($0.key, $0.text) }
        apply(envelope)
        if !envelope.diagnostics.isEmpty { diagnostics.append(contentsOf: envelope.diagnostics) }
        trace(envelope)
        envelope.requests.forEach { request in
            capabilities.run(request) { [weak self] outcome in self?.complete(ticket: request.ticket, outcome: outcome) }
        }
    }

    /// Only regions whose revision moved arrive, so untouched cells are not republished.
    private func apply(_ envelope: Envelope) {
        if let theme = envelope.theme { self.theme = theme }
        for patch in envelope.patches {
            if let cell = cells[patch.key] { cell.replace(revision: patch.revision, node: patch.node) } else { cells[patch.key] = RegionCell(key: patch.key, revision: patch.revision, node: patch.node) }
        }
        envelope.removed.forEach { cells.removeValue(forKey: $0) }
    }

    /// One line per envelope, and each of Osprey's diagnostics as an error.
    private func trace(_ envelope: Envelope) {
        let patched = envelope.patches.map { "\($0.key)@\($0.revision)" }.joined(separator: " ")
        let requested = envelope.requests.map { "\($0.kind) \($0.ticket)" }.joined(separator: ", ")
        Trace.line("patched [\(patched)] requested [\(requested)] diagnostics \(envelope.diagnostics)")
        Log.regions.debug("Patched [\(patched, privacy: .public)] removed [\(envelope.removed.joined(separator: " "), privacy: .public)] requested [\(requested, privacy: .public)]")
        envelope.diagnostics.forEach { Log.regions.error("Osprey: \($0, privacy: .public)") }
    }

    private func fail(_ problem: String) {
        Trace.line("failure " + problem)
        Log.regions.fault("\(problem, privacy: .public)")
        failure = problem
        diagnostics.append(problem)
    }
}
