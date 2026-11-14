import Foundation
import OSLog

// [ORCHESTRA-DIAGNOSTICS] One bounded sink for controller, transport, session and device events.
final class OrchestraDiagnostics {
    private let queue = DispatchQueue(label: "org.ospreylang.orchestra.diagnostics", qos: .utility)
    private let lock = NSLock()
    private let system = Logger(subsystem: "dev.ospreylang.orchestra", category: "diagnostics")
    private let sessionID = UUID().uuidString
    private var pending = 0
    private var dropped = 0
    private var file: FileHandle?
    private var path: URL?
    private var bytes = 0

    func start() throws {
        let directory = ProcessInfo.processInfo.environment["ORCHESTRA_LOG_DIRECTORY"].map(URL.init(fileURLWithPath:))
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/Osprey/LightOrchestra", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        let url = directory.appendingPathComponent("session-\(ProcessInfo.processInfo.processIdentifier).jsonl")
        path = url
        try openFile(url)
        record("logging.started", ["path": url.path])
    }

    private func openFile(_ url: URL) throws {
        guard FileManager.default.createFile(atPath: url.path, contents: nil, attributes: [.posixPermissions: 0o600]) else {
            throw problem("Cannot create diagnostic log")
        }
        file = try FileHandle(forWritingTo: url)
        bytes = 0
    }

    func record(_ event: String, _ fields: [String: Any]) {
        lock.lock()
        guard pending < 256 else { dropped += 1; lock.unlock(); return }
        pending += 1
        let omitted = dropped
        dropped = 0
        lock.unlock()
        let timestamp = Date().timeIntervalSince1970
        queue.async {
            defer { self.lock.lock(); self.pending -= 1; self.lock.unlock() }
            var record = fields
            record["event"] = event
            record["timestamp"] = timestamp
            record["session"] = self.sessionID
            record["component"] = event.hasPrefix("controller.") ? "controller" : "backend"
            if omitted > 0 { record["dropped"] = omitted }
            self.write(record)
        }
    }

    private func write(_ record: [String: Any]) {
        do {
            let encoded = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
            guard let raw = String(data: encoded, encoding: .utf8) else { throw problem("Invalid diagnostic encoding") }
            let line = raw.replacingOccurrences(of: token, with: "[redacted]")
            let data = Data((line + "\n").utf8)
            system.info("\(line, privacy: .public)")
            FileHandle.standardOutput.write(data)
            if bytes + data.count > 2 * 1024 * 1024 { try rotate() }
            try file?.write(contentsOf: data)
            bytes += data.count
        } catch {
            system.error("Diagnostic persistence failed: \(error.localizedDescription, privacy: .public)")
            FileHandle.standardError.write(Data("Diagnostic persistence failed: \(error.localizedDescription)\n".utf8))
        }
    }

    private func rotate() throws {
        guard let path else { return }
        try file?.close()
        file = nil
        let previous = path.appendingPathExtension("previous")
        if FileManager.default.fileExists(atPath: previous.path) { try FileManager.default.removeItem(at: previous) }
        try FileManager.default.moveItem(at: path, to: previous)
        try openFile(path)
    }

    func flush() { queue.sync { } }
}
