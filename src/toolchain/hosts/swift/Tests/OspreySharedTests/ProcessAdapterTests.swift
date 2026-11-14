import Foundation
import XCTest

@testable import OspreyShared

// [UI-EFFECTS] Store requests must stay inside the app's private folder, and process requests
// must stay bounded against real child processes, no doubles. Every ask is the typed field list
// the frame's ask rows became, numbered as `Requests::fields` numbers them.

/// Reads one member of a result document, as the JSON host's tests did.
extension AskResult {
    func text(_ key: String) -> String {
        guard case let .document(document) = self, case let .string(value)? = document.object?[key] else { return "" }
        return value
    }

    func count(_ key: String) -> Int {
        guard case let .document(document) = self, case let .number(value)? = document.object?[key] else { return 0 }
        return Int(value)
    }

    func flag(_ key: String) -> Bool {
        guard case let .document(document) = self, case let .bool(value)? = document.object?[key] else { return false }
        return value
    }
}

// [UI-EFFECTS-PROCESS]: the four long-lived process capabilities against real
// child processes. Every request stays bounded, stderr is kept apart, the
// environment merges over the existing one, and quit stops every process.
final class ProcessAdapterTests: RenderingTestCase {
    override func setUp() {
        super.setUp()
        ProcessPool.shared.stopAll()
    }

    override func tearDown() {
        ProcessPool.shared.stopAll()
        super.tearDown()
    }

    private func name(_ label: String) -> String { "\(label)-\(UUID().uuidString)" }

    private func field(_ field: Int64, _ text: String) -> AskField {
        .init(field: field, number: 0, text: text)
    }

    private func whole(_ field: Int64, _ number: Int64) -> AskField {
        .init(field: field, number: number, text: "")
    }

    private func start(_ handle: String, _ command: String, arguments: [String] = [], env: [String: String] = [:]) -> Ask {
        var fields = [field(1, command)]
        fields += arguments.map { field(2, $0) }
        fields += env.flatMap { [field(3, $0.key), field(3, $0.value)] }
        fields.append(field(4, handle))
        return Ask(code: 22, ticket: handle, fields: fields)
    }

    private func write(_ handle: String, _ input: String) -> Ask {
        Ask(code: 23, ticket: handle, fields: [field(1, handle), field(2, input)])
    }

    private func read(_ handle: String, limit: Int = 1_000_000, timeout: Int = 2000) -> Ask {
        Ask(code: 24, ticket: handle, fields: [field(1, handle), whole(2, Int64(limit)), whole(3, Int64(timeout))])
    }

    private func stop(_ handle: String) -> Ask {
        Ask(code: 25, ticket: handle, fields: [field(1, handle)])
    }

    /// Everything a process wrote, read a small piece at a time until it has exited.
    private func drained(_ handle: String, _ adapter: ProcessAdapter, limit: Int = 1_000_000) async throws -> (output: String, stderr: String, reads: Int) {
        var output = "", stderr = "", reads = 0
        while reads < 100_000 {
            let piece = try await adapter.perform(read(handle, limit: limit), slices: SliceStore())
            output += piece.text("output")
            stderr += piece.text("stderr")
            reads += 1
            if piece.flag("exited") { break }
        }
        return (output, stderr, reads)
    }

    // The main thread is busy when the reply arrives, as on a loaded machine: the listening read has taken
    // the bytes and is waiting to publish them when a new read with its ticket replaces it. They must survive.
    @MainActor
    func testOutputTakenByAReplacedReadReachesTheReadThatReplacedIt() async throws {
        let adapter = ProcessAdapter(), runner = CapabilityRunner(adapters: [adapter]), handle = name("replaced")
        _ = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore())
        var outcomes: [Result<AskResult, CapabilityError>] = []
        runner.run(read(handle, timeout: 5000)) { outcomes.append($0) }
        await Task.yield()
        let request = write(handle, "the reply\n")
        Task.detached { _ = try await adapter.perform(request, slices: SliceStore()) }
        Thread.sleep(forTimeInterval: 0.5)
        let replacing = expectation(description: "the replacing read answers")
        runner.run(read(handle, timeout: 2000)) {
            outcomes.append($0)
            replacing.fulfill()
        }
        await fulfillment(of: [replacing], timeout: 5)
        guard case let .success(.document(document)) = outcomes.last,
              let members = document.object, case let .string(output)? = members["output"] else { return XCTFail("\(outcomes)") }
        XCTAssertEqual(output, "the reply\n")
        XCTAssertEqual(outcomes.count, 2)
        guard case .failure(.cancelled) = outcomes[0] else { return XCTFail("\(outcomes)") }
    }

    func testStartWriteAndReadRoundTripThroughCat() async throws {
        let adapter = ProcessAdapter()
        let handle = name("cat")
        let started = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore())
        XCTAssertGreaterThan(started.count("pid"), 0)
        let wrote = try await adapter.perform(write(handle, "hello"), slices: SliceStore())
        XCTAssertEqual(wrote.count("written"), 5)
        let first = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertEqual(first.text("output"), "hello")
        XCTAssertEqual(first.text("stderr"), "")
        XCTAssertFalse(first.flag("exited"))
        _ = try await adapter.perform(write(handle, " world"), slices: SliceStore())
        let second = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertEqual(second.text("output"), " world")
    }

    func testReadsAreBoundedByTheirByteLimitAndDrainProgressively() async throws {
        let adapter = ProcessAdapter()
        let handle = name("digits")
        _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "printf 0123456789"]), slices: SliceStore())
        let first = try await adapter.perform(read(handle, limit: 4), slices: SliceStore())
        XCTAssertEqual(first.text("output"), "0123")
        let second = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertEqual(second.text("output"), "456789")
    }

    func testAReadAtItsDeadlineReturnsEmptyWhenThereIsNoOutput() async throws {
        let adapter = ProcessAdapter()
        let handle = name("quiet")
        _ = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore())
        let started = Date()
        let result = try await adapter.perform(read(handle, timeout: 100), slices: SliceStore())
        XCTAssertLessThan(Date().timeIntervalSince(started), 5)
        XCTAssertEqual(result.text("output"), "")
        XCTAssertEqual(result.text("stderr"), "")
        XCTAssertFalse(result.flag("exited"))
    }

    func testStderrIsKeptApartFromStdout() async throws {
        let adapter = ProcessAdapter()
        let handle = name("err")
        _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "printf out; printf err >&2"]), slices: SliceStore())
        let result = try await drained(handle, adapter)
        XCTAssertEqual(result.output, "out")
        XCTAssertEqual(result.stderr, "err")
    }

    func testEnvironmentIsMergedOverTheExistingOne() async throws {
        let adapter = ProcessAdapter()
        let handle = name("env")
        _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "printf \"$MERGE_TEST|$PATH\""], env: ["MERGE_TEST": "merged"]), slices: SliceStore())
        let result = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertTrue(result.text("output").hasPrefix("merged|"))
        XCTAssertTrue(result.text("output").contains("/usr/bin"))
    }

    func testExitStatusIsReported() async throws {
        let adapter = ProcessAdapter()
        let handle = name("exit")
        _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "exit 5"]), slices: SliceStore())
        let result = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertTrue(result.flag("exited"))
        XCTAssertEqual(result.count("status"), 5)
        XCTAssertEqual(result.text("output"), "")
    }

    func testStopIsHarmlessTwiceAndForUnknownHandles() async throws {
        let adapter = ProcessAdapter()
        let handle = name("stop")
        _ = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore())
        let first = try await adapter.perform(stop(handle), slices: SliceStore())
        XCTAssertTrue(first.flag("stopped"))
        let second = try await adapter.perform(stop(handle), slices: SliceStore())
        XCTAssertFalse(second.flag("stopped"))
        let unknown = try await adapter.perform(stop(name("unknown")), slices: SliceStore())
        XCTAssertFalse(unknown.flag("stopped"))
    }

    func testAMissingCommandIsATypedError() async {
        let adapter = ProcessAdapter()
        let handle = name("missing")
        do {
            _ = try await adapter.perform(start(handle, "definitely-not-a-real-command"), slices: SliceStore())
            XCTFail("a missing command started")
        } catch let error as CapabilityError {
            XCTAssertEqual(error, .notAllowed("Command not found: definitely-not-a-real-command"))
        } catch {
            XCTFail("unexpected error \(error)")
        }
    }

    func testQuitStopsEveryProcess() async throws {
        let adapter = ProcessAdapter()
        let handle = name("quit")
        let started = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore())
        let pid = Int32(started.count("pid"))
        XCTAssertGreaterThan(pid, 0)
        XCTAssertEqual(kill(pid, 0), 0)
        ProcessPool.shared.stopAll()
        var gone = false
        for _ in 0 ..< 50 {
            if kill(pid, 0) != 0 {
                gone = true
                break
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTAssertTrue(gone, "pid \(pid) survived quit")
    }

    func testAWriteToAProcessThatClosedItsInputIsATypedErrorNotASignal() async throws {
        let adapter = ProcessAdapter()
        let handle = name("closed")
        _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "exec 0<&-; printf ready; exec sleep 30"]), slices: SliceStore())
        let ready = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertEqual(ready.text("output"), "ready")
        do {
            _ = try await adapter.perform(write(handle, "nobody is reading"), slices: SliceStore())
            XCTFail("a write to a closed input succeeded")
        } catch let error as CapabilityError {
            guard case .platform = error else { return XCTFail("unexpected error \(error)") }
        }
    }

    func testAWriteToAProcessThatWillNotReadBlocksNothingElseAndEndsWhenItStops() async throws {
        let adapter = ProcessAdapter()
        let handle = name("stuck")
        let other = name("other")
        _ = try await adapter.perform(start(handle, "/bin/sleep", arguments: ["30"]), slices: SliceStore())
        _ = try await adapter.perform(start(other, "/bin/cat"), slices: SliceStore())
        let flood = String(repeating: "x", count: 1_000_000)
        let writing = Task { try await adapter.perform(self.write(handle, flood), slices: SliceStore()) }
        try await Task.sleep(for: .milliseconds(100))
        _ = try await adapter.perform(write(other, "still here"), slices: SliceStore())
        let echoed = try await adapter.perform(read(other), slices: SliceStore())
        XCTAssertEqual(echoed.text("output"), "still here")
        let stuckRead = try await adapter.perform(read(handle, timeout: 50), slices: SliceStore())
        XCTAssertFalse(stuckRead.flag("exited"))
        _ = try await adapter.perform(stop(handle), slices: SliceStore())
        do {
            _ = try await writing.value
            XCTFail("a write to a stopped process succeeded")
        } catch let error as CapabilityError {
            guard case .platform = error else { return XCTFail("unexpected error \(error)") }
        }
    }

    func testPiecesNeverSplitACharacterWhateverTheLimit() async throws {
        let adapter = ProcessAdapter()
        let text = String(repeating: "aé€😀", count: 200)
        for limit in [1, 2, 3, 4, 5, 6, 7, 64] {
            let handle = name("utf8-\(limit)")
            _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "printf '%s' \"$TEXT\""], env: ["TEXT": text]), slices: SliceStore())
            let result = try await drained(handle, adapter, limit: limit)
            XCTAssertEqual(result.output, text, "limit \(limit)")
        }
    }

    func testACharacterArrivingInTwoWritesIsHeldUntilItIsWhole() async throws {
        let adapter = ProcessAdapter()
        let handle = name("split")
        _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "printf '\\342'; sleep 0.3; printf '\\202\\254'"]), slices: SliceStore())
        let result = try await drained(handle, adapter)
        XCTAssertEqual(result.output, "€")
    }

    func testExitedIsOnlyReportedOnceEveryByteHasBeenRead() async throws {
        let adapter = ProcessAdapter()
        for round in 0 ..< 20 {
            let handle = name("all-\(round)")
            _ = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "i=0; while [ $i -lt 2000 ]; do echo line$i; i=$((i+1)); done; exit 7"]), slices: SliceStore())
            var (lines, last) = ("", AskResult.document(.null))
            repeat {
                last = try await adapter.perform(read(handle, limit: 512), slices: SliceStore())
                lines += last.text("output")
            } while !last.flag("exited")
            XCTAssertEqual(lines.split(separator: "\n").count, 2000, "round \(round)")
            XCTAssertTrue(lines.hasSuffix("line1999\n"), "round \(round)")
            XCTAssertEqual(last.count("status"), 7)
        }
    }

    func testManyWaitingReadsHoldNoThreads() async throws {
        let adapter = ProcessAdapter()
        let handles = (0 ..< 64).map { name("wait-\($0)") }
        for handle in handles { _ = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore()) }
        let started = Date()
        try await withThrowingTaskGroup(of: Void.self) { group in
            for handle in handles {
                group.addTask { _ = try await adapter.perform(self.read(handle, timeout: 500), slices: SliceStore()) }
            }
            try await group.waitForAll()
        }
        XCTAssertLessThan(Date().timeIntervalSince(started), 1.5)
    }

    func testACancelledReadEndsAtOnce() async throws {
        let adapter = ProcessAdapter()
        let handle = name("cancel")
        _ = try await adapter.perform(start(handle, "/bin/cat"), slices: SliceStore())
        let started = Date()
        let reading = Task { try await adapter.perform(self.read(handle, timeout: 30000), slices: SliceStore()) }
        try await Task.sleep(for: .milliseconds(50))
        reading.cancel()
        _ = try? await reading.value
        XCTAssertLessThan(Date().timeIntervalSince(started), 2)
    }

    func testStopClosesInputSoAServerCanEndItself() async throws {
        let adapter = ProcessAdapter()
        let handle = name("eof")
        let marker = NSTemporaryDirectory() + "osprey-eof-\(UUID().uuidString)"
        defer { try? FileManager.default.removeItem(atPath: marker) }
        let script = "trap '' TERM; printf ready; cat >/dev/null; echo done > \"$MARK\""
        let started = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", script], env: ["MARK": marker]), slices: SliceStore())
        XCTAssertGreaterThan(started.count("pid"), 0)
        let ready = try await adapter.perform(read(handle), slices: SliceStore())
        XCTAssertEqual(ready.text("output"), "ready")
        _ = try await adapter.perform(stop(handle), slices: SliceStore())
        for _ in 0 ..< 50 where !FileManager.default.fileExists(atPath: marker) { try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertTrue(FileManager.default.fileExists(atPath: marker))
    }

    func testQuitLeavesNothingBehindEvenAProcessThatIgnoresTerminate() async throws {
        let adapter = ProcessAdapter()
        let handle = name("stubborn")
        let started = try await adapter.perform(start(handle, "/bin/sh", arguments: ["-c", "trap '' TERM; exec sleep 60"]), slices: SliceStore())
        let pid = Int32(started.count("pid"))
        try await Task.sleep(for: .milliseconds(100))
        ProcessPool.shared.stopAll()
        var gone = kill(pid, 0) != 0
        for _ in 0 ..< 10 where !gone {
            try await Task.sleep(for: .milliseconds(10))
            gone = kill(pid, 0) != 0
        }
        XCTAssertTrue(gone, "pid \(pid) outlived quit")
    }
}
