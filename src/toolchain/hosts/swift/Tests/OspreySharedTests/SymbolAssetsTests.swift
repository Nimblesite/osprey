import AppKit
import SwiftUI
import XCTest

@testable import OspreyShared

// Bundled symbol artwork is a host resource, never app logic ([UI-COMPONENTS]): a bundle's
// `symbols.json` manifest, its SVG artwork, the template rendering that inherits scoped ink,
// and the size and symlink boundaries that keep both inside the bundle.

final class SymbolAssetsTests: RenderingTestCase {
    // The packaged application icon (`icon.png`, the one `run.sh` turns into every size a Mac
    // asks for) keeps an opaque dark backing and a visible bird at the Dock's smallest sizes.
    @MainActor
    func testPackagedDockIconKeepsAnOpaqueDarkBackingAndVisibleBirdAtSmallSizes() throws {
        let file = try XCTUnwrap(Repository.firstExampleIcon(), "no application icon.png ships today")
        let image = try XCTUnwrap(NSImage(contentsOf: file))
        for size in [16, 64] {
            let bitmap = try dockBitmap(image, size)
            let backing = try TestResources.pixel(bitmap, size / 4, size / 4)
            XCTAssertGreaterThan(backing.alphaComponent, 0.98, "The bird needs its own opaque backing instead of macOS's pale fallback")
            XCTAssertLessThan(luminance(backing), 0.05, "The Dock tile must retain the brand's deep navy contrast")
            let visible = (0..<size).flatMap { y in (0..<size).compactMap { x in bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) } }
                .filter { $0.alphaComponent > 0.9 && (luminance($0) + 0.05) / (luminance(backing) + 0.05) >= 3 }
            XCTAssertGreaterThan(Double(visible.count) / Double(size * size), 0.05, "The bird must remain recognizable at \(size) pixels")
        }
    }

    @MainActor
    private func dockBitmap(_ image: NSImage, _ size: Int) throws -> NSBitmapImageRep {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
        bitmap.size = NSSize(width: size, height: size)
        NSGraphicsContext.saveGraphicsState()
        defer { NSGraphicsContext.restoreGraphicsState() }
        NSGraphicsContext.current = try XCTUnwrap(NSGraphicsContext(bitmapImageRep: bitmap))
        image.draw(in: CGRect(x: 0, y: 0, width: size, height: size), from: .zero, operation: .copy, fraction: 1)
        return bitmap
    }

    private func luminance(_ color: NSColor) -> Double {
        let linear: (Double) -> Double = { $0 <= 0.04045 ? $0 / 12.92 : pow(($0 + 0.055) / 1.055, 2.4) }
        return 0.2126 * linear(color.redComponent) + 0.7152 * linear(color.greenComponent) + 0.0722 * linear(color.blueComponent)
    }

    // Extra: the packaging step's largest tile keeps the same opaque backing, so the icon never
    // washes out at full Dock size either.
    @MainActor
    func testPackagedDockIconKeepsAnOpaqueDarkBackingAndVisibleBirdAtSmallSizes_extraThePackedIconStaysOpaqueAtItsLargestTile() throws {
        let file = try XCTUnwrap(Repository.firstExampleIcon(), "no application icon.png ships today")
        let bitmap = try dockBitmap(try XCTUnwrap(NSImage(contentsOf: file)), 512)
        for point in [(128, 128), (384, 384)] {
            let backing = try TestResources.pixel(bitmap, point.0, point.1)
            XCTAssertGreaterThan(backing.alphaComponent, 0.98)
            XCTAssertLessThan(luminance(backing), 0.05)
        }
    }

    // Extra: every artwork file the shipped manifest names exists in the bundle.
    func testEveryShippedDesignSymbolDecodesAsTemplateArtwork_extraEveryManifestEntryNamesAnExistingArtworkFile() throws {
        let assets = try XCTUnwrap(SymbolAssets.load(root: Repo.assets))
        let manifest = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: Repo.assets.appendingPathComponent("symbols.json")))
        XCTAssertGreaterThan(manifest.count, 20, "the shipped vocabulary is a real design system")
        for (name, path) in manifest {
            XCTAssertNotNil(assets.image(name), "shipped artwork cannot decode: \(name) at \(path)")
        }
    }

    func testEveryShippedDesignSymbolDecodesAsTemplateArtwork() throws {
        let assets = try XCTUnwrap(SymbolAssets.load(root: Repo.assets))
        for name in ["moon", "sun.max", "folder.open", "square.and.arrow.down", "chevron.left.forwardslash.chevron.right", "pencil", "hand.draw"] {
            XCTAssertTrue(try XCTUnwrap(assets.image(name)).isTemplate, "Missing shipped design symbol: \(name)")
        }
    }

    // Extra: an unknown name has no bundled artwork, so the platform glyph answers for it.
    func testBundledRegistryResolvesTemplateArtworkInsteadOfTheSystemGlyph_extraUnmappedNamesFallBackToThePlatformGlyph() throws {
        let assets = try XCTUnwrap(SymbolAssets.load(root: Repo.assets))
        XCTAssertNil(assets.image("definitely-not-a-shipped-symbol"))
        XCTAssertNotNil(assets.image("moon"), "the fixture must still resolve shipped artwork")
    }

    func testBundledRegistryResolvesTemplateArtworkInsteadOfTheSystemGlyph() throws {
        try withResources { root in
            let assets = try registered(root)
            let image = try XCTUnwrap(assets.image("pencil"))
            XCTAssertTrue(image.isTemplate, "Bundled artwork must inherit the same scoped ink as its button")
            XCTAssertEqual(image.size, NSSize(width: 24, height: 24))
            XCTAssertNil(assets.image("book"), "Unmapped semantic symbols retain their platform glyph")
        }
    }

    @MainActor
    func testRenderedSemanticSymbolUsesBundledArtworkAndScopedInk() throws {
        try withResources { root in
            let assets = try registered(root)
            let resolver = Fixtures.resolver([("primary", "#00FFFF")])
            let node = Fixtures.framed(["symbolSize": 24, "ink": "primary"], .object(["kind": .string("icon"), "symbol": .string("pencil")]))
            let view = NodeView(node: node, drawing: Fixtures.drawing())
                .environment(\.ospreySymbols, .success(assets)).environment(\.ospreyTheme, resolver)
                .frame(width: 32, height: 32).background(Color.white)
            let bitmap = try TestResources.rendered(view)
            var ink = 0
            for y in 0..<32 { for x in 0..<32 {
                if let colour = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB), colour.greenComponent > 0.9, colour.redComponent < 0.1, colour.blueComponent > 0.9 { ink += 1 }
            } }
            XCTAssertGreaterThan(ink, 300, "The registered square fixture must replace the diagonal platform pencil")
            XCTAssertLessThan(ink, 24 * 24, "the fixture keeps its artwork bounds")
        }
    }

    // Extra: template artwork takes the ink of the tree it is drawn in — the same square
    // fixture renders red under a red frame and blue under a blue one.
    @MainActor
    func testRenderedSemanticSymbolUsesBundledArtworkAndScopedInk_extraTemplateArtworkFollowsItsScopedInk() throws {
        try withResources { root in
            let assets = try registered(root)
            for (hex, red, blue) in [("#FF0000", true, false), ("#0000FF", false, true)] {
                let resolver = Fixtures.resolver([("primary", hex)])
                let node = Fixtures.framed(["symbolSize": 24, "ink": "primary"], .object(["kind": .string("icon"), "symbol": .string("pencil")]))
                let view = NodeView(node: node, drawing: Fixtures.drawing())
                    .environment(\.ospreySymbols, .success(assets)).environment(\.ospreyTheme, resolver)
                    .frame(width: 32, height: 32).background(Color.white)
                let ink = try TestResources.pixel(try TestResources.rendered(view), 12, 12)
                red ? XCTAssertGreaterThan(ink.redComponent, 0.9) : XCTAssertLessThan(ink.redComponent, 0.1)
                blue ? XCTAssertGreaterThan(ink.blueComponent, 0.9) : XCTAssertLessThan(ink.blueComponent, 0.1)
            }
        }
    }

    func testAbsentRegistryPreservesThePlatformSymbolVocabulary() throws {
        try withResources { root in XCTAssertNil(try SymbolAssets.load(root: root)) }
    }

    // Extra: an empty registry object is present but maps nothing, which is not an error.
    func testAbsentRegistryPreservesThePlatformSymbolVocabulary_extraAnEmptyRegistryMapsNothing() throws {
        try withResources { root in
            try Data("{}".utf8).write(to: root.appendingPathComponent("symbols.json"))
            let assets = try XCTUnwrap(SymbolAssets.load(root: root))
            XCTAssertNil(assets.image("pencil"))
        }
    }

    func testMalformedAndOversizedRegistriesFailExplicitly() throws {
        try withResources { root in
            for source in ["not JSON", "[]", #"{"pencil":1}"#, String(repeating: " ", count: 65537)] {
                try Data(source.utf8).write(to: root.appendingPathComponent("symbols.json"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root))
            }
        }
    }

    // Extra: a manifest with too many entries refuses before any artwork is decoded.
    func testSizeLimitsRejectOtherwiseValidRegistryAndArtwork_extraTooManyRegisteredSymbolsFailExplicitly() throws {
        try withResources { root in
            let entries = (0..<129).map { "\"s\($0)\": \"icons/square.svg\"" }.joined(separator: ",")
            try Data("{\(entries)}".utf8).write(to: root.appendingPathComponent("symbols.json"))
            XCTAssertThrowsError(try SymbolAssets.load(root: root)) { error in
                XCTAssertTrue(error.localizedDescription.contains("Too many bundled symbols"), "\(error)")
            }
        }
    }

    @MainActor
    func testRasterPayloadCannotMasqueradeAsDeclaredSvgArtwork() throws {
        try withResources { root in
            _ = try registered(root)
            let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 2, pixelsHigh: 2, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 8, bitsPerPixel: 32))
            let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            XCTAssertNotNil(NSImage(data: png), "The refusal fixture must be a decodable raster image")
            try png.write(to: root.appendingPathComponent("symbols/pencil.svg"))
            XCTAssertThrowsError(try SymbolAssets.load(root: root), "The SVG contract must validate the payload before platform image decoding")
        }
    }

    // Extra: a raster payload decodes as an image but never registers as artwork, while the
    // same bytes under a `.png` name the manifest never declares stay simply unmapped.
    @MainActor
    func testRasterPayloadCannotMasqueradeAsDeclaredSvgArtwork_extraRasterPayloadsOnlyFailUnderTheirDeclaredSvgName() throws {
        try withResources { root in
            try FileManager.default.createDirectory(at: root.appendingPathComponent("symbols"), withIntermediateDirectories: true)
            let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 2, pixelsHigh: 2, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 8, bitsPerPixel: 32))
            let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            try png.write(to: root.appendingPathComponent("symbols/picture.png"))
            try square().write(to: root.appendingPathComponent("symbols/pencil.svg"))
            try Data("{\"pencil\":\"symbols/pencil.svg\",\"photo\":\"symbols/picture.png\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
            XCTAssertThrowsError(try SymbolAssets.load(root: root), "a `.png` path is not declared SVG artwork")
            try Data("{\"pencil\":\"symbols/pencil.svg\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
            XCTAssertNoThrow(try SymbolAssets.load(root: root))
        }
    }

    func testSizeLimitsRejectOtherwiseValidRegistryAndArtwork() throws {
        try withResources { root in
            _ = try registered(root)
            let manifest = root.appendingPathComponent("symbols.json")
            let source = try Data(contentsOf: manifest)
            try (source + Data(repeating: 32, count: 65537)).write(to: manifest)
            XCTAssertThrowsError(try SymbolAssets.load(root: root)) { XCTAssertTrue($0.localizedDescription.contains("size limit")) }
            try source.write(to: manifest)
            let artwork = root.appendingPathComponent("symbols/pencil.svg")
            try (Data(contentsOf: artwork) + Data(repeating: 32, count: 262145)).write(to: artwork)
            XCTAssertThrowsError(try SymbolAssets.load(root: root)) { XCTAssertTrue($0.localizedDescription.contains("size limit")) }
        }
    }

    // Extra: a name outside the semantic vocabulary refuses the registry before it loads.
    func testMalformedAndOversizedRegistriesFailExplicitly_extraMalformedRegistryNamesFailExplicitly() throws {
        try withResources { root in
            for name in ["", "not a name", "naïve", "semicolons;not", String(repeating: "n", count: 129)] {
                try Data("{\"\\(name)\": \"icons/square.svg\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root), name)
            }
        }
    }

    func testSymbolXmlRequiresTheSvgNamespaceAndRefusesDocumentEntities() throws {
        try withResources { root in
            _ = try registered(root)
            for source in ["<html/>", "<svg width=\"24\" height=\"24\"><path d=\"M0 0H24V24H0Z\"/></svg>", "<!DOCTYPE svg [<!ENTITY ink 'black'>]><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\"><path fill=\"&ink;\" d=\"M0 0H24V24H0Z\"/></svg>"] {
                try Data(source.utf8).write(to: root.appendingPathComponent("symbols/pencil.svg"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root))
            }
        }
    }

    // Extra: artwork paths that are not plain relative SVG file names refuse.
    func testDeclaredMissingAndEscapingArtworkIsRefused_extraMalformedArtworkPathsFailExplicitly() throws {
        try withResources { root in
            try FileManager.default.createDirectory(at: root.appendingPathComponent("icons"), withIntermediateDirectories: true)
            try square().write(to: root.appendingPathComponent("icons/square.svg"))
            for path in ["icons/square.png", "icons/.hidden.svg", "icons/two words.svg", "icons/../icons/square.svg", String(repeating: "d/", count: 90) + "square.svg"] {
                try Data("{\"pencil\": \"\\(path)\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root), path)
            }
        }
    }

    func testDeclaredMissingAndEscapingArtworkIsRefused() throws {
        try withResources { root in
            for path in ["../outside.svg", "/tmp/outside.svg", "symbols/../outside.svg", "symbols/missing.svg"] {
                let data = try JSONSerialization.data(withJSONObject: ["pencil": path])
                try data.write(to: root.appendingPathComponent("symbols.json"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root))
            }
        }
    }

    // Extra: artwork that decodes to nothing — an SVG with no drawable content — is refused.
    func testSymbolXmlRequiresTheSvgNamespaceAndRefusesDocumentEntities_extraUndrawableArtworkIsRefused() throws {
        try withResources { root in
            try FileManager.default.createDirectory(at: root.appendingPathComponent("icons"), withIntermediateDirectories: true)
            try Data(#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"/>"#.utf8).write(to: root.appendingPathComponent("icons/empty.svg"))
            try Data("{\"pencil\": \"icons/empty.svg\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
            XCTAssertThrowsError(try SymbolAssets.load(root: root)) { error in
                XCTAssertTrue(error.localizedDescription.contains("cannot be decoded"), "\(error)")
            }
        }
    }

    func testRegistrySymlinkCannotReadOutsideTheResourceRoot() throws {
        try withResources { container in
            let root = container.appendingPathComponent("resources")
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            let outside = container.appendingPathComponent("private.json")
            try Data("{}".utf8).write(to: outside)
            try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("symbols.json"), withDestinationURL: outside)
            XCTAssertThrowsError(try SymbolAssets.load(root: root), "A registry must obey the same resource boundary as its artwork")
        }
    }

    // Extra: artwork reached through a symlink planted inside the bundle refuses too.
    func testRegistrySymlinkCannotReadOutsideTheResourceRoot_extraArtworkSymlinksCannotEscapeTheResourceRoot() throws {
        try withResources { container in
            let root = container.appendingPathComponent("resources")
            try FileManager.default.createDirectory(at: root.appendingPathComponent("icons"), withIntermediateDirectories: true)
            let outside = container.appendingPathComponent("outside.svg")
            try square().write(to: outside)
            try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("icons/pencil.svg"), withDestinationURL: outside)
            try Data("{\"pencil\": \"icons/pencil.svg\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
            XCTAssertThrowsError(try SymbolAssets.load(root: root))
        }
    }

    private func square() -> Data {
        Data(#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><path d="M2 2H22V22H2Z"/></svg>"#.utf8)
    }

    private func registered(_ root: URL) throws -> SymbolAssets {
        try Data("{\"pencil\":\"symbols/pencil.svg\"}".utf8).write(to: root.appendingPathComponent("symbols.json"))
        let directory = root.appendingPathComponent("symbols")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try square().write(to: directory.appendingPathComponent("pencil.svg"))
        return try XCTUnwrap(SymbolAssets.load(root: root))
    }

    private func withResources(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-symbol-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer {
            do { try FileManager.default.removeItem(at: root) }
            catch { XCTFail("Cannot remove symbol fixtures: \(error)") }
        }
        try body(root)
    }
}

/// The repository's shipped application icons, for the packaging tests.
enum Repository {
    static func firstExampleIcon() -> URL? {
        let examples = Repo.src.appendingPathComponent("osprey/examples")
        let names = (try? FileManager.default.contentsOfDirectory(atPath: examples.path)) ?? []
        for name in names.sorted() {
            let icon = examples.appendingPathComponent(name).appendingPathComponent("icon.png")
            if FileManager.default.fileExists(atPath: icon.path) { return icon }
        }
        return nil
    }
}
