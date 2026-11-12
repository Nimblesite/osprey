import Foundation
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW] for the host's model: it opens the bridge, calls
// the application's exports, keeps the view they drew and tells the views to
// read the rows again. All of it runs on the UI thread.

/// The kinds of control a row can be ([NATIVE-SHM-VIEW]); a kind not listed
/// here is not a control and draws nothing.
public enum SharedKind: Int64 {
    case column = 0, line, label, press, entry, gap, scroll, layer, toggle
    case picker, option, rule, image, canvas, editor, menu, sheet
}

/// The `flags` bits a row may carry ([NATIVE-SHM-VIEW]).
public enum SharedFlag {
    public static let disabled: Int64 = 1
    public static let chosen: Int64 = 2
    public static let secure: Int64 = 4
    public static let fixedWidth: Int64 = 8
    public static let slanted: Int64 = 16
    public static let centred: Int64 = 32
    public static let onCommit: Int64 = 64
}

/// The host's model: one leased view and the events its controls deliver.
public final class SharedStore: ObservableObject {
    /// Bumped whenever a view is leased, so the views read the rows again.
    @Published public private(set) var revision = 0
    /// The first failure of this launch, or nil while every call answered zero.
    @Published public private(set) var failure: String?

    private var bridge: SharedBridge?
    /// Each row's children by parent index. A row naming a parent that is not
    /// an earlier row is in no container, so it is never drawn.
    private var children: [[Int]] = []

    public init() {}

    /// How many controls the held view has.
    public var count: Int { bridge?.rows.count ?? 0 }

    /// Opens the instance, runs the program once and draws its first view.
    public func start() {
        guard bridge == nil, failure == nil else { return }
        do {
            let opened = try SharedBridge()
            bridge = opened
            let started = osprey_main()
            guard started == 0 else { throw SharedFailure(call: "osprey_main", status: started) }
            if try opened.call("start", { osprey_app_start() }) {
                adopt()
            }
        } catch {
            failure = "\(error)"
            bridge = nil
        }
    }

    /// Delivers one tap or edit with its text and keeps the view it drew.
    public func event(action: Int64, text words: String) {
        guard let bridge, failure == nil else { return }
        do {
            if try bridge.call("event", { words.withCString { osprey_app_event(action, $0) } }) { adopt() }
        } catch {
            failure = "\(error)"
        }
    }

    /// The row at `index`, read in place from the leased run.
    public func row(_ index: Int) -> OspNativeView? {
        guard let bridge, index >= 0, index < bridge.rows.count else { return nil }
        return bridge.rows[index]
    }

    /// The indexes of `index`'s children, in drawing order.
    public func kids(_ index: Int) -> [Int] {
        children.indices.contains(index) ? children[index] : []
    }

    /// The whole text of `row`.
    public func words(_ row: OspNativeView) -> String {
        bridge?.words(offset: row.text_offset, length: row.text_length) ?? ""
    }

    /// The second text of `row`: a field's prompt or an option's key.
    public func hint(_ row: OspNativeView) -> String {
        bridge?.words(offset: row.hint_offset, length: row.hint_length) ?? ""
    }

    /// The text of the row now at `index`, so a field that edits itself reads
    /// the rows the last call produced rather than the ones it was drawn from.
    public func words(at index: Int) -> String {
        guard let row = row(index) else { return "" }
        return words(row)
    }

    /// The binding the row now at `index` delivers, or -1 when it has none.
    public func action(at index: Int) -> Int64 {
        row(index)?.action ?? -1
    }

    private func adopt() {
        children = SharedStore.indexed(bridge?.rows)
        revision += 1
    }

    private static func indexed(_ rows: UnsafeBufferPointer<OspNativeView>?) -> [[Int]] {
        guard let rows else { return [] }
        var index = Array(repeating: [Int](), count: rows.count)
        for child in rows.indices {
            let parent = rows[child].parent
            guard parent >= 0, Int(parent) < child else { continue }
            index[Int(parent)].append(child)
        }
        return index
    }
}
