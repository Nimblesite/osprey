import Foundation
import XCTest

@testable import OspreyShared

// A tool the model runs can ignore signals, flood its output or leave a server
// behind: none of them may hang the window's request or grow without bound.
// Every ask is the typed field list the frame's ask rows became, numbered as
// `Requests::fields` numbers them.
final class RunAdapterTests: RenderingTestCase {
    private func ask(_ script: String, timeout: Int = 5000, limit: Int = 1_000_000, plist: Bool = false) -> Ask {
        Ask(code: 1, ticket: "t", fields: [
            .init(field: 1, number: 0, text: "/bin/sh"),
            .init(field: 2, number: 0, text: "-c"),
            .init(field: 2, number: 0, text: script),
            .init(field: 3, number: Int64(timeout), text: ""),
            .init(field: 4, number: Int64(limit), text: ""),
            .init(field: 5, number: plist ? 1 : 0, text: ""),
            .init(field: 6, number: 0, text: "out"),
        ])
    }

    private func failure(of request: Ask, within seconds: TimeInterval) async -> Error? {
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
        let result = try await RunAdapter().perform(ask("printf hello; printf oops >&2; exit 3"), slices: slices)
        XCTAssertEqual(slices.text("out"), "hello")
        guard case let .document(document) = result, case let .object(members) = document else { return XCTFail("not an object") }
        XCTAssertEqual(members["status"], .number(3))
        XCTAssertEqual(members["stderr"], .string("oops"))
    }

    func testDeadlineEndsAChildThatIgnoresTerminate() async {
        let error = await failure(of: ask("trap '' TERM; while :; do sleep 0.1; done", timeout: 100), within: 10)
        guard case .timedOut? = error as? CapabilityError else { return XCTFail("expected a timeout, got \(String(describing: error))") }
    }

    func testOutputLimitEndsAFloodThatIgnoresTerminate() async {
        let error = await failure(of: ask("trap '' TERM; while :; do printf 0123456789abcdef; done", timeout: 60000, limit: 64), within: 10)
        guard case .tooLarge? = error as? CapabilityError else { return XCTFail("expected too large, got \(String(describing: error))") }
    }

    func testABackgroundChildHoldingThePipeDoesNotHoldTheResult() async throws {
        let slices = SliceStore()
        let started = Date()
        let result = try await RunAdapter().perform(ask("sleep 30 & printf started"), slices: slices)
        XCTAssertLessThan(Date().timeIntervalSince(started), 10)
        XCTAssertEqual(slices.text("out"), "started")
        guard case let .document(document) = result, case let .object(members) = document else { return XCTFail("not an object") }
        XCTAssertEqual(members["status"], .number(0))
    }

    func testCancellingBeforeLaunchRunsNothing() async throws {
        let marker = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-run-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: marker) }
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await RunAdapter().perform(ask("touch '\(marker.path)'"), slices: SliceStore())
        }
        _ = try? await task.value
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))
    }

    func testAPropertyListRequestIsRefusedRatherThanReturnedRaw() async {
        let error = await failure(of: ask("printf x", plist: true), within: 10)
        guard case .notAllowed? = error as? CapabilityError else { return XCTFail("expected a refusal, got \(String(describing: error))") }
    }
}
