import Foundation
import OspreyViewApp

// Implements [NATIVE-SHM-VIEW], [NATIVE-SHM-EVENT] and [NATIVE-SHM-ASK] for the host's model:
// it opens the bridge, calls the application's exports with typed events, keeps the checked
// frame they drew and answers what that frame asks — slice writes land in the host's slice
// store, diagnostics show in the host's strip, and requests go to capability adapters whose
// completions return as events. All of it runs on the UI thread.

/// The host's model: one checked frame and the host events its controls deliver.
@MainActor
public final class SharedStore: ObservableObject {
    /// Bumped whenever a frame is adopted, so the views read the rows again.
    @Published public private(set) var revision = 0
    /// The checked frame the application last drew; empty until the first good one.
    @Published public private(set) var shown = Shown.empty
    /// The host's own diagnostics, in encounter order: refused frames and unknown capabilities.
    @Published public private(set) var diagnostics: [String] = []
    /// The first failure that ends the session, or nil while every call answered.
    @Published public private(set) var failure: String?
    /// Whether an `animate` ask has turned the frame clock on.
    public private(set) var animating = false

    private var bridge: SharedBridge?
    private var seen: Set<String> = []
    private let runner: CapabilityRunner
    private let slices: SliceStore
    private let clock = SharedClock()

    public init(adapters: [CapabilityAdapter], slices: SliceStore = .shared) {
        runner = CapabilityRunner(adapters: adapters, slices: slices)
        self.slices = slices
        clock.onTick = { [weak self] elapsed in self?.send(Touch.frame(elapsed)) }
    }

    /// The adapters this host answers ([GPUI-HOST-CAPABILITIES] names the same kinds).
    public static func standardAdapters(store: StoreAdapter) -> [CapabilityAdapter] {
        #if os(macOS)
        [RunAdapter(), ProcessAdapter(), HttpAdapter(), store, DocumentAdapter(),
         ListDirectoriesAdapter(), ReadPropertyListsAdapter(), ReadTextsAdapter(), FingerprintAdapter(), RecogniseAdapter(),
         MeasureAdapter()]
        #else
        [HttpAdapter(), store, DocumentAdapter(), ListDirectoriesAdapter(), ReadTextsAdapter(), MeasureAdapter()]
        #endif
    }

    /// How many controls the held frame has.
    public var count: Int { shown.controls.count }

    /// The control at `index`, read from the checked frame.
    public func control(_ index: Int) -> RowControl? {
        shown.controls.indices.contains(index) ? shown.controls[index] : nil
    }

    /// The indexes of `index`'s children, in drawing order.
    public func kids(_ index: Int) -> [Int] {
        shown.controls.indices.contains(index) ? shown.controls[index].children : []
    }

    /// The scene the canvas at `index` draws, when its frame gave it one.
    public func scene(_ index: Int) -> SharedScene? {
        shown.scenes[index]
    }

    /// The words of the control now at `index`, so a field that edits itself reads the frame
    /// the last call produced rather than the one it was drawn from.
    public func words(at index: Int) -> String {
        control(index)?.words ?? ""
    }

    /// The binding the control now at `index` delivers, or -1 when it has none.
    public func action(at index: Int) -> Int64 {
        control(index)?.action ?? -1
    }

    /// Opens the instance, runs the program, hands over the launch arguments in order, draws
    /// the first frame for this appearance and tells the application it is active.
    public func start(arguments: [String], dark: Bool) {
        guard bridge == nil, failure == nil else { return }
        do {
            let opened = try SharedBridge()
            bridge = opened
            try opened.main()
            for argument in arguments { try opened.argument(argument) }
            adopt(try opened.open(now: moment, offset: Int64(TimeZone.current.secondsFromGMT()),
                                  platform: platform, home: home, dark: dark))
            send(Touch.phase("active"))
        } catch {
            fail("Osprey could not start: \(error)")
        }
    }

    /// Delivers one host event and keeps the frame it drew. A frame that fails its checks is
    /// refused whole and the last good view stays, with the reason on the diagnostic strip.
    public func send(_ touch: Touch) {
        guard let bridge, failure == nil else { return }
        do {
            adopt(try bridge.event(now: moment, touch: touch))
        } catch let refusal as SharedRefusal {
            diagnose(refusal.why)
        } catch {
            fail("Osprey could not handle an event: \(error)")
        }
    }

    /// The system appearance changed; the application redraws for it.
    public func appear(dark: Bool) {
        send(Touch.appearance(dark))
    }

    /// Adds a host diagnostic, in encounter order.
    public func diagnose(_ message: String) {
        diagnostics.append(message)
    }

    /// Adds a recurring diagnostic only on its first encounter, so a redraw cannot grow it.
    public func diagnoseOnce(_ message: String) {
        guard !seen.contains(message) else { return }
        seen.insert(message)
        diagnose(message)
    }

    /// Applies a checked frame: its slice writes, its diagnostics, its view and its requests.
    private func adopt(_ next: Shown) {
        // Slice writes land first, so a request that names one finds the text it wrote.
        next.slices.forEach { slices.write($0.key, $0.text) }
        diagnostics.append(contentsOf: next.diagnostics)
        shown = next
        revision += 1
        answer(next.requests)
    }

    /// Answers every capability the latest frame requested; an unknown kind is diagnosed and
    /// still completes, with an error naming it.
    private func answer(_ asks: [Ask]) {
        asks.forEach { ask in
            switch ask.kind {
            case "animate":
                animating = ask.flag(1)
                clock.setRunning(animating)
                send(Touch.completed(ask.ticket, result: ""))
            case let kind where runner.knows(kind):
                runner.run(ask) { [weak self] outcome in
                    switch outcome {
                    case let .success(value):
                        self?.send(Touch.completed(ask.ticket, result: value.text))
                    case let .failure(problem):
                        Log.events.error("Request \(ask.ticket, privacy: .public) failed: \(problem.description, privacy: .private)")
                        self?.send(Touch.completed(ask.ticket, failed: problem.description))
                    }
                }
            case let kind:
                let problem = "Unknown capability \(kind)"
                diagnose("\(problem) (ticket \(ask.ticket))")
                send(Touch.completed(ask.ticket, failed: problem))
            }
        }
    }

    private func fail(_ problem: String) {
        failure = problem
        bridge = nil
    }

    /// Unix seconds for an event or launch context.
    private var moment: Int64 { Int64(Date().timeIntervalSince1970) }

    /// This platform's name and the person's home folder, as the launch context carries them.
    private var platform: String {
        #if os(macOS)
        return "macos"
        #elseif os(iOS)
        return "ios"
        #else
        return "unknown"
        #endif
    }

    private var home: String {
        #if os(iOS)
        // The launch context's home is the app's Documents folder, the one folder a person sees.
        return FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first?.path ?? NSHomeDirectory()
        #else
        return NSHomeDirectory()
        #endif
    }
}
