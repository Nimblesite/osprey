import SwiftUI
import Combine

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
    let context = HostContext(platform: "ios", home: NSHomeDirectory(), arguments: ["--machine", folder + "/player.machine.json"])
    let adapters: [CapabilityAdapter] = [CastBrowseAdapter(discovery: CastDiscovery()), DocumentAdapter(), FrameAdapter()]
    return RegionStore(library: Library(entry: entry), context: context, capabilities: CapabilityRunner(adapters: adapters))
}

@main
struct CastingHost: App {
    @StateObject private var store = castingStore()
    @Environment(\.scenePhase) private var phase
    private let frames = Timer.publish(every: 1.0 / 30, on: .main, in: .common).autoconnect()

    var body: some Scene {
        WindowGroup {
            RootView(store: store)
                .onReceive(frames) { _ in if phase == .active { store.frame(elapsed: 1.0 / 30) } }
                .onChange(of: phase) { _, next in store.lifecycle(next == .active ? "active" : "inactive") }
        }
    }
}
