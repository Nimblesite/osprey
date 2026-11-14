import Foundation

/// Implements [NATIVE-SHM-VIEW]'s headless check: Tally's journey, played exactly as the C
/// reference host plays it, with every fact read back out of the checked frame the application
/// drew — its controls, its ask rows, its scene rows and a refused event's frame. No window, no
/// copy, no JSON. Both hosts run it: macOS's `--view-check` and iOS's view check.
@MainActor
public enum SelfCheck {
    /// One fact that did not hold, in the words a person reads.
    struct Problem: Error {
        let why: String
    }

    /// Any application's launch, with no window: it opens, draws a checked frame with controls and
    /// says nothing is wrong. Its own asks stay unanswered, as no adapter is installed.
    public static func opened() -> Int32 {
        let store = SharedStore(adapters: [])
        store.start(arguments: [], dark: false)
        let problem = store.failure ?? (store.count == 0 ? "the first frame drew no controls" : nil)
        if let problem {
            print("FAILED: \(problem)")
            return 1
        }
        print("PASSED: \(store.count) controls")
        return 0
    }

    public static func run() -> Int32 {
        let store = SharedStore(adapters: [])
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

    /// The first view's controls, in the order Tally draws them.
    private static let kinds: [SharedKind] = [.column, .label, .label, .line, .press,
                                              .press, .press, .entry, .gap, .press]
    private static let parents: [Int64] = [-1, 0, 0, 0, 3, 3, 3, 0, 0, 0]
    private static let actions: [Int64] = [-1, -1, -1, -1, 0, 1, 2, 3, -1, 4]

    private static func checked(_ store: SharedStore) throws {
        store.start(arguments: [], dark: false)
        if let failure = store.failure { throw Problem(why: failure) }
        // One frame for `open`, one for the `active` phase that follows it.
        try expect(store.revision == 2, "the launch leased \(store.revision) views, not two")
        try structure(store)
        try look(store)
        try journey(store)
        try asksAndScenes(store)
        try refusal(store)
        if let failure = store.failure { throw Problem(why: failure) }
        #if os(macOS)
        try onScreen()
        #endif
    }

    #if os(macOS)
    /// What a person sees follows the frames: after three taps of Add one the window looks exactly
    /// like a window opened at three, and not like the one it was before.
    private static func onScreen() throws {
        let shown = try SelfCheckWindow.pictures(press: "Add one", times: 3)
        let moved = SelfTestPicture.difference(shown.before, shown.fresh) ?? 0
        try expect(moved > 0, "a window at three looks the same as a window at zero")
        let stale = SelfTestPicture.difference(shown.after, shown.fresh) ?? 1
        try expect(stale == 0, "after three taps \(String(format: "%.2f", stale * 100))% of the window still differs from a window opened at three")
    }
    #endif

    /// Nothing but Tally's own controls: their kinds, the container each one sits in and the
    /// control each one delivers.
    private static func structure(_ store: SharedStore) throws {
        try expect(store.count == kinds.count, "the first view has \(store.count) controls, not \(kinds.count)")
        for index in kinds.indices {
            guard let control = store.control(index) else { throw Problem(why: "row \(index) is missing") }
            try expect(control.kind == kinds[index], "row \(index) is kind \(control.kind), not \(kinds[index])")
            try expect(control.parent == parents[index], "row \(index) sits in \(control.parent), not \(parents[index])")
            try expect(control.action ?? -1 == actions[index], "row \(index) delivers \(control.action.map(String.init) ?? "nothing"), not \(actions[index])")
        }
    }

    /// The words and the colours of the first view, straight from the checked controls.
    private static func look(_ store: SharedStore) throws {
        try words(store, 1, "Tally")
        try words(store, 2, "0")
        try words(store, 5, "Add one")
        try words(store, 9, "Use night colours")
        guard let page = store.control(0), let figure = store.control(2), let add = store.control(5) else {
            throw Problem(why: "the first view has fewer than ten controls")
        }
        try expect(page.look.fill == 0xFB_FA_F7_FF, "the page is not paper")
        try expect(page.look.padding == 32 && page.look.spacing == 18, "the page is not inset 32 with 18-point gaps")
        try expect(figure.look.ink == 0x0B_6E_8A_FF, "the tally is not in the accent colour")
        try expect(figure.look.size == 72 && figure.look.weight == 800, "the tally is not 72 points at weight 800")
        try expect(add.look.fill == 0x0B_6E_8A_FF && add.look.radius == 10, "Add one is not the accent button")
    }

    /// Three taps, a rename and a restyle, each read back from the shared frame.
    private static func journey(_ store: SharedStore) throws {
        let add = try control(store, words: "Add one")
        for _ in 0..<3 { store.send(Touch.control(add.action ?? -1, "")) }
        try words(store, 2, "3")
        let entry = try control(store, kind: .entry)
        store.send(Touch.control(entry.action ?? -1, "Zoë"))
        try words(store, 1, "Zoë's tally")
        try words(store, 7, "Zoë")
        let restyle = try last(store, kind: .press)
        store.send(Touch.control(restyle.action ?? -1, ""))
        try expect(store.count == kinds.count, "the restyled view lost controls")
        try words(store, 2, "3")
        try words(store, 9, "Use day colours")
        guard let page = store.control(0), let figure = store.control(2) else {
            throw Problem(why: "the restyled view has fewer than three controls")
        }
        try expect(page.look.fill == 0x10_14_1F_FF, "the night page is not dark")
        try expect(figure.look.ink == 0x7A_D7_F0_FF, "the night tally is not the night accent")
    }

    /// Tally asks for nothing: no capability requests, no slice writes, no diagnostics, and no
    /// canvas rows, so its frames' ask and scene runs read back empty.
    private static func asksAndScenes(_ store: SharedStore) throws {
        let shown = store.shown
        try expect(shown.requests.isEmpty, "Tally asked for a capability it never asks for")
        try expect(shown.slices.isEmpty, "Tally wrote a host slice it never writes")
        try expect(shown.diagnostics.isEmpty, "Tally refused an event it takes")
        let canvases = shown.controls.enumerated().filter { $0.element.kind == .canvas }
        try expect(canvases.isEmpty, "Tally drew a canvas it never draws")
        try expect(shown.scenes.isEmpty, "a scene named a canvas Tally never drew")
    }

    /// An event that names no drawn control is refused — but it still drew a whole frame, which
    /// the host committed and now shows: the same view, adopted again, with nothing changed.
    private static func refusal(_ store: SharedStore) throws {
        let before = (revision: store.revision, words: store.words(at: 9), count: store.count)
        store.send(Touch.control(99, ""))
        store.send(Touch.control(-1, "x"))
        try expect(store.revision == before.revision + 2, "a refused event drew no frame")
        try expect(store.count == before.count, "a refused event changed the view's controls")
        try expect(store.words(at: 9) == before.words, "a refused event changed the words")
        try asksAndScenes(store)
        try expect(store.diagnostics.isEmpty, "a refused event left a diagnostic Tally never sends")
    }

    /// The control that reads `words`, so the check taps what a person reads.
    private static func control(_ store: SharedStore, words expected: String) throws -> RowControl {
        for index in 0..<store.count where store.words(at: index) == expected {
            if let control = store.control(index) { return control }
        }
        throw Problem(why: "no control reads \"\(expected)\"")
    }

    private static func control(_ store: SharedStore, kind: SharedKind) throws -> RowControl {
        for index in 0..<store.count {
            if let control = store.control(index), control.kind == kind { return control }
        }
        throw Problem(why: "no control is a \(kind)")
    }

    /// The last control of `kind`: a view's last press is its bottom-most one.
    private static func last(_ store: SharedStore, kind: SharedKind) throws -> RowControl {
        for index in (0..<store.count).reversed() {
            if let control = store.control(index), control.kind == kind { return control }
        }
        throw Problem(why: "no control is a \(kind)")
    }

    private static func words(_ store: SharedStore, _ index: Int, _ expected: String) throws {
        guard store.control(index) != nil else { throw Problem(why: "row \(index) is missing") }
        let read = store.words(at: index)
        try expect(read == expected, "row \(index) reads \"\(read)\", not \"\(expected)\"")
    }

    private static func expect(_ fact: @autoclosure () -> Bool, _ why: String) throws {
        guard fact() else { throw Problem(why: why) }
    }
}
