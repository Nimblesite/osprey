import AppKit
import SwiftUI
import XCTest
@testable import OspreyNative

final class FrameInheritanceTests: XCTestCase {
    @MainActor
    func testNestedPlainFrameRetainsAuthoredControlPresentation() throws {
        let parent = try TestResources.frame(["quietControls": true, "trailingSymbols": true, "controlHeight": 38])
        let nested = try TestResources.frame(["quietControls": false, "trailingSymbols": false, "controlHeight": 0])
        var observed: [Int] = []
        let view = ControlProbe { observed = $0 }.modifier(FrameControls(style: nested, padding: nil))
            .modifier(FrameControls(style: parent, padding: nil))
        let host = NSHostingView(rootView: view)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.2))
        XCTAssertEqual(observed, [1, 1, 38], "An ink-only child frame must preserve its parent’s quiet, trailing-symbol control styling")
    }
}

private struct ControlProbe: View {
    let observed: ([Int]) -> Void
    @Environment(\.ospreyQuietControls) private var quiet
    @Environment(\.ospreyTrailingSymbols) private var trailing
    @Environment(\.ospreyControlHeight) private var height

    var body: some View {
        Color.clear.frame(width: 1, height: 1).onAppear { observed([quiet ? 1 : 0, trailing ? 1 : 0, height]) }
    }
}
