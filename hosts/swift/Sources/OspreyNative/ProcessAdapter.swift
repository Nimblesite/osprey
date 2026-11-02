import Foundation

// Implements [UI-EFFECTS]: one executable with arguments, no shell, a deadline,
// separate stdout/stderr and an output limit. The exit status is reported, never
// interpreted. The person's PATH is extended with the usual tool folders, because
// an app opened from the Finder inherits almost none of it.

public struct RunAdapter: CapabilityAdapter {
    public let kinds = ["run"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let executable = try request.text("executable")
        guard executable.hasPrefix("/") else { throw CapabilityError.notAllowed("Executable must be an absolute path") }
        guard try !request.flag("plist") else { throw CapabilityError.notAllowed("This host does not convert property lists") }
        let slice = try request.text("slice")
        let launch = Launch(executable: executable, arguments: try request.texts("arguments"), timeout: try request.number("timeout"), limit: try request.number("limit"))
        let run = try await withTaskCancellationHandler { try await launch.run() } onCancel: { launch.stop() }
        slices.write(slice, String(decoding: run.stdout, as: UTF8.self))
        return .object(["status": .number(Double(run.status)), "stderr": .string(String(decoding: run.stderr.prefix(65536), as: UTF8.self))])
    }
}

struct Launched: Sendable {
    let status: Int32
    let stdout: Data
    let stderr: Data
}

/// Drains both pipes on their own threads, so neither can fill and block the child, and enforces the deadline and limit.
final class Launch: @unchecked Sendable {
    private let process = Process()
    private let out = Pipe(), err = Pipe()
    private let timeout: Int, limit: Int
    private let lock = NSLock()
    private var ended: CapabilityError?
    private var collected = [Data(), Data()]

    init(executable: String, arguments: [String], timeout: Int, limit: Int) {
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        let inherited = environment["PATH"].map { ":" + $0 } ?? ""
        environment["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" + inherited
        environment["LANG"] = "en_US.UTF-8"
        process.environment = environment
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = out
        process.standardError = err
        self.timeout = timeout
        self.limit = limit
    }

    /// Ends the child and remembers why, so its result is reported as that reason. A child that ignores the request is killed.
    private func end(_ reason: CapabilityError) {
        lock.lock()
        let first = ended == nil
        if first { ended = reason }
        lock.unlock()
        guard first, process.isRunning else { return }
        process.terminate()
        DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(2)) { [self] in
            if process.isRunning { kill(process.processIdentifier, SIGKILL) }
        }
    }

    func stop() { end(.cancelled) }

    /// Keeps at most one byte past the limit; the rest is read and discarded so the child never blocks on a full pipe.
    private func drain(_ pipe: Pipe, into slot: Int) {
        let handle = pipe.fileHandleForReading
        while true {
            let block = handle.availableData
            if block.isEmpty { return }
            lock.lock()
            let room = limit + 1 - collected[slot].count
            if room > 0 { collected[slot].append(block.prefix(room)) }
            let over = collected[slot].count > limit
            lock.unlock()
            if over { end(.tooLarge(limit + 1, limit)) }
        }
    }

    private func reason() -> CapabilityError? {
        lock.lock()
        defer { lock.unlock() }
        return ended
    }

    func run() async throws -> Launched {
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Launched, Error>) in
            if let reason = reason() {
                done.resume(throwing: reason)
                return
            }
            let exited = DispatchGroup(), drained = DispatchGroup()
            // Set before launch, so an exit during draining is never missed.
            exited.enter()
            process.terminationHandler = { _ in exited.leave() }
            do {
                try process.run()
            } catch {
                done.resume(throwing: error)
                return
            }
            if reason() != nil, process.isRunning { process.terminate() }
            let timer = DispatchWorkItem { [self] in end(.timedOut(timeout)) }
            DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(max(timeout, 1)), execute: timer)
            for (slot, pipe) in [out, err].enumerated() {
                drained.enter()
                DispatchQueue.global().async { [self] in drain(pipe, into: slot); drained.leave() }
            }
            exited.notify(queue: .global()) { [self] in
                timer.cancel()
                // A background child can hold the pipes open for as long as it runs, and is not this result.
                _ = drained.wait(timeout: .now() + .seconds(1))
                lock.lock()
                let stopped = ended, stdout = collected[0], stderr = collected[1]
                lock.unlock()
                if let stopped { done.resume(throwing: stopped) } else { done.resume(returning: Launched(status: process.terminationStatus, stdout: stdout, stderr: stderr)) }
            }
        }
    }
}
