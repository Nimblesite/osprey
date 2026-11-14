#if os(macOS)
import AppKit
import Foundation

// Implements [UI-EFFECTS]: one executable with arguments, no shell, a deadline, separate
// stdout/stderr and an output limit. The exit status is reported, never interpreted. The
// person's PATH is extended with the usual tool folders, because an app opened from the Finder
// inherits almost none of it. Requests arrive as the typed fields of their ask rows.

/// Shared launch setup for one-shot and long-lived processes ([UI-EFFECTS-PROCESS]).
enum ProcessLaunch {
    /// The environment a child process runs in: the person's environment with the usual tool
    /// folders first, then `extra` variables merged over it.
    static func environment(_ extra: [String: String] = [:]) -> [String: String] {
        var environment = ProcessInfo.processInfo.environment
        let inherited = environment["PATH"].map { ":" + $0 } ?? ""
        environment["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" + inherited
        environment["LANG"] = "en_US.UTF-8"
        for (name, value) in extra { environment[name] = value }
        return environment
    }

    /// A command as an absolute path, or a name found on `environment`'s PATH. A missing command
    /// is a typed error, never a crash.
    static func resolve(_ command: String, in environment: [String: String]) throws -> URL {
        if command.hasPrefix("/") {
            let url = URL(fileURLWithPath: command)
            guard FileManager.default.isExecutableFile(atPath: url.path) else { throw CapabilityError.notAllowed("Command not found: \(command)") }
            return url
        }
        for directory in (environment["PATH"] ?? "").split(separator: ":").map(String.init) {
            let candidate = URL(fileURLWithPath: directory).appendingPathComponent(command)
            if FileManager.default.isExecutableFile(atPath: candidate.path) { return candidate }
        }
        throw CapabilityError.notAllowed("Command not found: \(command)")
    }
}

public struct RunAdapter: CapabilityAdapter {
    public let kinds = ["run"]
    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let executable = try required(ask, text: 1, "executable")
        guard executable.hasPrefix("/") else { throw CapabilityError.notAllowed("Executable must be an absolute path") }
        guard !ask.flag(5) else { throw CapabilityError.notAllowed("This host does not convert property lists") }
        let slice = try required(ask, text: 6, "slice")
        guard !slice.isEmpty else { throw CapabilityError.invalidArgument("run has no valid slice") }
        let launch = Launch(executable: executable, arguments: ask.texts(2),
                            timeout: try required(ask, whole: 3, "timeout"), limit: try required(ask, whole: 4, "limit"))
        let run = try await withTaskCancellationHandler { try await launch.run() } onCancel: { launch.stop() }
        slices.write(slice, String(decoding: run.stdout, as: UTF8.self))
        return .document(["status": .number(Double(run.status)), "stderr": .string(String(decoding: run.stderr.prefix(65536), as: UTF8.self))])
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
        process.environment = ProcessLaunch.environment()
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

/// One long-lived child process ([UI-EFFECTS-PROCESS]): standard input stays open, standard output
/// and error accumulate in separate bounded buffers, and a read returns the output so far within a
/// byte limit and a deadline, never waiting longer.
final class ChildProcess: @unchecked Sendable {
    /// How long a process's last output may take to arrive after it exits. A grandchild holding the pipes open must not hold back `exited` for ever.
    private static let drainGrace = DispatchTimeInterval.seconds(1)
    /// The longest UTF-8 character, so a piece of any limit can hold a whole one.
    private static let smallestPiece = 4
    private let process = Process()
    private let stdin: FileHandle
    private let out = Pipe(), err = Pipe()
    private let lock = NSLock()
    private let drains = DispatchGroup()
    /// A write blocks until the child reads, so each child writes on its own queue and holds up nothing else.
    private let writes = DispatchQueue(label: "org.ospreylang.process-write")
    private var stdout = Data()
    private var stderr = Data()
    private var ended = false
    private var inputClosed = false
    private var status: Int32 = 0
    private let cap: Int

    init(command: URL, arguments: [String], environment: [String: String], cap: Int) {
        let input = Pipe()
        process.executableURL = command
        process.arguments = arguments
        process.environment = environment
        process.standardInput = input
        process.standardOutput = out
        process.standardError = err
        stdin = input.fileHandleForWriting
        // A write to a process that went away is an error for that write, not a signal that ends this app.
        _ = fcntl(stdin.fileDescriptor, F_SETNOSIGPIPE, 1)
        self.cap = cap
    }

    /// Launches the child and starts both drains; the exit handler is set first so an early exit is never missed.
    func launch() throws {
        drain(out, isStderr: false)
        drain(err, isStderr: true)
        process.terminationHandler = { [weak self] _ in
            guard let self else { return }
            DispatchQueue.global().async {
                _ = self.drains.wait(timeout: .now() + Self.drainGrace)
                self.noteExit()
            }
        }
        try process.run()
    }

    var isRunning: Bool {
        lock.lock()
        defer { lock.unlock() }
        return !ended
    }

    var isAlive: Bool { process.isRunning }

    var processIdentifier: Int32 { process.processIdentifier }

    private func noteExit() {
        lock.lock()
        ended = true
        status = process.terminationStatus
        lock.unlock()
    }

    private func drain(_ pipe: Pipe, isStderr: Bool) {
        let handle = pipe.fileHandleForReading
        drains.enter()
        DispatchQueue.global().async { [weak self] in
            defer { self?.drains.leave() }
            while true {
                let block = handle.availableData
                if block.isEmpty { return }
                self?.append(block, isStderr: isStderr)
            }
        }
    }

    private func append(_ block: Data, isStderr: Bool) {
        lock.lock()
        defer { lock.unlock() }
        if isStderr {
            stderr.append(block)
            if stderr.count > cap { stderr = Data(stderr.suffix(cap)) }
        } else {
            stdout.append(block)
            if stdout.count > cap { stdout = Data(stdout.suffix(cap)) }
        }
    }

    /// Writes to the child's standard input; a write to an exited process is a typed error.
    func write(_ bytes: Data) async throws -> Int {
        guard isRunning else { throw CapabilityError.platform("The process \(process.processIdentifier) has exited") }
        return try await withCheckedThrowingContinuation { (written: CheckedContinuation<Int, Error>) in
            writes.async { [self] in
                do {
                    guard !inputClosed else { throw CapabilityError.platform("The process \(process.processIdentifier) has stopped") }
                    try stdin.write(contentsOf: bytes)
                    written.resume(returning: bytes.count)
                } catch let error as CapabilityError {
                    written.resume(throwing: error)
                } catch {
                    written.resume(throwing: CapabilityError.platform(error.localizedDescription))
                }
            }
        }
    }

    /// Reads whole characters of stdout and stderr produced so far, up to `limit` bytes each, waiting only
    /// until `timeout`. The wait holds no thread, so any number of servers can be waiting at once.
    func read(limit: Int, timeout: Duration) async throws -> (output: Data, stderr: Data, ended: Bool, status: Int32) {
        let deadline = ContinuousClock.now + timeout
        while true {
            let piece = take(limit: max(limit, Self.smallestPiece))
            if !piece.0.isEmpty || !piece.1.isEmpty || piece.2 || ContinuousClock.now >= deadline { return piece }
            try await Task.sleep(for: .milliseconds(10))
        }
    }

    private func take(limit: Int) -> (Data, Data, Bool, Int32) {
        lock.lock()
        defer { lock.unlock() }
        let output = stdout.prefix(wholeCharacters(stdout, limit: limit))
        let errors = stderr.prefix(wholeCharacters(stderr, limit: limit))
        stdout.removeFirst(output.count)
        stderr.removeFirst(errors.count)
        // `exited` is the last thing a reader sees: after it, nothing more will come.
        return (output, errors, ended && stdout.isEmpty && stderr.isEmpty, status)
    }

    /// Puts back what a read took but nobody received, ahead of anything that arrived since.
    func unread(output: Data, stderr errors: Data) {
        lock.lock()
        defer { lock.unlock() }
        stdout = output + stdout
        stderr = errors + stderr
    }

    /// How many leading bytes, at most `limit`, end on a character boundary. Once the process has ended
    /// nothing can complete a cut character at the very end, so that passes as it is.
    private func wholeCharacters(_ bytes: Data, limit: Int) -> Int {
        let size = min(limit, bytes.count)
        if size == 0 || (ended && size == bytes.count) { return size }
        var last = size - 1
        while last > 0, size - last < Self.smallestPiece, bytes[bytes.startIndex + last] & 0xC0 == 0x80 { last -= 1 }
        let lead = bytes[bytes.startIndex + last]
        let length = lead >= 0xF0 ? 4 : lead >= 0xE0 ? 3 : lead >= 0xC0 ? 2 : 1
        return size - last < length ? last : size
    }

    /// Asks the child to end: closes its input, as the MCP specification's stdio shutdown begins, then
    /// sends terminate. A child that ignores both is killed after a grace period.
    func stop() {
        writes.async { [self] in
            guard !inputClosed else { return }
            inputClosed = true
            try? stdin.close()
        }
        if process.isRunning { process.terminate() }
        DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(2)) { [weak self] in self?.kill9() }
    }

    func kill9() {
        if process.isRunning { kill(process.processIdentifier, SIGKILL) }
    }
}

/// Keeps every long-lived child process under its handle and stops them all on quit ([UI-EFFECTS-PROCESS]).
final class ProcessPool: @unchecked Sendable {
    static let shared = ProcessPool()
    static let bufferCap = 1_000_000
    static let quitGrace: TimeInterval = 0.5
    private let lock = NSLock()
    private var children: [String: ChildProcess] = [:]
    private var observingQuit = false

    private init() {}

    private func observeQuit() {
        lock.lock()
        let already = observingQuit
        observingQuit = true
        lock.unlock()
        guard !already else { return }
        NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
            self?.stopAll()
        }
    }

    func start(name: String, command: String, arguments: [String], env: [String: String]) throws -> Int32 {
        observeQuit()
        let environment = ProcessLaunch.environment(env)
        let url = try ProcessLaunch.resolve(command, in: environment)
        let child = ChildProcess(command: url, arguments: arguments, environment: environment, cap: ProcessPool.bufferCap)
        try child.launch()
        lock.lock()
        let previous = children[name]
        children[name] = child
        lock.unlock()
        previous?.stop()
        return child.processIdentifier
    }

    func write(name: String, bytes: Data) async throws -> Int {
        guard let child = child(named: name) else { throw CapabilityError.invalidArgument("No process named \(name)") }
        return try await child.write(bytes)
    }

    func read(name: String, limit: Int, timeout: Duration) async throws -> (output: Data, stderr: Data, ended: Bool, status: Int32) {
        guard let child = child(named: name) else { throw CapabilityError.invalidArgument("No process named \(name)") }
        return try await child.read(limit: limit, timeout: timeout)
    }

    func unread(name: String, output: Data, stderr: Data) {
        child(named: name)?.unread(output: output, stderr: stderr)
    }

    /// Stops one process; unknown or already-exited handles are a harmless false.
    func stop(name: String) -> Bool {
        lock.lock()
        let child = children.removeValue(forKey: name)
        lock.unlock()
        guard let child, child.isRunning else { return false }
        child.stop()
        return true
    }

    /// Stops every process, for quit ([UI-EFFECTS-PROCESS]). The app is ending, so no later timer can
    /// finish the job: this waits briefly, then kills whatever is left, before it returns.
    func stopAll() {
        lock.lock()
        let all = Array(children.values)
        children.removeAll()
        lock.unlock()
        all.forEach { $0.stop() }
        let deadline = Date().addingTimeInterval(ProcessPool.quitGrace)
        while Date() < deadline, all.contains(where: \.isAlive) { Thread.sleep(forTimeInterval: 0.01) }
        all.forEach { $0.kill9() }
        while all.contains(where: \.isAlive), Date() < deadline.addingTimeInterval(ProcessPool.quitGrace) { Thread.sleep(forTimeInterval: 0.01) }
    }

    private func child(named name: String) -> ChildProcess? {
        lock.lock()
        defer { lock.unlock() }
        return children[name]
    }
}

/// The four long-lived process capabilities for MCP stdio servers ([UI-EFFECTS-PROCESS]).
public struct ProcessAdapter: CapabilityAdapter {
    public let kinds = ["process-start", "process-write", "process-read", "process-stop"]
    public init() {}

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        switch ask.kind {
        case "process-start":
            let name = try required(ask, text: 4, "name")
            let pid = try ProcessPool.shared.start(name: name, command: try required(ask, text: 1, "command"),
                                                   arguments: ask.texts(2), env: Dictionary(ask.pairs(3), uniquingKeysWith: { _, later in later }))
            return .document(["pid": .number(Double(pid))])
        case "process-write":
            let written = try await ProcessPool.shared.write(name: try required(ask, text: 1, "name"), bytes: Data(try required(ask, text: 2, "input").utf8))
            return .document(["written": .number(Double(written))])
        case "process-read":
            let timeout = max(try required(ask, whole: 3, "deadline"), 0)
            let read = try await ProcessPool.shared.read(name: try required(ask, text: 1, "name"), limit: try required(ask, whole: 2, "limit"), timeout: .milliseconds(timeout))
            return .document(["output": .string(String(decoding: read.output, as: UTF8.self)), "stderr": .string(String(decoding: read.stderr, as: UTF8.self)), "exited": .bool(read.ended), "status": .number(Double(read.status))])
        default:
            return .document(["stopped": .bool(ProcessPool.shared.stop(name: try required(ask, text: 1, "name")))])
        }
    }

    /// A read replaced just after it took output: those bytes belong to the read that replaced it.
    public func discarded(_ ask: Ask, _ value: AskResult) {
        guard ask.kind == "process-read", case let .document(document) = value,
              let name = try? required(ask, text: 1, "name"),
              case let .string(output)? = document.object?["output"], case let .string(stderr)? = document.object?["stderr"] else { return }
        ProcessPool.shared.unread(name: name, output: Data(output.utf8), stderr: Data(stderr.utf8))
    }
}

#endif
