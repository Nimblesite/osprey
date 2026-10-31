import Foundation

// Implements [UI-EFFECTS] and [SECURITY-COLLECTORS]: one executable with
// arguments, no shell, a fixed minimal environment, a deadline, separate
// stdout/stderr and an output limit. With `plist`, stdout is converted
// mechanically to JSON. The exit status is reported, never interpreted.

public struct RunAdapter: CapabilityAdapter {
    public let kinds = ["run"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        #if os(macOS)
        let executable = try request.text("executable")
        guard executable.hasPrefix("/") else { throw CapabilityError.notAllowed("Executable must be an absolute path") }
        let plist = try request.members["plist"].map { _ in try request.flag("plist") } ?? false
        let slice = try request.text("slice")
        let run = try await Launch(executable: executable, arguments: try request.texts("arguments"), timeout: try request.number("timeout"), limit: try request.number("limit")).run()
        let output = plist ? try RunAdapter.plistJSON(run.stdout) : try Bounded.text(run.stdout)
        slices.write(slice, output)
        return .object(["status": .number(Double(run.status)), "stderr": .string(String(decoding: run.stderr.prefix(65536), as: UTF8.self))])
        #else
        throw CapabilityError.platform("Processes cannot be launched on this platform")
        #endif
    }

    static func plistJSON(_ data: Data) throws -> String {
        guard !data.isEmpty else { return "null" }
        let raw = try PropertyListSerialization.propertyList(from: data, format: nil)
        guard let json = PropertyLists.json(raw) else { throw CapabilityError.invalidArgument("Unsupported property-list value") }
        return json.text
    }
}

#if os(macOS)
struct Launched {
    let status: Int32
    let stdout: Data
    let stderr: Data
}

/// Collects both pipes concurrently and enforces the deadline and output limit.
final class Launch: @unchecked Sendable {
    private let process = Process()
    private let out = Pipe(), err = Pipe()
    private let timeout: Int, limit: Int
    private var expired = false
    private let exited: AsyncStream<Void>

    init(executable: String, arguments: [String], timeout: Int, limit: Int) {
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = out
        process.standardError = err
        self.timeout = timeout
        self.limit = limit
        let (stream, signal) = AsyncStream<Void>.makeStream()
        exited = stream
        // Set before launch, so an exit during draining is never missed.
        process.terminationHandler = { _ in signal.finish() }
    }

    func run() async throws -> Launched {
        try process.run()
        let deadline = Task { [self] in
            try await Task.sleep(nanoseconds: UInt64(max(timeout, 1)) * 1_000_000)
            expired = true
            if process.isRunning { process.terminate() }
        }
        defer { deadline.cancel() }
        async let stdout = drain(out)
        async let stderr = drain(err)
        let (output, errors) = try await (stdout, stderr)
        for await _ in exited {}
        if Task.isCancelled { throw CancellationError() }
        if expired { throw CapabilityError.timedOut(timeout) }
        return Launched(status: process.terminationStatus, stdout: output, stderr: errors)
    }

    private func drain(_ pipe: Pipe) async throws -> Data {
        var collected = Data()
        for try await chunk in pipe.chunks {
            collected.append(chunk)
            if collected.count > limit { process.terminate(); throw CapabilityError.tooLarge(collected.count, limit) }
            if Task.isCancelled { process.terminate(); throw CancellationError() }
        }
        return collected
    }
}

extension Pipe {
    /// Each pipe owns its readiness callback; AsyncBytes can serialize reads across handles.
    var chunks: AsyncThrowingStream<Data, Error> {
        let reader = fileHandleForReading
        return AsyncThrowingStream { continuation in
            reader.readabilityHandler = { handle in
                do {
                    let block = try handle.read(upToCount: 65536) ?? Data()
                    if block.isEmpty { handle.readabilityHandler = nil; continuation.finish() }
                    else { continuation.yield(block) }
                } catch {
                    handle.readabilityHandler = nil
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in reader.readabilityHandler = nil }
        }
    }
}
#endif
