import Foundation
import XCTest
@testable import OspreyNative

final class ProcessAdapterTests: XCTestCase {
    func testMissingPlistFlagRunsAsPlainText() async throws {
        let slices = SliceStore()
        let result = try await RunAdapter().perform(request(), slices: slices)
        XCTAssertEqual(slices.text("output"), "proof output")
        XCTAssertEqual(result, .object(["status": .number(0), "stderr": .string("")]))
    }

    func testExplicitPlistFlagsPreserveTheirOutputModes() async throws {
        let xml = #"<?xml version="1.0"?><plist version="1.0"><dict><key>answer</key><integer>42</integer></dict></plist>"#
        for flag in [false, true] {
            let slices = SliceStore()
            _ = try await RunAdapter().perform(request(plist: .bool(flag), arguments: ["%s", xml]), slices: slices)
            if flag {
                let value = try JSONDecoder().decode(JSONValue.self, from: Data(slices.text("output").utf8))
                XCTAssertEqual(value, .object(["answer": .number(42)]))
            } else {
                XCTAssertEqual(slices.text("output"), xml)
            }
        }
    }

    func testMalformedPresentPlistFlagsFailBeforeLaunchingAProcess() async {
        for value in [JSONValue.null, .string("false"), .number(0), .array([]), .object([:])] {
            do {
                _ = try await RunAdapter().perform(request(plist: value, executable: "/osprey-must-not-launch"), slices: SliceStore())
                XCTFail("A malformed plist flag must fail explicitly")
            } catch let error as ProtocolError {
                XCTAssertEqual(error, .missingMember("bool", "plist"))
            } catch {
                XCTFail("Flag validation must precede launch: \(error)")
            }
        }
    }

    func testLargeStderrCannotBlockCollectionOfEmptyStdout() async throws {
        let arguments = (0..<2000).map { "/osprey-missing-process-fixture-\($0)" }
        let output = try await Launch(executable: "/bin/ls", arguments: arguments, timeout: 1000, limit: 524288).run()
        XCTAssertNotEqual(output.status, 0)
        XCTAssertTrue(output.stdout.isEmpty)
        XCTAssertGreaterThan(output.stderr.count, 65536, "Both pipes must drain concurrently before the process can exit")
    }

    func testMissingOrMalformedOutputSliceCannotLaunchAProcess() async {
        for value in [nil, JSONValue.null, .bool(false), .number(0), .array([]), .object([:])] {
            await assertInvalidSliceCannotLaunch(value)
        }
    }

    private func assertInvalidSliceCannotLaunch(_ value: JSONValue?) async {
        let marker = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-invalid-slice-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: marker) }
        do {
            _ = try await RunAdapter().perform(request(executable: "/usr/bin/touch", arguments: [marker.path], slice: value), slices: SliceStore())
            XCTFail("An invalid output slice must fail explicitly")
        } catch let error as ProtocolError {
            XCTAssertEqual(error, .missingMember("string", "slice"))
        } catch {
            XCTFail("Slice validation must precede launch: \(error)")
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path), "An invalid request must never execute its process")
    }

    func testOtherRequestFlagsRemainRequired() {
        XCTAssertThrowsError(try request().flag("required")) { error in
            XCTAssertEqual(error as? ProtocolError, .missingMember("bool", "required"))
        }
    }

    private func request(plist: JSONValue? = nil, executable: String = "/usr/bin/printf", arguments: [String] = ["proof output"], slice: JSONValue? = .string("output")) -> Request {
        let members: [String: JSONValue] = ["executable": .string(executable), "arguments": .array(arguments.map(JSONValue.string)),
            "timeout": .number(1000), "limit": .number(4096)]
        let optional = (plist.map { ["plist": $0] } ?? [:]).merging(slice.map { ["slice": $0] } ?? [:]) { _, next in next }
        return Request(ticket: "run:test", kind: "run", members: members.merging(optional) { _, next in next })
    }
}
