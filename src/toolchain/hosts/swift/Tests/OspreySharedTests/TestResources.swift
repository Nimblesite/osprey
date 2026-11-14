import AppKit
import CoreText
import OspreyViewApp
import SwiftUI
import XCTest

@testable import OspreyShared

// The fixtures every restored Workbench test shares: told trees and frames, real windows and
// rasters, and the faces the application bundles. The tests drive the shared renderer exactly
// as the frame hands it trees — no JSON between application and host ([0053-NativeSharedViews]).

// Window activation is an external input; text controls and first-responder changes remain AppKit's.
final class FocusTestWindow: NSWindow {
    var keyState = true
    override var isKeyWindow: Bool { keyState }
}

// A test window keeps the size its test asks for: AppKit would otherwise shrink a titled window
// to the screen, and a hosted runner's small display then cuts every tall layout short.
final class UnclampedTestWindow: NSWindow {
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect { frameRect }
}

/// The repository's folders, resolved from this file's path (usable from any actor).
enum Repo {
    /// The repository's `src` folder: walks up from this file until it finds the checkout.
    static let src: URL = {
        var folder = URL(fileURLWithPath: #filePath)
        for _ in 0..<12 {
            folder.deleteLastPathComponent()
            if FileManager.default.fileExists(atPath: folder.appendingPathComponent("src/osprey/workbench/osprey.toml").path) {
                return folder.appendingPathComponent("src")
            }
        }
        fatalError("cannot locate the repository from \(#filePath)")
    }()

    /// The Workbench application's bundled assets: fonts, symbols, artwork and lessons.
    static let assets = src.appendingPathComponent("osprey/workbench/assets")

    private static let fontLock = NSLock()
    private static var fontHolders: [String: Int] = [:]

    /// Registers bundled faces once per process, however many parallel tests ask for them;
    /// the closing closure releases the faces when the last holder goes.
    @discardableResult
    static func registerFonts(_ names: [String]) throws -> () -> Void {
        fontLock.lock()
        var fresh: [URL] = []
        for name in names {
            fontHolders[name, default: 0] += 1
            if fontHolders[name] == 1 { fresh.append(assets.appendingPathComponent("fonts").appendingPathComponent(name)) }
        }
        fontLock.unlock()
        fresh.forEach { XCTAssertTrue(CTFontManagerRegisterFontsForURL($0 as CFURL, .process, nil), $0.lastPathComponent) }
        return {
            fontLock.lock()
            var gone: [URL] = []
            for name in names where fontHolders[name] != nil {
                fontHolders[name]! -= 1
                if fontHolders[name] == 0 {
                    fontHolders.removeValue(forKey: name)
                    gone.append(assets.appendingPathComponent("fonts").appendingPathComponent(name))
                }
            }
            fontLock.unlock()
            gone.forEach { _ = CTFontManagerUnregisterFontsForURL($0 as CFURL, .process, nil) }
        }
    }
}

/// Every test case renders text as the application does: an application that bundles its faces
/// turns smoothing off on macOS 26 before AppKit first draws text
/// (`FontRegistration.configureTextRendering`), so the first test class to run does it too.
class RenderingTestCase: XCTestCase {
    override class func setUp() {
        super.setUp()
        if #available(macOS 26, *) { UserDefaults.standard.register(defaults: ["AppleFontSmoothing": 0]) }
        FontRegistration.exactLines = true
    }
}

/// Told trees, frames, themes and drawings — pure values, usable from any actor.
enum Fixtures {
    /// A text leaf: words, role and tone.
    static func text(_ words: String, _ role: String = "body", _ tone: String = "primary") -> Tree {
        .object(["kind": .string("text"), "text": .string(words), "role": .string(role), "tone": .string(tone)])
    }

    /// A frame wrapping `body`, carrying `style` ([UI-COMPONENTS-FRAMED]).
    static func framed(_ style: [String: Tree], _ body: Tree) -> Tree {
        .object(["kind": .string("framed"), "style": .object(style), "body": body])
    }

    /// A theme record from `#RRGGBB` pairs, one token per pair.
    static func theme(_ tokens: [(name: String, colour: String)]) -> Tree {
        .object(["tokens": .array(tokens.map { .object(["name": .string($0.name), "light": .string($0.colour), "dark": .string($0.colour)]) }),
                 "spacing": .array([.number(8)]), "feedback": .number(0), "settle": .number(0)])
    }

    /// A resolver for a theme of `#RRGGBB` pairs.
    static func resolver(_ tokens: [(name: String, colour: String)]) -> ThemeResolver {
        ThemeResolver(theme: Theme(theme(tokens)))
    }

    /// A drawing that records nothing, for a tree a test only renders.
    static func drawing(_ region: String = "test", send: @escaping (Int, Tree) -> Void = { _, _ in }) -> Drawing {
        Drawing(region: region, revision: 1, send: send, point: send, act: { _, _ in })
    }

    /// One mathematics glyph or word.
    static func mathSym(_ words: String) -> Tree { .object(["kind": .string("sym"), "text": .string(words)]) }

    /// One number literal.
    static func mathNum(_ words: String) -> Tree { .object(["kind": .string("num"), "text": .string(words)]) }

    /// One binary or punctuation operator.
    static func mathOp(_ symbol: String, _ left: Tree, _ right: Tree) -> Tree {
        .object(["kind": .string("op"), "symbol": .string(symbol), "left": left, "right": right])
    }

    /// A displayed formula: the `math` leaf with its spoken alternative text.
    static func math(_ inner: Tree, spoken: String = "formula") -> Tree {
        .object(["kind": .string("math"), "math": inner, "spoken": .string(spoken)])
    }

    /// An editor node: its text, its UTF-8 caret and its focus epoch.
    static func editor(_ value: String, focusEpoch: Int = 0, label: String = "Formula") -> Tree {
        .object(["kind": .string("editor"), "label": .string(label), "value": .string(value), "inline": .bool(false),
                 "cursor": .number(Double(value.utf8.count)), "focusEpoch": .number(Double(focusEpoch)), "marks": .array([]),
                 "event": .number(0), "placeEvent": .number(1)])
    }

    /// A picker over `choices` (key and label pairs) with `key` selected.
    static func picker(_ choices: [(String, String)], selected: String, event: Int = 0) -> Tree {
        .object(["kind": .string("picker"), "title": .string("Theorem"), "selected": .string(selected),
                 "choices": .array(choices.map { .object(["key": .string($0.0), "label": .string($0.1)]) }), "event": .number(Double(event))])
    }
}

@MainActor
enum TestResources {
    /// The Workbench application's bundled assets: fonts, symbols, artwork and lessons.
    static let assets = Repo.assets

    // MARK: windows and pixels

    /// A view in a real window, laid out and settled.
    static func hosted<Content: View>(_ view: Content, width: CGFloat, height: CGFloat, keyboard: Bool = false, suppliedWindow: NSWindow? = nil) -> (NSHostingView<Content>, NSWindow) {
        let host = NSHostingView(rootView: view)
        let window = suppliedWindow ?? UnclampedTestWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height),
                                               styleMask: keyboard ? .titled : [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        if keyboard { window.makeKeyAndOrderFront(nil) } else { window.orderFrontRegardless() }
        host.layoutSubtreeIfNeeded()
        settle()
        return (host, window)
    }

    /// Lets AppKit settle queued work — focus, sheets and scrolling.
    static func settle(_ seconds: TimeInterval = 0.2) {
        RunLoop.main.run(until: Date(timeIntervalSinceNow: seconds))
    }

    /// The window's pixels, drawn off screen.
    static func raster(_ host: NSView) throws -> NSBitmapImageRep {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(host.bounds.width), pixelsHigh: Int(host.bounds.height),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
        bitmap.size = host.bounds.size
        host.cacheDisplay(in: host.bounds, to: bitmap)
        return bitmap
    }

    /// A view's pixels at its window's backing scale, as the screen shows them.
    static func backingRaster(_ host: NSView) throws -> NSBitmapImageRep {
        let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: bitmap)
        return bitmap
    }

    /// A view's pixels, without a window.
    static func rendered<V: View>(_ view: V) throws -> NSBitmapImageRep {
        NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
    }

    /// A colour at (`x`, `y`) in the sRGB space every raster assertion reads.
    static func pixel(_ bitmap: NSBitmapImageRep, _ x: Int, _ y: Int) throws -> NSColor {
        try XCTUnwrap(bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB))
    }

    /// A titled window holding `views`, whose key state a test controls.
    static func focusWindow(_ views: [NSView] = []) -> FocusTestWindow {
        let window = FocusTestWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: .titled, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let content = NSView()
        views.forEach(content.addSubview)
        window.contentView = content
        return window
    }

    /// The scroll views under a view, in drawing order.
    static func scrollViews(_ view: NSView) -> [NSScrollView] {
        (view as? NSScrollView).map { [$0] } ?? view.subviews.flatMap(scrollViews)
    }

    /// A horizontal scroll view's complete right-hand side is reachable.
    static func assertInspectableEnd(_ scroll: NSScrollView) throws {
        let document = try XCTUnwrap(scroll.documentView)
        XCTAssertGreaterThan(document.bounds.width, scroll.contentView.bounds.width)
        let end = document.bounds.maxX
        scroll.contentView.scroll(to: NSPoint(x: end, y: 0))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertGreaterThan(scroll.contentView.bounds.minX, 0)
        XCTAssertGreaterThanOrEqual(scroll.contentView.bounds.maxX, end, "The complete right-hand side must be reachable")
    }

    /// The size a view settles at inside `width` × `height`.
    static func measured<V: View>(_ view: V, width: CGFloat, height: CGFloat) -> CGSize {
        var size = CGSize.zero
        let host = NSHostingView(rootView: view.onGeometryChange(for: CGSize.self, of: \.size) { size = $0 }
            .frame(width: width, height: height, alignment: .topLeading))
        host.layoutSubtreeIfNeeded()
        settle()
        return size
    }

    /// The rows of the view's pixels a dark-ink test counts: rows holding a dark pixel.
    static func darkInkRows(_ bitmap: NSBitmapImageRep) -> [Int] {
        (0..<bitmap.pixelsHigh).filter { y in
            (0..<bitmap.pixelsWide).contains { x in
                guard let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return false }
                return colour.redComponent < 0.4 && colour.greenComponent < 0.4 && colour.blueComponent < 0.4
            }
        }
    }

    /// Counts pixels whose colour passes `test`, for ink-coverage assertions.
    static func pixels(_ bitmap: NSBitmapImageRep, where test: (NSColor) -> Bool) -> Int {
        (0..<bitmap.pixelsHigh).reduce(0) { count, y in
            count + (0..<bitmap.pixelsWide).filter { x in
                (bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB)).map(test) ?? false
            }.count
        }
    }
}

/// Writes one frame's rows over one growing text run, as the bridge itself writes them
/// ([NATIVE-SHM]): the same fixed-width `OspNativeView` rows the host reads in place, so a test
/// can hand the checker real bytes — or bytes that must be refused.
final class FrameWire {
    private(set) var rows: [OspNativeView] = []
    private var asks: [OspNativeAsk] = []
    private var text = ""

    /// Rewrites a row's text range, for the ranges a frame must refuse.
    func moveText(of row: Int, to offset: Int64, _ length: Int64) {
        rows[row].text_offset = offset
        rows[row].text_length = length
    }

    /// Appends one view row; `words` and `hint` join the frame's one text run.
    func row(_ kind: Int64, parent: Int64 = -1, action: Int64 = -1, words: String = "", hint: String = "",
             weight: Int64 = 400, size: Double = 13, flags: Int64 = 0, colour: Int64 = 0, background: Int64 = 0,
             width: Double = 0, height: Double = 0, grow: Double = 0, border: Int64 = 0,
             padding: Double = 0, spacing: Double = 0, radius: Double = 0) {
        let wordsAt = Int64(text.utf8.count)
        text += words
        let hintAt = Int64(text.utf8.count)
        text += hint
        var row = OspNativeView()
        row.kind = kind; row.parent = parent; row.action = action
        row.text_offset = wordsAt; row.text_length = Int64(words.utf8.count)
        row.colour = colour; row.background = background
        row.size = size; row.padding = padding; row.spacing = spacing; row.radius = radius
        row.weight = weight; row.width = width; row.height = height; row.grow = grow
        row.flags = flags; row.border = border
        row.hint_offset = hintAt; row.hint_length = Int64(hint.utf8.count)
        rows.append(row)
    }

    /// Appends one ask row: a field of 0 starts an ask, the rest are its arguments.
    func ask(_ field: Int64, _ number: Int64 = 0, _ text: String = "") {
        let at = Int64(self.text.utf8.count)
        self.text += text
        var row = OspNativeAsk()
        row.field = field; row.number = number; row.text_offset = at; row.text_length = Int64(text.utf8.count)
        asks.append(row)
    }

    /// The frame these rows make, checked as the host checks it.
    func read() throws -> Shown {
        try rows.withUnsafeBufferPointer { views in
            try text.data(using: .utf8)!.withUnsafeBytes { run in
                try asks.withUnsafeBufferPointer { asked in
                    try SharedRows.read(shapes: UnsafeBufferPointer(start: nil, count: 0), views: views,
                                         text: UnsafeRawBufferPointer(run), asks: asked)
                }
            }
        }
    }
}
