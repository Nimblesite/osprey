import AppKit
import SwiftUI
import XCTest
@testable import OspreyNative

enum TestResources {
    static let apple = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    static let assets = apple.appendingPathComponent("../../assets").standardizedFileURL

    static func frame(_ values: [String: Any]) throws -> FrameStyle {
        FontRegistration.configureTextRendering()
        let data = try JSONSerialization.data(withJSONObject: frameJSON(values))
        return try JSONDecoder().decode(FrameStyle.self, from: data)
    }

    static func frameJSON(_ values: [String: Any]) -> [String: Any] {
        let defaults: [String: Any] = ["name": "", "width": 0, "height": 0, "top": 0, "right": 0, "bottom": 0, "left": 0, "radius": 0, "gap": 0, "fill": "", "ink": "", "line": "", "grow": false, "font": "", "textSize": 0]
        return defaults.merging(values) { _, value in value }
    }

    @MainActor
    static func hosted<Content: View>(_ view: Content, width: CGFloat, height: CGFloat, keyboard: Bool = false, suppliedWindow: NSWindow? = nil) -> (NSHostingView<Content>, NSWindow) {
        FontRegistration.configureTextRendering()
        let host = NSHostingView(rootView: view)
        let window = suppliedWindow ?? NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height), styleMask: keyboard ? .titled : .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        if keyboard { window.makeKeyAndOrderFront(nil) }
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        return (host, window)
    }

    @MainActor
    static func assertInspectableEnd(_ scroll: NSScrollView) throws {
        XCTAssertGreaterThan(try XCTUnwrap(scroll.documentView).bounds.width, scroll.contentView.bounds.width)
        XCTAssertTrue(scroll.hasHorizontalScroller)
        let end = try XCTUnwrap(scroll.documentView).bounds.maxX
        scroll.contentView.scroll(to: NSPoint(x: end, y: 0))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertGreaterThan(scroll.contentView.bounds.minX, 0)
        XCTAssertGreaterThanOrEqual(scroll.contentView.bounds.maxX, end, "The complete right-hand side must be reachable")
    }

    @MainActor
    static func raster(_ host: NSView) throws -> NSBitmapImageRep {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(host.bounds.width), pixelsHigh: Int(host.bounds.height),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
        bitmap.size = host.bounds.size
        host.cacheDisplay(in: host.bounds, to: bitmap)
        return bitmap
    }

    @MainActor
    static func scrollViews(_ view: NSView) -> [NSScrollView] {
        (view as? NSScrollView).map { [$0] } ?? view.subviews.flatMap(scrollViews)
    }
}
