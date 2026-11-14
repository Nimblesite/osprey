#if os(macOS)
import AppKit
import SwiftUI

/// Implements [NATIVE-SHM-VIEW]'s on-screen half of the view check: the frames the store adopts
/// must reach the views a person sees. The real `SharedView` is hosted in a window nobody can see
/// or reach, the taps go through the store, and the window is drawn to a picture — so the check
/// judges what SwiftUI shows, not what the store holds.
@MainActor
enum SelfCheckWindow {
    static let size = NSSize(width: 640, height: 560)

    /// Pictures of one window before and after `times` taps of the control reading `press`, and of
    /// a second window opened only once the same taps were made.
    static func pictures(press: String, times: Int) throws -> (before: Data, after: Data, fresh: Data) {
        let live = try opened()
        let before = try picture(live.window)
        try tap(live.store, press: press, times: times)
        let after = try picture(live.window)
        let tapped = SharedStore(adapters: [])
        tapped.start(arguments: [], dark: false)
        try tap(tapped, press: press, times: times)
        let second = try hosted(tapped)
        let fresh = try picture(second)
        live.window.close()
        second.close()
        return (before, after, fresh)
    }

    private static func opened() throws -> (store: SharedStore, window: NSWindow) {
        let store = SharedStore(adapters: [])
        store.start(arguments: [], dark: false)
        if let failure = store.failure { throw SelfCheck.Problem(why: failure) }
        return (store, try hosted(store))
    }

    private static func tap(_ store: SharedStore, press: String, times: Int) throws {
        guard let index = (0..<store.count).first(where: { store.words(at: $0) == press }) else {
            throw SelfCheck.Problem(why: "no control reads \"\(press)\"")
        }
        for _ in 0..<times { store.send(Touch.control(store.action(at: index), "")) }
    }

    /// SwiftUI stops updating a window the system sees as hidden, so this is a real window that is
    /// fully transparent and takes no clicks.
    private static func hosted(_ store: SharedStore) throws -> NSWindow {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: 40, y: 40), size: size),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.alphaValue = 0
        window.ignoresMouseEvents = true
        window.appearance = NSAppearance(named: .aqua)
        let content = NSHostingView(rootView: SharedView(store: store))
        content.sizingOptions = []
        content.frame = NSRect(origin: .zero, size: size)
        window.contentView = content
        window.orderFrontRegardless()
        return window
    }

    /// The first picture no different from the one 100 ms before, so a pending update has landed.
    private static func picture(_ window: NSWindow) throws -> Data {
        var earlier = try drawn(window)
        for _ in 0..<30 {
            RunLoop.main.run(until: Date().addingTimeInterval(0.1))
            let later = try drawn(window)
            if let moved = SelfTestPicture.difference(earlier, later), moved == 0 { return later }
            earlier = later
        }
        throw SelfCheck.Problem(why: "the window never came to rest")
    }

    private static func drawn(_ window: NSWindow) throws -> Data {
        guard let view = window.contentView,
              let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else {
            throw SelfCheck.Problem(why: "the window cannot be drawn to a picture")
        }
        view.layoutSubtreeIfNeeded()
        view.cacheDisplay(in: view.bounds, to: bitmap)
        guard let data = bitmap.representation(using: .png, properties: [:]) else {
            throw SelfCheck.Problem(why: "the picture cannot be written")
        }
        return data
    }
}
#endif
