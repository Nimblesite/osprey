import Combine
import Foundation

// Implements [UI-REGIONS], [UI-EVENTS] and [NATIVE-SHM-NODES]: keeps each region the application
// told as an independently observable cell, so only the regions whose tree changed draw again,
// and delivers control events by the ordinal the frame issued. It holds no application state:
// the session lives in the application's library, and the frames arrive through `SharedStore`.

@MainActor
public final class RegionCell: ObservableObject, Identifiable {
    public let key: String
    @Published public private(set) var revision: Int
    @Published public private(set) var node: Tree

    init(key: String, revision: Int, node: Tree) {
        self.key = key
        self.revision = revision
        self.node = node
    }

    func replace(revision: Int, node: Tree) {
        self.revision = revision
        self.node = node
    }

    // [UI-EVENTS] A click can outlive a frame, never a changed control binding.
    func controlRevision(for expected: Tree, value: Tree) -> Int? {
        func contains(_ value: Tree) -> Bool {
            if value == expected { return true }
            return (value.array ?? Array(value.object?.values ?? [:].values)).contains(where: contains)
        }
        let valid: Bool
        switch expected.text("kind") {
        case "button": valid = expected.flag("enabled") && value == .null
        case "picker": valid = value.string.map { key in expected.list("choices").contains { $0.text("key") == key } } ?? false
        default: valid = false
        }
        guard valid, expected["event"]?.int != nil, contains(node) else { return nil }
        return revision
    }
}

@MainActor
public final class RegionStore: ObservableObject {
    @Published public private(set) var cells: [String: RegionCell] = [:]
    @Published public private(set) var theme: Theme?
    @Published public private(set) var diagnostics: [String] = []
    @Published public private(set) var failure: String?
    /// Whether the last frame told components; a frame of plain view rows is drawn by the row view.
    @Published public private(set) var told = true
    /// The frames, events and capabilities behind the regions.
    public let shared: SharedStore
    private let arguments: [String]
    private var watching: Set<AnyCancellable> = []

    public init(shared: SharedStore, arguments: [String] = []) {
        self.shared = shared
        self.arguments = arguments
        shared.$shown.dropFirst().sink { [weak self] shown in self?.adopt(shown) }.store(in: &watching)
        shared.$diagnostics.sink { [weak self] all in self?.diagnostics = all }.store(in: &watching)
        shared.$failure.sink { [weak self] problem in self?.failure = problem }.store(in: &watching)
    }

    public var root: RegionCell? { cells["root"] }

    public func cell(_ key: String) -> RegionCell? { cells[key] }

    /// Opens the application for this appearance; a second call changes nothing.
    public func start(dark: Bool) {
        shared.start(arguments: arguments, dark: dark)
    }

    /// A control event: the control's ordinal in the frame it was drawn from, and its value.
    public func send(region: String, revision: Int, ordinal: Int, value: Tree) {
        shared.send(Touch.control(Int64(ordinal), value.words))
    }

    /// Keeps the regions of a frame: a region whose tree did not change keeps its cell untouched.
    private func adopt(_ shown: Shown) {
        guard let frame = Told.frame(shown) else {
            told = shown.controls.isEmpty
            return
        }
        told = true
        if frame.theme != theme { theme = frame.theme }
        var next = cells
        for region in frame.regions {
            if let cell = next[region.key] {
                if cell.node != region.node { cell.replace(revision: cell.revision + 1, node: region.node) }
            } else {
                next[region.key] = RegionCell(key: region.key, revision: 0, node: region.node)
            }
        }
        let live = Set(frame.regions.map(\.key))
        next = next.filter { live.contains($0.key) }
        if next.count != cells.count || Set(next.keys) != Set(cells.keys) { cells = next }
    }
}

/// The standard adapters this host answers ([GPUI-HOST-CAPABILITIES] names the same kinds).
@MainActor
public enum Adapters {
    public static func standard(store: StoreAdapter) -> [CapabilityAdapter] {
        SharedStore.standardAdapters(store: store)
    }
}
