import Foundation
import OspreyShared
import OspreyViewApp

/// Implements [NATIVE-SHM-VIEW]'s headless check: Tally's journey, played
/// exactly as the C reference host plays it, with every fact read back out of
/// the rows the application wrote. No window, no copy, no JSON.
enum SelfCheck {
    /// One fact that did not hold, in the words a person reads.
    struct Problem: Error {
        let why: String
    }

    static func run() -> Int32 {
        let store = SharedStore()
        do {
            try checked(store)
        } catch let problem as Problem {
            print("FAILED: \(problem.why)")
            return 1
        } catch {
            print("FAILED: \(error)")
            return 1
        }
        print("PASSED")
        return 0
    }

    /// The first view's rows, in the order Tally draws them.
    private static let kinds: [Int64] = [SharedKind.column, .label, .label, .line, .press,
                                         .press, .press, .entry, .gap, .press].map(\.rawValue)
    private static let parents: [Int64] = [-1, 0, 0, 0, 3, 3, 3, 0, 0, 0]
    private static let actions: [Int64] = [-1, -1, -1, -1, 0, 1, 2, 3, -1, 4]

    private static func checked(_ store: SharedStore) throws {
        store.start()
        if let failure = store.failure { throw Problem(why: failure) }
        try expect(store.revision == 1, "the launch leased \(store.revision) views, not one")
        try structure(store)
        try look(store)
        try journey(store)
        try refusal(store)
        if let failure = store.failure { throw Problem(why: failure) }
    }

    /// Nothing but Tally's own rows: its kinds, the container each one sits in
    /// and the control each one delivers.
    private static func structure(_ store: SharedStore) throws {
        try expect(store.count == kinds.count, "the first view has \(store.count) controls, not \(kinds.count)")
        for index in kinds.indices {
            guard let row = store.row(index) else { throw Problem(why: "row \(index) is missing") }
            try expect(row.kind == kinds[index], "row \(index) is kind \(row.kind), not \(kinds[index])")
            try expect(row.parent == parents[index], "row \(index) sits in \(row.parent), not \(parents[index])")
            try expect(row.action == actions[index], "row \(index) delivers \(row.action), not \(actions[index])")
        }
    }

    /// The words and the colours of the first view, straight from the rows.
    private static func look(_ store: SharedStore) throws {
        try words(store, 1, "Tally")
        try words(store, 2, "0")
        try words(store, 5, "Add one")
        try words(store, 9, "Use night colours")
        guard let page = store.row(0), let figure = store.row(2), let add = store.row(5) else {
            throw Problem(why: "the first view has fewer than ten rows")
        }
        try expect(page.background == 0xFB_FA_F7_FF, "the page is not paper")
        try expect(page.padding == 32 && page.spacing == 18, "the page is not inset 32 with 18-point gaps")
        try expect(figure.colour == 0x0B_6E_8A_FF, "the tally is not in the accent colour")
        try expect(figure.size == 72 && figure.weight == 800, "the tally is not 72 points at weight 800")
        try expect(add.background == 0x0B_6E_8A_FF && add.radius == 10, "Add one is not the accent button")
    }

    /// Three taps, a rename and a restyle, each read back from the shared frame.
    private static func journey(_ store: SharedStore) throws {
        let add = try control(store, words: "Add one")
        for _ in 0..<3 { store.event(action: add.action, text: "") }
        try words(store, 2, "3")
        let entry = try control(store, kind: .entry)
        store.event(action: entry.action, text: "Zoë")
        try words(store, 1, "Zoë's tally")
        try words(store, 7, "Zoë")
        let restyle = try last(store, kind: .press)
        store.event(action: restyle.action, text: "")
        try expect(store.count == kinds.count, "the restyled view lost controls")
        try words(store, 2, "3")
        try words(store, 9, "Use day colours")
        guard let page = store.row(0), let figure = store.row(2) else {
            throw Problem(why: "the restyled view has fewer than three rows")
        }
        try expect(page.background == 0x10_14_1F_FF, "the night page is not dark")
        try expect(figure.colour == 0x7A_D7_F0_FF, "the night tally is not the night accent")
    }

    /// An event that names no drawn control changes nothing: the call's frame is
    /// dropped and the view already shown stays valid and unchanged.
    private static func refusal(_ store: SharedStore) throws {
        let before = store.revision
        store.event(action: 99, text: "")
        store.event(action: -1, text: "x")
        try expect(store.revision == before, "a refused event leased a view")
        try words(store, 9, "Use day colours")
    }

    /// The control that reads `words`, so the check taps what a person reads.
    private static func control(_ store: SharedStore, words expected: String) throws -> OspNativeView {
        for index in 0..<store.count {
            if let row = store.row(index), store.words(row) == expected { return row }
        }
        throw Problem(why: "no control reads \"\(expected)\"")
    }

    private static func control(_ store: SharedStore, kind: SharedKind) throws -> OspNativeView {
        for index in 0..<store.count {
            if let row = store.row(index), row.kind == kind.rawValue { return row }
        }
        throw Problem(why: "no control is a \(kind)")
    }

    /// The last row of `kind`: a view's last press is its bottom-most one.
    private static func last(_ store: SharedStore, kind: SharedKind) throws -> OspNativeView {
        for index in (0..<store.count).reversed() {
            if let row = store.row(index), row.kind == kind.rawValue { return row }
        }
        throw Problem(why: "no control is a \(kind)")
    }

    private static func words(_ store: SharedStore, _ index: Int, _ expected: String) throws {
        guard let row = store.row(index) else { throw Problem(why: "row \(index) is missing") }
        let read = store.words(row)
        try expect(read == expected, "row \(index) reads \"\(read)\", not \"\(expected)\"")
    }

    private static func expect(_ fact: @autoclosure () -> Bool, _ why: String) throws {
        guard fact() else { throw Problem(why: why) }
    }
}