import Foundation
import SQLite3

// Implements [MOBILE-HOST-SERVICES]: platform transport; SQL and URLs come from Osprey.
enum HostFailure: LocalizedError {
    case invalid(String)
    var errorDescription: String? { if case let .invalid(message) = self { return message } }
}

enum SQLScalar: Decodable {
    case null, boolean(Bool), integer(Int64), real(Double), text(String)

    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let x = try? value.decode(Bool.self) { self = .boolean(x) }
        else if let x = try? value.decode(Int64.self) { self = .integer(x) }
        else if let x = try? value.decode(Double.self), x.isFinite { self = .real(x) }
        else if let x = try? value.decode(String.self) { self = .text(x) }
        else { throw HostFailure.invalid("SQL parameters must be finite JSON scalars") }
    }
}

struct HostCommand: Decodable {
    let kind: String
    let id: String
    let sql: String?
    let params: [SQLScalar]?
    let url: String?
}

final class SQLiteStore {
    private var database: OpaquePointer?
    private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

    init(url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let result = sqlite3_open_v2(url.path, &database, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX, nil)
        guard result == SQLITE_OK else {
            let failure = problem()
            sqlite3_close(database)
            database = nil
            throw failure
        }
        sqlite3_busy_timeout(database, 3_000)
    }

    deinit { sqlite3_close(database) }

    func execute(_ sql: String, params: [SQLScalar] = []) throws -> [[String: Any]] {
        let statement = try prepare(sql)
        defer { sqlite3_finalize(statement) }
        guard sqlite3_bind_parameter_count(statement) == params.count else {
            throw HostFailure.invalid("SQL parameter count does not match the prepared statement")
        }
        for (index, value) in params.enumerated() { try bind(value, at: Int32(index + 1), to: statement) }
        return try rows(statement)
    }

    private func prepare(_ sql: String) throws -> OpaquePointer {
        guard !sql.utf8.contains(0) else { throw HostFailure.invalid("SQL contains a NUL byte") }
        return try sql.withCString { source in
            var statement: OpaquePointer?
            var tail: UnsafePointer<CChar>?
            guard sqlite3_prepare_v2(database, source, -1, &statement, &tail) == SQLITE_OK else { throw problem() }
            guard let statement else { throw HostFailure.invalid("SQL command contains no statement") }
            if let tail, !String(cString: tail).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                sqlite3_finalize(statement)
                throw HostFailure.invalid("SQL commands must contain exactly one statement")
            }
            return statement
        }
    }

    private func bind(_ value: SQLScalar, at index: Int32, to statement: OpaquePointer) throws {
        let result: Int32
        switch value {
        case .null: result = sqlite3_bind_null(statement, index)
        case let .boolean(value): result = sqlite3_bind_int64(statement, index, value ? 1 : 0)
        case let .integer(value): result = sqlite3_bind_int64(statement, index, value)
        case let .real(value): result = sqlite3_bind_double(statement, index, value)
        case let .text(value):
            guard let size = Int32(exactly: value.utf8.count) else { throw HostFailure.invalid("SQL text parameter is too large") }
            result = sqlite3_bind_text(statement, index, value, size, transient)
        }
        guard result == SQLITE_OK else { throw problem() }
    }

    private func rows(_ statement: OpaquePointer) throws -> [[String: Any]] {
        var result: [[String: Any]] = []
        var status = sqlite3_step(statement)
        while status == SQLITE_ROW {
            result.append(try row(statement))
            status = sqlite3_step(statement)
        }
        guard status == SQLITE_DONE else { throw problem() }
        return result
    }

    private func row(_ statement: OpaquePointer) throws -> [String: Any] {
        var result: [String: Any] = [:]
        for index in 0..<sqlite3_column_count(statement) {
            guard let pointer = sqlite3_column_name(statement, index) else { throw problem() }
            let name = String(cString: pointer)
            guard result[name] == nil else { throw HostFailure.invalid("SQL returned duplicate column name: \(name)") }
            result[name] = try column(statement, index)
        }
        return result
    }

    private func column(_ statement: OpaquePointer, _ index: Int32) throws -> Any {
        switch sqlite3_column_type(statement, index) {
        case SQLITE_NULL: return NSNull()
        case SQLITE_INTEGER: return sqlite3_column_int64(statement, index)
        case SQLITE_FLOAT:
            let value = sqlite3_column_double(statement, index)
            guard value.isFinite else { throw HostFailure.invalid("SQL returned a non-finite number") }
            return value
        case SQLITE_TEXT:
            guard let pointer = sqlite3_column_text(statement, index) else { throw problem() }
            let data = Data(bytes: pointer, count: Int(sqlite3_column_bytes(statement, index)))
            guard let value = String(data: data, encoding: .utf8) else { throw HostFailure.invalid("SQL returned invalid UTF-8") }
            return value
        default: throw HostFailure.invalid("SQL blob columns cannot be returned as JSON scalars")
        }
    }

    private func problem() -> HostFailure {
        .invalid(database.map { String(cString: sqlite3_errmsg($0)) } ?? "SQLite database could not be opened")
    }
}

@MainActor
final class HostServices {
    typealias HTTPHandler = (String, String) async -> [String: Any]
    let database: SQLiteStore
    private let http: HTTPHandler

    init(databaseURL: URL? = nil, http: @escaping HTTPHandler = HTTPTransport.get) throws {
        let directory = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                    appropriateFor: nil, create: true)
        database = try SQLiteStore(url: databaseURL ?? directory.appendingPathComponent("issue-inbox.sqlite"))
        self.http = http
    }

    func execute(_ command: HostCommand) async -> [String: Any] {
        if command.kind == "http", let url = command.url { return await http(command.id, url) }
        do {
            guard command.kind == "sql", let sql = command.sql, let params = command.params else {
                throw HostFailure.invalid("Invalid host command: \(command.kind)")
            }
            return ["type": "sql", "id": command.id, "ok": true,
                    "rows": try database.execute(sql, params: params), "error": ""]
        } catch {
            return ["type": command.kind, "id": command.id, "ok": false, "rows": [], "error": error.localizedDescription]
        }
    }
}

@MainActor
enum HTTPTransport {
    private static let maximumBytes = 2 * 1_024 * 1_024

    static func get(id: String, url: String) async -> [String: Any] {
        var status = 0
        do {
            let request = try request(url)
            let session = URLSession(configuration: configuration())
            defer { session.invalidateAndCancel() }
            let (bytes, response) = try await session.bytes(for: request)
            guard let response = response as? HTTPURLResponse else { throw HostFailure.invalid("HTTP response was not HTTP") }
            status = response.statusCode
            let body = try await read(bytes, expected: response.expectedContentLength)
            return ["type": "http", "id": id, "status": status, "body": body, "error": ""]
        } catch {
            return ["type": "http", "id": id, "status": status, "body": "", "error": error.localizedDescription]
        }
    }

    private static func request(_ address: String) throws -> URLRequest {
        guard let url = URL(string: address), url.scheme == "https", url.host != nil,
              url.user == nil, url.password == nil else { throw HostFailure.invalid("HTTP commands require an HTTPS URL") }
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 20)
        request.httpMethod = "GET"
        request.setValue("Osprey-Issue-Inbox", forHTTPHeaderField: "User-Agent")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        return request
    }

    private static func configuration() -> URLSessionConfiguration {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 20
        configuration.timeoutIntervalForResource = 30
        return configuration
    }

    private static func read(_ bytes: URLSession.AsyncBytes, expected: Int64) async throws -> String {
        guard expected <= maximumBytes else { throw HostFailure.invalid("HTTP response exceeds the 2 MiB limit") }
        var data = Data()
        for try await byte in bytes {
            guard data.count < maximumBytes else { throw HostFailure.invalid("HTTP response exceeds the 2 MiB limit") }
            data.append(byte)
        }
        guard let body = String(data: data, encoding: .utf8) else { throw HostFailure.invalid("HTTP response is not valid UTF-8") }
        return body
    }
}
