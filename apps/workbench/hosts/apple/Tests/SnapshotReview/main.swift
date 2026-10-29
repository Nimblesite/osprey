import AppKit
import OspreyNative
import SwiftUI

// Replays actual Osprey-authored regions captured by the browser E2E suite.
// This is a renderer comparison at an exact viewport, not an application host.
private enum ReviewInput {
    static let text = String(data: FileHandle.standardInput.readDataToEndOfFile(), encoding: .utf8).map { $0 as NSString }
}

private enum ReviewError: Error {
    case arguments, invalidInput, bitmap, unexpectedEvent(String)
}

private func initialized() -> Int32 { 0 }
private func started(_ context: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { ReviewInput.text?.utf8String }
private func dispatched(_ saved: UnsafePointer<CChar>?, _ event: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? { nil }

@main
private enum SnapshotReview {
    @MainActor static func main() throws {
        let args = CommandLine.arguments
        guard args.count == 4, let width = Int(args[1]), let height = Int(args[2]), width > 0, height > 0 else { throw ReviewError.arguments }
        guard ReviewInput.text != nil else { throw ReviewError.invalidInput }
        _ = NSApplication.shared
        try FontRegistration.registerBundled()
        let library = Library(entry: LibraryEntry(main: initialized, start: started, dispatch: dispatched))
        let store = RegionStore(library: library, context: HostContext(platform: "render-review", home: ""), capabilities: CapabilityRunner(adapters: []))
        store.start()
        if ProcessInfo.processInfo.environment["OSPREY_RENDER_REVIEW_WINDOW"] == "1" { show(store, width: width, height: height) }
        else { try capture(store, width: width, height: height, path: args[3]) }
    }

    @MainActor private static func capture(_ store: RegionStore, width: Int, height: Int, path: String) throws {
        let (host, window) = hosted(store, width: width, height: height)
        defer { window.close() }
        host.setFrameSize(NSSize(width: width, height: height))
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.25))
        FileHandle.standardError.write(Data("Requested \(width)x\(height); native bounds \(host.bounds.size); fitting \(host.fittingSize)\n".utf8))
        guard store.failure == nil, store.diagnostics.isEmpty else { throw ReviewError.unexpectedEvent(store.failure ?? store.diagnostics.joined(separator: "; ")) }
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32) else { throw ReviewError.bitmap }
        bitmap.size = NSSize(width: width, height: height)
        host.cacheDisplay(in: host.bounds, to: bitmap)
        guard let data = bitmap.representation(using: .png, properties: [:]) else { throw ReviewError.bitmap }
        try data.write(to: URL(fileURLWithPath: path))
    }

    @MainActor private static func hosted(_ store: RegionStore, width: Int, height: Int) -> (NSHostingView<RootView>, NSWindow) {
        let host = NSHostingView(rootView: RootView(store: store))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        return (host, window)
    }

    @MainActor private static func show(_ store: RegionStore, width: Int, height: Int) {
        let (host, window) = hosted(store, width: width, height: height)
        host.setFrameSize(NSSize(width: width, height: height))
        window.orderFrontRegardless()
        NSApplication.shared.run()
    }
}
