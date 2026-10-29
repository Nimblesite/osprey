import Foundation
import XCTest

final class EntrypointTests: XCTestCase {
    func testNativeEntrypointIncludesEverySharedApplicationModule() throws {
        let apple = TestResources.apple
        let manifest = try String(contentsOf: apple.appendingPathComponent("app/osprey.toml"), encoding: .utf8)
        let line = try XCTUnwrap(manifest.split(separator: "\n").first { $0.hasPrefix("source_roots = ") })
        let array = try XCTUnwrap(line.firstIndex(of: "["))
        let roots = try JSONDecoder().decode([String].self, from: Data(line[array...].utf8))
        let shared = try FileManager.default.contentsOfDirectory(at: apple.appendingPathComponent("../../src"), includingPropertiesForKeys: nil)
        for file in shared where file.pathExtension == "ospml" && file.lastPathComponent != "main.ospml" {
            XCTAssertTrue(roots.contains("../../../src/" + file.lastPathComponent), "The native app omits shared module \(file.lastPathComponent)")
        }
    }
}
