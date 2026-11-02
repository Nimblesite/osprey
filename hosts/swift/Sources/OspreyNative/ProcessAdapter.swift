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

    /// Ends the child and remembers why, so its result is reported as that reason.
    private func end(_ reason: CapabilityError) {
        lock.lock()
        if ended == nil { ended = reason }
        lock.unlock()
        if process.isRunning { process.terminate() }
    }

    func stop() { end(.cancelled) }

    private func drain(_ pipe: Pipe) -> Data {
        var collected = Data()
        let handle = pipe.fileHandleForReading
        while true {
            let block = handle.availableData
            if block.isEmpty { return collected }
            collected.append(block)
            if collected.count > limit { end(.tooLarge(collected.count, limit)) }
        }
    }

    func run() async throws -> Launched {
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Launched, Error>) in
            let group = DispatchGroup()
            var stdout = Data(), stderr = Data()
            // Set before launch, so an exit during draining is never missed.
            group.enter()
            process.terminationHandler = { _ in group.leave() }
            do {
                try process.run()
            } catch {
                done.resume(throwing: error)
                return
            }
            let timer = DispatchWorkItem { [self] in end(.timedOut(timeout)) }
            DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(max(timeout, 1)), execute: timer)
            group.enter()
            DispatchQueue.global().async { [self] in stdout = drain(out); group.leave() }
            group.enter()
            DispatchQueue.global().async { [self] in stderr = drain(err); group.leave() }
            group.notify(queue: .global()) { [self] in
                timer.cancel()
                lock.lock()
                let reason = ended
                lock.unlock()
                if let reason { done.resume(throwing: reason) } else { done.resume(returning: Launched(status: process.terminationStatus, stdout: stdout, stderr: stderr)) }
            }
        }
    }
}
