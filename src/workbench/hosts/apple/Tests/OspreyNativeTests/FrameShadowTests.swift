import AppKit
import SwiftUI
import XCTest
@testable import OspreyNative

final class FrameShadowTests: XCTestCase {
    func testUnknownShadowNamesFailInsteadOfRenderingWithoutTheEffect() {
        XCTAssertThrowsError(try TestResources.frame(["shadow": "unknown"]))
    }

    @MainActor
    func testAuthoredFloatingShadowExtendsOutsideTheFrameWithoutChangingLayout() throws {
        let style = try TestResources.frame(["width": 60, "height": 40, "fill": "canvas", "shadow": "floating"])
        let drawing = Drawing(region: "shadow", revision: 1, send: { _, _ in })
        let theme = Theme(tokens: [ThemeToken(name: "primary", light: "#242923", dark: "#242923"), ThemeToken(name: "canvas", light: "#ffffff", dark: "#ffffff")], spacing: [], feedback: 0, settle: 0)
        let view = FramedView(style: style, content: .text("", .body, .primary), drawing: drawing)
            .padding(40).background(Color.white).environment(\.ospreyTheme, ThemeResolver(theme: theme))
        let renderer = ImageRenderer(content: view)
        let bitmap = NSBitmapImageRep(cgImage: try XCTUnwrap(renderer.cgImage))
        XCTAssertEqual(bitmap.pixelsWide, 140)
        XCTAssertEqual(bitmap.pixelsHigh, 120)
        let outside = try XCTUnwrap(bitmap.colorAt(x: 70, y: 90)?.usingColorSpace(.sRGB))
        XCTAssertLessThan(outside.redComponent, 0.995, "The floating surface must cast its authored shadow below the border")
    }
}
