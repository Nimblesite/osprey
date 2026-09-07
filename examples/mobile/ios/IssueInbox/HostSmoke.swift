import Foundation

// Implements [MOBILE-HOST-VERIFICATION]: real C ABI + SQLite, isolated from the user's cache.
@MainActor
enum HostSmoke {
    private static let fixture = """
    [
      {"id":101,"number":42,"title":"First issue","html_url":"https://github.com/swiftlang/swift/issues/42","user":{"login":"alice"},"comments":3},
      {"id":102,"number":43,"title":"Second issue","user":{"login":"bob"},"comments":0},
      {"id":103,"number":44,"title":"A pull request","user":{"login":"carol"},"comments":0,"pull_request":{}}
    ]
    """

    static func run() async {
        let result: String
        do {
            try await verify()
            result = "OSPREY_INBOX_SMOKE_OK\n"
        } catch { result = "FAIL: \(error.localizedDescription)\n" }
        do {
            let directory = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask,
                                                        appropriateFor: nil, create: true)
            try result.write(to: directory.appendingPathComponent("inbox-smoke-result.txt"), atomically: true, encoding: .utf8)
        } catch { print("Could not write inbox smoke result: \(error)") }
        print(result, terminator: "")
    }

    private static func verify() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("inbox-smoke-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("inbox.sqlite")
        try seedProbe(url)
        try verifyProbe(url)
        try await verifyApplication(url)
        let rejected = await HTTPTransport.get(id: "unsafe", url: "http://example.invalid")
        try require(rejected["status"] as? Int == 0 && !(rejected["error"] as? String ?? "").isEmpty,
                    "HTTP transport accepted a non-HTTPS URL")
    }

    private static func seedProbe(_ url: URL) throws {
        let database = try SQLiteStore(url: url)
        _ = try database.execute("CREATE TABLE host_probe (text_value TEXT, int_value INTEGER, real_value REAL, bool_value INTEGER, null_value TEXT)")
        _ = try database.execute("INSERT INTO host_probe VALUES (?, ?, ?, ?, ?)", params: [
            .text("O'Reilly 🦉\u{0}; DROP TABLE host_probe;"), .integer(Int64.max), .real(1.25), .boolean(true), .null
        ])
    }

    private static func verifyProbe(_ url: URL) throws {
        let database = try SQLiteStore(url: url)
        let rows = try database.execute("SELECT * FROM host_probe")
        try require(rows.count == 1, "SQLite did not preserve the committed row after reopen")
        try require(rows[0]["text_value"] as? String == "O'Reilly 🦉\u{0}; DROP TABLE host_probe;", "SQLite text binding changed bytes")
        try require(rows[0]["int_value"] as? Int64 == Int64.max && rows[0]["real_value"] as? Double == 1.25,
                    "SQLite numeric binding changed values")
        try require(rows[0]["bool_value"] as? Int64 == 1 && rows[0]["null_value"] is NSNull, "SQLite boolean/null binding failed")
        try require((try? database.execute("SELECT 1; DROP TABLE host_probe")) == nil, "SQLite accepted multiple statements")
        try require((try? database.execute("SELECT ?")) == nil, "SQLite accepted missing parameter bindings")
    }

    private static func verifyApplication(_ url: URL) async throws {
        let services = try HostServices(databaseURL: url) { id, _ in
            ["type": "http", "id": id, "status": 200, "body": fixture, "error": ""]
        }
        let store = InboxStore(services: services)
        store.start()
        await store.waitUntilIdle()
        try require(store.error.isEmpty && store.ui != nil, "Osprey startup/envelope failed: \(store.error)")
        try require(store.snapshot.total == 2 && store.snapshot.items.count == 2, "HTTP issue decoding or pull-request exclusion failed")
        try await verifyReactiveEvents(store)
        try await verifyCache(url)
    }

    private static func verifyReactiveEvents(_ store: InboxStore) async throws {
        store.send(["type": "search", "value": "ALICE"])
        try require(store.snapshot.items.map(\.id) == ["101"], "Osprey search did not react to author input")
        store.send(["type": "search", "value": ""])
        store.send(["type": "bookmark", "id": "101"])
        store.send(["type": "filter", "value": "saved"])
        await store.waitUntilIdle()
        try require(store.error.isEmpty, "Osprey persistence failed: \(store.error)")
        try require(store.snapshot.items.map(\.id) == ["101"] && store.snapshot.items[0].bookmarked,
                    "Osprey saved filter did not react to bookmark input")
        try require(store.snapshot.bookmarked == 1, "Osprey bookmark total is wrong")
    }

    private static func verifyCache(_ url: URL) async throws {
        var requests = 0
        let services = try HostServices(databaseURL: url) { id, _ in
            requests += 1
            return ["type": "http", "id": id, "status": 403, "body": "{\"message\":\"API rate limit exceeded\"}", "error": ""]
        }
        let store = InboxStore(services: services)
        store.start()
        await store.waitUntilIdle()
        try require(requests == 0 && store.error.isEmpty, "Cached startup unexpectedly required HTTP")
        try require(store.snapshot.total == 2 && store.snapshot.bookmarked == 1, "Osprey cache lost issues or bookmarks")
        store.send(["type": "refresh"])
        await store.waitUntilIdle()
        try require(requests == 1 && !store.error.isEmpty, "HTTP failure was not exposed to the view")
        try require(store.snapshot.total == 2 && store.snapshot.bookmarked == 1, "HTTP failure destroyed cached state")
    }

    private static func require(_ condition: Bool, _ message: String) throws {
        if !condition { throw HostFailure.invalid(message) }
    }
}
