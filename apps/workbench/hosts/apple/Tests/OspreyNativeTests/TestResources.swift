import Foundation
@testable import OspreyNative

enum TestResources {
    static let apple = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    static let assets = apple.appendingPathComponent("../../assets").standardizedFileURL

    static func frame(_ values: [String: Any]) throws -> FrameStyle {
        let defaults: [String: Any] = ["name": "", "width": 0, "height": 0, "top": 0, "right": 0, "bottom": 0, "left": 0, "radius": 0, "gap": 0, "fill": "", "ink": "", "line": "", "grow": false, "font": "", "textSize": 0]
        let data = try JSONSerialization.data(withJSONObject: defaults.merging(values) { _, value in value })
        return try JSONDecoder().decode(FrameStyle.self, from: data)
    }
}
