import Foundation
import XCTest

@testable import OspreyShared

// A real folder is a maze the walk must not flinch at [UI-EFFECTS-MEASURE]: hard links that
// would count twice, links to large files whose bytes must never arrive, hidden corners,
// folders nobody may read, more children than the limit and a deadline already past. Each
// test builds its tree in a temporary folder, asks with the fields `Requests::fields` numbers,
// and asserts every field of the slice the host writes.
final class MeasureAdapterTests: RenderingTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("osprey-measure-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.setAttributes([.posixPermissions: 0o755],
                                               ofItemAtPath: root.appendingPathComponent("locked").path)
        try? FileManager.default.removeItem(at: root)
    }

    /// A measure-folders ask: folders at 1, the entry limit at 2, the deadline at 3, the slice at 4.
    private func ask(_ folders: [String], limit: Int, deadline: Int) -> Ask {
        var fields = folders.map { AskField(field: 1, number: 0, text: $0) }
        fields.append(AskField(field: 2, number: Int64(limit), text: ""))
        fields.append(AskField(field: 3, number: Int64(deadline), text: ""))
        fields.append(AskField(field: 4, number: 0, text: "measured"))
        return Ask(code: 16, ticket: "t", fields: fields)
    }

    /// Runs the walk and returns the slice's first folder, parsed as the application parses it.
    private func folder(_ request: Ask) async throws -> [String: Any] {
        let slices = SliceStore()
        let result = try await MeasureAdapter().perform(request, slices: slices)
        XCTAssertEqual(result, .document(.null))
        let text = slices.text("measured")
        let held = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        return try XCTUnwrap((held["folders"] as? [[String: Any]])?.first)
    }

    /// The maze: nested folders, a hidden file, a hidden folder holding bytes, a hard link, a
    /// link to a large file and a folder nobody may read.
    private func buildTree() throws {
        write(Data(repeating: 65, count: 4096), at: "a.txt")
        write(Data(repeating: 66, count: 1_048_576), at: "big.bin")
        write(Data(repeating: 67, count: 8192), at: ".secret")
        write(Data(repeating: 68, count: 2048), at: ".stash/buried.txt")
        write(Data(repeating: 69, count: 4096), at: "sub/inner.txt")
        write(Data(repeating: 70, count: 4096), at: "sub/deeper/leaf.txt")
        try FileManager.default.createSymbolicLink(atPath: root.appendingPathComponent("to-big").path,
                                                   withDestinationPath: root.appendingPathComponent("big.bin").path)
        try FileManager.default.linkItem(at: root.appendingPathComponent("a.txt"),
                                         to: root.appendingPathComponent("same.txt"))
        try FileManager.default.createDirectory(at: root.appendingPathComponent("locked"),
                                                withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o000],
                                              ofItemAtPath: root.appendingPathComponent("locked").path)
    }

    private func write(_ data: Data, at path: String) {
        let url = root.appendingPathComponent(path)
        try! FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                 withIntermediateDirectories: true)
        try! data.write(to: url)
    }

    /// One child's allocated bytes, in the unit POSIX defines, read as the walk reads them.
    private func allocated(_ path: String) -> Int {
        var status = stat()
        XCTAssertEqual(lstat(root.appendingPathComponent(path).path, &status), 0)
        return Int(Int64(status.st_blocks) * 512)
    }

    private func bytes(_ folder: [String: Any]) -> Int {
        folder["bytes"] as? Int ?? -1
    }

    func testWalkMeasuresEveryFieldOfATangledTree() async throws {
        try buildTree()
        let folder = try await folder(ask([root.path], limit: 20_000, deadline: 600_000))
        XCTAssertEqual(folder["path"] as? String, root.path)
        XCTAssertEqual(folder["error"] as? String, "")
        XCTAssertEqual(folder["missing"] as? Bool, false)
        XCTAssertEqual(folder["truncated"] as? Bool, false)
        XCTAssertEqual(folder["unreadable"] as? Int, 1)
        let a = allocated("a.txt"), big = allocated("big.bin"), secret = allocated(".secret")
        let stash = allocated(".stash") + allocated(".stash/buried.txt")
        let sub = allocated("sub") + allocated("sub/inner.txt") + allocated("sub/deeper")
            + allocated("sub/deeper/leaf.txt")
        // The hard link counts once, the link's target is not counted again, and everything below
        // both folders and the hidden ones is in the total.
        XCTAssertEqual(bytes(folder), a + big + secret + stash + sub)
        // Visible leaves the hidden file and everything below the hidden folder out.
        XCTAssertEqual(folder["visible"] as? Int, a + big + sub)
        // Eight direct children, one below .stash, three below sub.
        XCTAssertEqual(folder["items"] as? Int, 12)
        let volume = try XCTUnwrap(folder["volume"] as? [String: Any])
        XCTAssertFalse((volume["name"] as? String ?? "").isEmpty)
        XCTAssertGreaterThan(volume["total"] as? Int ?? 0, 0)
        XCTAssertGreaterThan(volume["available"] as? Int ?? 0, 0)
        var entries = try XCTUnwrap(folder["entries"] as? [[String: Any]])
        XCTAssertEqual(entries.count, 8)
        var byName = [String: [String: Any]]()
        entries.forEach { byName[$0["name"] as? String ?? ""] = $0 }
        // The link is listed with no bytes and never followed, so its target is counted only once.
        let link = try XCTUnwrap(byName["to-big"])
        XCTAssertEqual(link["link"] as? Bool, true)
        XCTAssertEqual(link["folder"] as? Bool, false)
        XCTAssertEqual(link["bytes"] as? Int, 0)
        // The hard-linked pair carries a.txt's bytes exactly once between them.
        let same = try XCTUnwrap(byName["same.txt"])
        XCTAssertEqual((byName["a.txt"]?["bytes"] as? Int ?? 0) + (same["bytes"] as? Int ?? 0), a)
        XCTAssertEqual(same["link"] as? Bool, false)
        // Hidden items are listed, marked hidden, measured, and count nothing visible.
        let hiddenFile = try XCTUnwrap(byName[".secret"])
        XCTAssertEqual(hiddenFile["hidden"] as? Bool, true)
        XCTAssertEqual(hiddenFile["visible"] as? Int, 0)
        XCTAssertEqual(hiddenFile["bytes"] as? Int, secret)
        let stashEntry = try XCTUnwrap(byName[".stash"])
        XCTAssertEqual(stashEntry["hidden"] as? Bool, true)
        XCTAssertEqual(stashEntry["folder"] as? Bool, true)
        XCTAssertEqual(stashEntry["items"] as? Int, 1)
        XCTAssertEqual(stashEntry["bytes"] as? Int, stash)
        XCTAssertEqual(stashEntry["visible"] as? Int, 0)
        // The sub folder carries its whole subtree.
        let subEntry = try XCTUnwrap(byName["sub"])
        XCTAssertEqual(subEntry["folder"] as? Bool, true)
        XCTAssertEqual(subEntry["items"] as? Int, 3)
        XCTAssertEqual(subEntry["bytes"] as? Int, sub)
        // The unreadable folder is listed guarded, with nothing measured below it.
        let guarded = try XCTUnwrap(byName["locked"])
        XCTAssertEqual(guarded["guarded"] as? Bool, true)
        XCTAssertEqual(guarded["bytes"] as? Int, 0)
        XCTAssertEqual(guarded["items"] as? Int, 0)
        // The entries array is replaced, not appended, by a fresh walk of the same folder.
        let again = try await self.folder(ask([root.path], limit: 20_000, deadline: 600_000))
        entries = try XCTUnwrap(again["entries"] as? [[String: Any]])
        byName = [:]
        entries.forEach { byName[$0["name"] as? String ?? ""] = $0 }
        XCTAssertEqual(byName.count, 8)
    }

    func testMoreChildrenThanTheLimitAreMeasuredButNotListed() async throws {
        try buildTree()
        let folder = try await folder(ask([root.path], limit: 3, deadline: 600_000))
        XCTAssertEqual(folder["truncated"] as? Bool, true)
        XCTAssertEqual((folder["entries"] as? [[String: Any]])?.count, 3)
        // Everything was still measured beyond the listed three.
        XCTAssertEqual(folder["items"] as? Int, 12)
        XCTAssertEqual(folder["unreadable"] as? Int, 1)
        XCTAssertEqual(bytes(folder), allocated("a.txt") + allocated("big.bin") + allocated(".secret")
            + allocated(".stash") + allocated(".stash/buried.txt") + allocated("sub") + allocated("sub/inner.txt")
            + allocated("sub/deeper") + allocated("sub/deeper/leaf.txt"))
    }

    func testADeadlineAlreadyPastTruncatesBeforeAnythingIsEntered() async throws {
        try buildTree()
        let folder = try await folder(ask([root.path], limit: 20_000, deadline: 0))
        XCTAssertEqual(folder["truncated"] as? Bool, true)
        XCTAssertEqual(folder["missing"] as? Bool, false)
        XCTAssertEqual(folder["error"] as? String, "")
        XCTAssertEqual(folder["items"] as? Int, 0)
        XCTAssertEqual((folder["entries"] as? [[String: Any]])?.count, 0)
    }

    func testADeadlineReachedMidWalkStopsAndTruncates() async throws {
        for at in 0..<300 { write(Data(repeating: 65, count: 32), at: "deep/n\(at)/leaf.txt") }
        let folder = try await folder(ask([root.path], limit: 20_000, deadline: 1))
        XCTAssertEqual(folder["missing"] as? Bool, false)
        XCTAssertEqual(folder["truncated"] as? Bool, true)
    }

    func testFoldersKeepTheirOrderAndAMissingFolderIsMissing() async throws {
        try buildTree()
        let slices = SliceStore()
        let missing = root.appendingPathComponent("does-not-exist").path
        _ = try await MeasureAdapter().perform(ask([root.path, missing], limit: 20_000, deadline: 600_000),
                                               slices: slices)
        let held = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(slices.text("measured").utf8)) as? [String: Any])
        let folders = try XCTUnwrap(held["folders"] as? [[String: Any]])
        XCTAssertEqual(folders.count, 2)
        XCTAssertEqual(folders[0]["path"] as? String, root.path)
        XCTAssertEqual(folders[0]["items"] as? Int, 12)
        XCTAssertEqual(folders[1]["path"] as? String, missing)
        XCTAssertEqual(folders[1]["missing"] as? Bool, true)
        XCTAssertEqual(folders[1]["error"] as? String, "")
        XCTAssertEqual(folders[1]["truncated"] as? Bool, false)
        XCTAssertEqual((folders[1]["entries"] as? [[String: Any]])?.count, 0)
    }

    func testCancellingTheWalkCompletesWithTheErrorCancelledAndNoSlice() async throws {
        for at in 0..<600 { write(Data(repeating: 65, count: 16), at: "wide/n\(at)/leaf.txt") }
        let slices = SliceStore()
        let task = Task { try await MeasureAdapter()
            .perform(ask([root.path], limit: 20_000, deadline: 600_000), slices: slices) }
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("a cancelled walk must not succeed")
        } catch is CancellationError {
            XCTAssertEqual(slices.text("measured"), "")
        }
    }
}
