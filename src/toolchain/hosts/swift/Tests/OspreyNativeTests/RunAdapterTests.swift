import Foundation
import XCTest
@testable import OspreyNative

// A tool the model runs can ignore signals, flood its output or leave a server
// behind: none of them may hang the window's request or grow without bound.
final class RunAdapterTests: XCTestCase {
    private func request(_ script: String, timeout: Int = 5000, limit: Int = 1_000_000, plist: Bool = false) -> Request {
        Request(ticket: "t", kind: "run", members: [
            "executable": .string("/bin/sh"),
            "arguments": .array([.string("-c"), .string(script)]),
            "timeout": .number(Double(timeout)),
            "limit": .number(Double(limit)),
            "plist": .bool(plist),
            "slice": .string("out"),
        ])
    }

    private func failure(of request: Request, within seconds: TimeInterval) async -> Error? {
        let started = Date()
        defer { XCTAssertLessThan(Date().timeIntervalSince(started), seconds) }
        do {
            _ = try await RunAdapter().perform(request, slices: SliceStore())
            return nil
        } catch {
            return error
        }
    }

    func testOutputAndStatusReachTheCaller() async throws {
        let slices = SliceStore()
        let result = try await RunAdapter().perform(request("printf hello; printf oops >&2; exit 3"), slices: slices)
        XCTAssertEqual(slices.text("out"), "hello")
        guard case let .object(members) = result else { return XCTFail("not an object") }
        XCTAssertEqual(members["status"], .number(3))
        XCTAssertEqual(members["stderr"], .string("oops"))
    }

    func testDeadlineEndsAChildThatIgnoresTerminate() async {
        let error = await failure(of: request("trap '' TERM; while :; do sleep 0.1; done", timeout: 100), within: 10)
        guard case .timedOut? = error as? CapabilityError else { return XCTFail("expected a timeout, got \(String(describing: error))") }
    }

    func testOutputLimitEndsAFloodThatIgnoresTerminate() async {
        let error = await failure(of: request("trap '' TERM; while :; do printf 0123456789abcdef; done", timeout: 60000, limit: 64), within: 10)
        guard case .tooLarge? = error as? CapabilityError else { return XCTFail("expected too large, got \(String(describing: error))") }
    }

    func testABackgroundChildHoldingThePipeDoesNotHoldTheResult() async throws {
        let slices = SliceStore()
        let started = Date()
        let result = try await RunAdapter().perform(request("sleep 30 & printf started"), slices: slices)
        XCTAssertLessThan(Date().timeIntervalSince(started), 10)
        XCTAssertEqual(slices.text("out"), "started")
        guard case let .object(members) = result else { return XCTFail("not an object") }
        XCTAssertEqual(members["status"], .number(0))
    }

    func testCancellingBeforeLaunchRunsNothing() async throws {
        let marker = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-run-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: marker) }
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await RunAdapter().perform(request("touch '\(marker.path)'"), slices: SliceStore())
        }
        _ = try? await task.value
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))
    }

    func testAPropertyListRequestIsRefusedRatherThanReturnedRaw() async {
        let error = await failure(of: request("printf x", plist: true), within: 10)
        guard case .notAllowed? = error as? CapabilityError else { return XCTFail("expected a refusal, got \(String(describing: error))") }
    }
}
