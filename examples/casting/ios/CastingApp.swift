import SwiftUI
import Combine
import QuartzCore

// [CAST-MOBILE-DELIVERY] A native shell around the exact resident casting app.
@_cdecl("native_slice")
func castingSlice(_ key: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    SliceStore.shared.pointer(key.map { String(cString: $0) } ?? "")
}

@_cdecl("native_shape")
func castingShape(_ code: Int64, _ index: Int64, _ a: Double, _ b: Double, _ c: Double,
                  _ d: Double, _ e: Double, _ tint: Int64, _ layer: Int64) -> Int64 {
    guard [a, b, c, d, e].allSatisfy(\.isFinite), [0, 1, 3].contains(code) else { return -1 }
    let values: [Double] = code == 1 ? [1, a, b, c, Double(tint), Double(layer)] :
        code == 3 ? [3, Double(index), a, b, c, d, e, Double(tint), Double(layer)] :
        [0, a, b, c, d, e, Double(tint), Double(layer)]
    MainActor.assumeIsolated { SceneStream.shared.append(.array(values.map(JSONValue.number))) }
    return 0
}

func residentStart(_ context: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    MainActor.assumeIsolated { SceneStream.shared.begin() }
    let result = osprey_app_start(context)
    if CommandLine.arguments.contains("--capture-envelope"), let result {
        let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("start-envelope.json")
        do { try String(cString: result).write(to: url, atomically: true, encoding: .utf8) }
        catch { NSLog("Cannot capture test envelope: %@", error.localizedDescription) }
    }
    MainActor.assumeIsolated { SceneStream.shared.commit() }
    return result
}

func residentDispatch(_ state: UnsafePointer<CChar>?, _ event: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    MainActor.assumeIsolated { SceneStream.shared.begin() }
    let result = osprey_app_dispatch(state, event)
    MainActor.assumeIsolated { SceneStream.shared.commit() }
    return result
}

private struct CastBrowseAdapter: CapabilityAdapter {
    let kinds = ["run"]
    let discovery: CastDiscovery

    func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        guard request.ticket == "discover", try request.text("slice") == "cast-browse" else {
            throw CapabilityError.notAllowed("Mobile casting does not execute processes")
        }
        #if DEBUG
        if let encoded = ProcessInfo.processInfo.environment["CAST_TEST_RECEIVER"] {
            guard let data = Data(base64Encoded: encoded), data.count <= 65536,
                  let text = String(data: data, encoding: .utf8) else { throw CapabilityError.invalidArgument("Invalid receiver fixture") }
            slices.write("cast-browse", text)
            return .object([:])
        }
        #endif
        let transcript: String = try await withCheckedThrowingContinuation { continuation in
            Task { @MainActor in discovery.start { continuation.resume(with: $0) } }
        }
        slices.write("cast-browse", transcript)
        return .object([:])
    }
}

private struct FrameAdapter: CapabilityAdapter {
    let kinds = ["animate"]
    func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue { .object([:]) }
}

@MainActor
private func castingStore() -> RegionStore {
    let entry = LibraryEntry(main: osprey_main, start: residentStart, dispatch: residentDispatch,
                             releaseString: osprey_release_string, hostABIVersion: osprey_host_abi_version)
    let folder = Bundle.main.resourceURL?.path ?? ""
    var arguments = ["--machine", folder + "/player.machine.json"] + Array(CommandLine.arguments.dropFirst())
    // [CAST-MOBILE-SKIN-PERSISTENCE] Explicit launch arguments override local settings.
    #if DEBUG
    if let saved = ProcessInfo.processInfo.environment["CAST_TEST_SAVED_SKIN"] {
        UserDefaults.standard.set(saved, forKey: "casting.selected-skin")
    }
    #endif
    if !arguments.contains("--skin"), let saved = UserDefaults.standard.string(forKey: "casting.selected-skin") {
        arguments += ["--skin", saved]
    }
    #if DEBUG
    // Xcode may move the data container when installing the test build.
    // Resolve the fixture inside this process, after installation has finished.
    if ProcessInfo.processInfo.environment["CAST_TEST_AUDIO"] == "Casting mobile test.mp3" {
        let audio = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("Casting mobile test.mp3")
        arguments += ["--open", audio.path]
    }
    #endif
    let context = HostContext(platform: "ios", home: NSHomeDirectory(), arguments: arguments)
    let adapters: [CapabilityAdapter] = [CastBrowseAdapter(discovery: CastDiscovery()), DocumentAdapter(), FrameAdapter()]
    return RegionStore(library: Library(entry: entry), context: context, capabilities: CapabilityRunner(adapters: adapters),
                       onSliceWrite: { slice in
        if slice.key == "casting.selected-skin" { UserDefaults.standard.set(slice.text, forKey: slice.key) }
        if slice.key == "casting.appearance-warning" {
            NotificationCenter.default.post(name: Notification.Name(slice.key), object: slice.text)
        }
    })
}

@main
struct CastingHost: App {
    @StateObject private var store = castingStore()
    private let skins = Result { try castingSkins() }
    @Environment(\.scenePhase) private var phase
    @State private var previousFrame = 0.0
    @State private var appearanceWarning = ""
    private let frames = Timer.publish(every: 1.0 / 30, on: .main, in: .common).autoconnect()

    var body: some Scene {
        WindowGroup {
            content
                .onReceive(NotificationCenter.default.publisher(for: Notification.Name("casting.appearance-warning"))) { note in
                    if let message = note.object as? String { appearanceWarning = message }
                }
                .alert("Appearance restored", isPresented: Binding(get: { !appearanceWarning.isEmpty }, set: { if !$0 { appearanceWarning = "" } })) {
                    Button("OK") { appearanceWarning = "" }
                } message: { Text(appearanceWarning) }
                .onReceive(frames) { _ in
                    let now = CACurrentMediaTime()
                    if phase == .active, previousFrame > 0 { store.frame(elapsed: min(0.25, max(0, now - previousFrame))) }
                    previousFrame = now
                }
                .onChange(of: phase) { _, next in store.lifecycle(next == .active ? "active" : "inactive") }
        }
    }

    @ViewBuilder private var content: some View {
        switch skins {
        case let .success(choices):
            RootView(store: store).frame(maxWidth: .infinity, maxHeight: .infinity).overlay(alignment: .topTrailing) {
                SkinMenu(choices: choices) { store.lifecycle("skin:" + $0) }.equatable().padding(12)
            }
        case let .failure(error):
            ContentUnavailableView("Cannot load player appearances", systemImage: "exclamationmark.triangle", description: Text(error.localizedDescription))
        }
    }
}

// [CAST-MOBILE-SKINS] The menu uses the bundled machine's choices on every host.
private func castingSkins() throws -> [JSONValue] {
    guard let url = Bundle.main.url(forResource: "player.machine", withExtension: "json") else {
        throw CapabilityError.invalidArgument("The player machine is missing")
    }
    let doc = try JSONDecoder().decode(JSONValue.self, from: Data(contentsOf: url))
    guard let choices = doc.object?["inputs"]?.object?["skins"]?.object?["value"]?.array, !choices.isEmpty,
          choices.allSatisfy({ !($0.object?["key"]?.string ?? "").isEmpty && !($0.object?["label"]?.string ?? "").isEmpty }) else {
        throw CapabilityError.invalidArgument("The player has no valid appearance choices")
    }
    return choices
}

private struct SkinMenu: View, Equatable {
    let choices: [JSONValue]
    let choose: (String) -> Void
    static func == (left: Self, right: Self) -> Bool { left.choices == right.choices }
    var body: some View {
        Menu {
            ForEach(Array(choices.enumerated()), id: \.offset) { _, choice in
                Button(choice.object?["label"]?.string ?? "") { choose(choice.object?["key"]?.string ?? "") }
            }
        } label: { Label("Appearance", systemImage: "paintpalette").frame(minWidth: 44, minHeight: 44) }
            .buttonStyle(.bordered).accessibilityLabel("Appearance")
    }
}
