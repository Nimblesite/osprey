import AppKit
import SwiftUI
import XCTest

@testable import OspreyShared

// The floating shadow a frame authors: cast by its surface alone, extending outside it without
// changing layout. Values the frame does not know are refused by the checker before a view
// exists — the validation the forked host's style decoder used to do now lives in the rows.

final class FrameShadowTests: RenderingTestCase {
    // A value the frame does not know — an unknown flag bit, an unknown kind, a weight outside
    // its range — refuses the whole frame as a typed error instead of rendering without its
    // effect.
    func testUnknownShadowNamesFailInsteadOfRenderingWithoutTheEffect() {
        for flags in [Int64(128), Int64(256), Int64(1024)] {
            let wire = FrameWire()
            wire.row(3, words: "Control", flags: flags)
            XCTAssertThrowsError(try wire.read(), "unknown flags \(flags)") { error in
                XCTAssertTrue(error is SharedRefusal, "\(error)")
            }
        }
        let wire = FrameWire()
        wire.row(3, words: "Control", weight: 0)
        XCTAssertThrowsError(try wire.read()) { error in
            XCTAssertTrue(error is SharedRefusal, "\(error)")
        }
    }

    // The floating surface casts its authored shadow below the border, outside the frame, and
    // the frame itself keeps exactly its authored size.
    @MainActor
    func testAuthoredFloatingShadowExtendsOutsideTheFrameWithoutChangingLayout() throws {
        let node = Fixtures.framed(["width": .number(60), "height": .number(40), "fill": "canvas", "shadow": .string("floating")],
                                   Fixtures.text("", "body", "one"))
        let view = NodeView(node: node, drawing: Fixtures.drawing())
            .environment(\.ospreyTheme, Fixtures.resolver([("canvas", "#ffffff"), ("one", "#242923")]))
            .padding(40).background(Color.white)
        let bitmap = try TestResources.rendered(view)
        XCTAssertEqual(bitmap.pixelsWide, 140)
        XCTAssertEqual(bitmap.pixelsHigh, 120)
        let outside = try TestResources.pixel(bitmap, 70, 90)
        XCTAssertLessThan(outside.redComponent, 0.995, "The floating surface must cast its authored shadow below the border")
    }

    // Extra: only the authored `floating` shadow name lifts a frame — an empty or unknown name
    // draws the frame exactly as no shadow at all, never an error pixel.
    @MainActor
    func testUnknownShadowNamesFailInsteadOfRenderingWithoutTheEffect_extraOnlyTheFloatingNameLiftsAFrame() throws {
        for name in ["", "unknown", "lifted"] {
            let node = Fixtures.framed(["width": .number(60), "height": .number(40), "fill": "canvas", "shadow": .string(name)],
                                       Fixtures.text("", "body", "one"))
            let view = NodeView(node: node, drawing: Fixtures.drawing())
                .environment(\.ospreyTheme, Fixtures.resolver([("canvas", "#ffffff"), ("one", "#242923")]))
                .padding(40).background(Color.white)
            let outside = try TestResources.pixel(try TestResources.rendered(view), 70, 90)
            XCTAssertGreaterThan(outside.redComponent, 0.995, "an unnamed shadow must draw nothing at all (got \(name.debugDescription))")
        }
    }

    // Extra: the shadow never shades the surface it is cast from — the panel's own fill stays
    // exactly its authored colour, and the frame's size is unchanged by the shadow.
    @MainActor
    func testAuthoredFloatingShadowExtendsOutsideTheFrameWithoutChangingLayout_extraTheShadowNeverShadesItsOwnSurface() throws {
        let plain = Fixtures.framed(["width": .number(60), "height": .number(40), "fill": "canvas"], Fixtures.text("", "body", "one"))
        let lifted = Fixtures.framed(["width": .number(60), "height": .number(40), "fill": "canvas", "shadow": .string("floating")],
                                     Fixtures.text("", "body", "one"))
        for node in [plain, lifted] {
            let view = NodeView(node: node, drawing: Fixtures.drawing())
                .environment(\.ospreyTheme, Fixtures.resolver([("canvas", "#ffffff"), ("one", "#242923")]))
                .padding(40).background(Color.white)
            let bitmap = try TestResources.rendered(view)
            XCTAssertEqual(bitmap.pixelsWide, 140)
            XCTAssertEqual(bitmap.pixelsHigh, 120)
            let inside = try TestResources.pixel(bitmap, 70, 70)
            XCTAssertGreaterThan(inside.redComponent, 0.995, "the surface keeps its authored fill under its own shadow")
        }
    }
}
