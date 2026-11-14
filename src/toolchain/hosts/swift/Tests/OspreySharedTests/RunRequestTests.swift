import Foundation
import XCTest

@testable import OspreyShared

// The Workbench's run-request tests: the property-list flag this host answers with a typed
// refusal rather than a conversion, the output slice a request must name before anything
// launches, and the pipes that must drain concurrently. Restored from the forked host, on the
// typed ask rows the frame now carries.
final class RunRequestTests: RenderingTestCase {
    /// One run request: an executable, its arguments, the deadline, the byte limit, the
    /// property-list flag and the slice its output belongs to.
    private func workbenchRun(_ executable: String = "/usr/bin/printf", arguments: [String] = ["proof output"],
                              timeout: Int64? = 1000, limit: Int64? = 4096, plist: Int64? = nil, slice: String? = "output") -> Ask {
        var fields = [AskField(field: 1, number: 0, text: executable)]
        fields += arguments.map { AskField(field: 2, number: 0, text: $0) }
        if let timeout { fields.append(AskField(field: 3, number: timeout, text: "")) }
        if let limit { fields.append(AskField(field: 4, number: limit, text: "")) }
        if let plist { fields.append(AskField(field: 5, number: plist, text: "")) }
        if let slice { fields.append(AskField(field: 6, number: 0, text: slice)) }
        return Ask(code: 1, ticket: "run:test", fields: fields)
    }

    func testMissingPlistFlagRunsAsPlainText() async throws {
        let slices = SliceStore()
        let result = try await RunAdapter().perform(workbenchRun(), slices: slices)
        XCTAssertEqual(slices.text("output"), "proof output")
        XCTAssertEqual(result, .document(.object(["status": .number(0), "stderr": .string("")])))
    }

    func testExplicitPlistFlagsPreserveTheirOutputModes() async throws {
        let xml = #"<?xml version="1.0"?><plist version="1.0"><dict><key>answer</key><integer>42</integer></dict></plist>"#
        for flag in [Int64(0), 1] {
            let slices = SliceStore()
            if flag == 0 {
                _ = try await RunAdapter().perform(workbenchRun(arguments: ["%s", xml], plist: 0), slices: slices)
                XCTAssertEqual(slices.text("output"), xml, "an absent conversion keeps the raw bytes the process wrote")
            } else {
                do {
                    _ = try await RunAdapter().perform(workbenchRun(arguments: ["%s", xml], plist: flag), slices: slices)
                    XCTFail("A present plist flag must fail explicitly")
                } catch let error as CapabilityError {
                    XCTAssertEqual(error, .notAllowed("This host does not convert property lists"))
                }
            }
        }
    }

    func testMalformedPresentPlistFlagsFailBeforeLaunchingAProcess() async {
        let marker = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-plist-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: marker) }
        for value in [Int64(1), 2, -1] {
            do {
                _ = try await RunAdapter().perform(workbenchRun("/usr/bin/touch", arguments: [marker.path], plist: value), slices: SliceStore())
                XCTFail("A present plist flag must fail explicitly")
            } catch let error as CapabilityError {
                XCTAssertEqual(error, .notAllowed("This host does not convert property lists"))
            } catch {
                XCTFail("Flag validation must precede launch: \(error)")
            }
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path), "An invalid request must never execute its process")
    }

    func testLargeStderrCannotBlockCollectionOfEmptyStdout() async throws {
        let arguments = (0..<2000).map { "/osprey-missing-process-fixture-\($0)" }
        let output = try await Launch(executable: "/bin/ls", arguments: arguments, timeout: 1000, limit: 524288).run()
        XCTAssertNotEqual(output.status, 0)
        XCTAssertTrue(output.stdout.isEmpty)
        XCTAssertGreaterThan(output.stderr.count, 65536, "Both pipes must drain concurrently before the process can exit")
    }

    func testMissingOrMalformedOutputSliceCannotLaunchAProcess() async {
        let marker = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-invalid-slice-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: marker) }
        for value in [nil, ""] {
            do {
                _ = try await RunAdapter().perform(workbenchRun("/usr/bin/touch", arguments: [marker.path], slice: value), slices: SliceStore())
                XCTFail("An invalid output slice must fail explicitly")
            } catch let error as CapabilityError {
                XCTAssertEqual(error, .invalidArgument("run has no valid slice"), "slice \(value.debugDescription)")
            } catch {
                XCTFail("Slice validation must precede launch: \(error)")
            }
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path), "An invalid request must never execute its process")
    }

    func testOtherRequestFlagsRemainRequired() async throws {
        for ask in [workbenchRun("", slice: "output").dropping(1),
                    workbenchRun(timeout: nil).dropping(3),
                    workbenchRun(limit: nil).dropping(4),
                    workbenchRun(slice: "output").dropping(6)] {
            do {
                _ = try await RunAdapter().perform(ask, slices: SliceStore())
                XCTFail("A request without its required fields must fail: \(ask.fields)")
            } catch let error as CapabilityError {
                guard case .invalidArgument = error else { return XCTFail("unexpected error \(error)") }
            }
        }
    }

    // Extra: the exit status of a missing executable is a typed launch failure, never a crash
    // and never a phantom status.
    func testMissingPlistFlagRunsAsPlainText_extraAMissingExecutableIsALaunchFailure() async {
        do {
            _ = try await RunAdapter().perform(workbenchRun("/osprey-no-such-executable"), slices: SliceStore())
            XCTFail("a missing executable launched")
        } catch {
            XCTAssertTrue("\(error)".contains("exist"), "the failure must name the missing file: \(error)")
        }
    }

    // Extra: the deadline and byte limit apply together — the earlier one wins.
    func testExplicitPlistFlagsPreserveTheirOutputModes_extraDeadlineAndLimitApplyTogether() async {
        let started = Date()
        do {
            _ = try await RunAdapter().perform(workbenchRun("/bin/sleep", arguments: ["30"], timeout: 150, limit: 4096), slices: SliceStore())
            XCTFail("a sleeping child ended on its own")
        } catch let error as CapabilityError {
            XCTAssertEqual(error, .timedOut(150))
        } catch {
            XCTFail("unexpected error \(error)")
        }
        XCTAssertLessThan(Date().timeIntervalSince(started), 10)
    }

    // Extra: arguments travel whole — quoting, spaces and UTF-8 reach the child unchanged.
    func testMalformedPresentPlistFlagsFailBeforeLaunchingAProcess_extraArgumentsTravelWhole() async throws {
        let slices = SliceStore()
        let words = "spaces\tand é→ tabs"
        _ = try await RunAdapter().perform(workbenchRun("/bin/echo", arguments: ["-n", words], slice: "echoed"), slices: slices)
        XCTAssertEqual(slices.text("echoed"), words)
    }

    // Extra: a zero-length output is still a completed run with an empty slice.
    func testOtherRequestFlagsRemainRequired_extraAnEmptyOutputIsACompletedRun() async throws {
        let slices = SliceStore()
        let result = try await RunAdapter().perform(workbenchRun("/usr/bin/true", slice: "silent"), slices: slices)
        XCTAssertEqual(slices.text("silent"), "")
        XCTAssertEqual(result, .document(.object(["status": .number(0), "stderr": .string("")])))
    }

    // Extra: a relative executable never resolves — the request names one absolute program.
    func testMissingOrMalformedOutputSliceCannotLaunchAProcess_extraARelativeExecutableIsRefused() async throws {
        do {
            _ = try await RunAdapter().perform(workbenchRun("printf", arguments: ["relative"]), slices: SliceStore())
            XCTFail("a relative executable ran")
        } catch let error as CapabilityError {
            XCTAssertEqual(error, .notAllowed("Executable must be an absolute path"))
        }
    }

    // Extra: the reported stderr keeps its first 65,536 bytes when a child writes more.
    func testLargeStderrCannotBlockCollectionOfEmptyStdout_extraStderrIsCappedAtSixtyFiveKilobytes() async throws {
        let ask = workbenchRun("/bin/sh", arguments: ["-c", "i=0; while [ $i -lt 30000 ]; do printf 'err' >&2; i=$((i+1)); done"],
                               timeout: 10_000, limit: 1_000_000)
        let result = try await RunAdapter().perform(ask, slices: SliceStore())
        guard case let .document(document) = result, case let .string(stderr)? = document.object?["stderr"] else { return XCTFail("no document") }
        XCTAssertEqual(stderr.utf8.count, 65_536)
        XCTAssertEqual(stderr, String(String(repeating: "err", count: 30_000).prefix(65_536)))
    }
}

private extension Ask {
    /// The ask without its field `field`, as a frame that never sent it.
    func dropping(_ field: Int64) -> Ask {
        Ask(code: code, ticket: ticket, fields: fields.filter { $0.field != field })
    }
}
