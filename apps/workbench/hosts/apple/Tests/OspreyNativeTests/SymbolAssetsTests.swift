import AppKit
import SwiftUI
import XCTest
@testable import OspreyNative

final class SymbolAssetsTests: XCTestCase {
    func testEveryShippedDesignSymbolDecodesAsTemplateArtwork() throws {
        let assets = try XCTUnwrap(SymbolAssets.load(root: TestResources.assets))
        for name in ["moon", "sun.max", "folder.open", "square.and.arrow.down", "chevron.left.forwardslash.chevron.right", "pencil", "hand.draw"] {
            XCTAssertTrue(try XCTUnwrap(assets.image(name)).isTemplate, "Missing shipped design symbol: \(name)")
        }
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
            let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#00FFFF", dark: "#00FFFF")], spacing: [], feedback: 0, settle: 0)
            let view = NodeView(node: .icon("pencil"), drawing: Drawing(region: "symbols", revision: 1, send: { _, _ in }))
                .environment(\.ospreySymbols, .success(assets)).environment(\.ospreyTheme, ThemeResolver(theme: theme))
                .frame(width: 32, height: 32).background(Color.white)
            let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(ImageRenderer(content: view).cgImage))
            let ink = try XCTUnwrap(bitmap.colorAt(x: 5, y: 5)?.usingColorSpace(.sRGB))
            XCTAssertLessThan(ink.redComponent, 0.1, "The registered square fixture must replace the diagonal platform pencil")
            XCTAssertGreaterThan(ink.greenComponent, 0.9)
            XCTAssertGreaterThan(ink.blueComponent, 0.9)
        }
    }

    func testAbsentRegistryPreservesThePlatformSymbolVocabulary() throws {
        try withResources { root in XCTAssertNil(try SymbolAssets.load(root: root)) }
    }

    func testMalformedAndOversizedRegistriesFailExplicitly() throws {
        try withResources { root in
            for source in ["not JSON", "[]", #"{"pencil":1}"#, String(repeating: " ", count: 65537)] {
                try Data(source.utf8).write(to: root.appendingPathComponent("symbols.json"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root))
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

    func testDeclaredMissingAndEscapingArtworkIsRefused() throws {
        try withResources { root in
            for path in ["../outside.svg", "/tmp/outside.svg", "symbols/../outside.svg", "symbols/missing.svg"] {
                let data = try JSONSerialization.data(withJSONObject: ["pencil": path])
                try data.write(to: root.appendingPathComponent("symbols.json"))
                XCTAssertThrowsError(try SymbolAssets.load(root: root))
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

    private func registered(_ root: URL) throws -> SymbolAssets {
        try Data(#"{"pencil":"symbols/pencil.svg"}"#.utf8).write(to: root.appendingPathComponent("symbols.json"))
        let directory = root.appendingPathComponent("symbols")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let svg = #"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><path d="M2 2H22V22H2Z"/></svg>"#
        try Data(svg.utf8).write(to: directory.appendingPathComponent("pencil.svg"))
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
