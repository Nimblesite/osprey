import Foundation
import XCTest
@testable import OspreyNative

// [UI-EFFECTS] Store requests must stay inside the app's private folder.
final class StoreAdapterTests: XCTestCase {
    func testWriteCreatesPrivateDirectoriesAndReplacesAnEntry() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let slices = SliceStore()
        let store = StoreAdapter(root: root)
        let write = Request(ticket: "t", kind: "store-write", members: [
            "name": .string("nested/entry"), "slice": .string("out"),
        ])
        let directory = root.appendingPathComponent("nested")
        let entry = directory.appendingPathComponent("entry")
        for text in ["original", "new"] {
            slices.write("out", text)
            let result = try await store.perform(write, slices: slices)
            XCTAssertEqual(result, .object(["path": .string(entry.path)]))
            XCTAssertEqual(try String(contentsOf: entry, encoding: .utf8), text)
        }
        XCTAssertEqual(try files.contentsOfDirectory(atPath: directory.path), ["entry"])
        XCTAssertEqual(try files.attributesOfItem(atPath: directory.path)[.posixPermissions] as? Int, 0o700)
        XCTAssertEqual(try files.attributesOfItem(atPath: entry.path)[.posixPermissions] as? Int, 0o600)
    }

    func testWriteRefusesAnIntermediateSymlinkOutsideTheStore() async throws {
        let files = FileManager.default
        let fixture = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        let root = fixture.appendingPathComponent("store")
        let outside = fixture.appendingPathComponent("outside")
        defer { try? files.removeItem(at: fixture) }
        for directory in [root, outside] {
            try files.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try files.createSymbolicLink(at: root.appendingPathComponent("link"), withDestinationURL: outside)
        let slices = SliceStore()
        slices.write("out", "private store data")
        let store = StoreAdapter(root: root)
        let write = Request(ticket: "t", kind: "store-write", members: [
            "name": .string("link/secret"), "slice": .string("out"),
        ])
        do {
            _ = try await store.perform(write, slices: slices)
            XCTFail("store accepted a write through a symlink")
        } catch CapabilityError.notAllowed {}
        XCTAssertEqual(try files.contentsOfDirectory(atPath: outside.path), [])
    }

    // [UI-EFFECTS] Reads, listings and deletions resolve through pinned directories too.
    func testReadListAndDeleteWorkInsideTheStore() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let slices = SliceStore()
        slices.write("out", "kept")
        let store = StoreAdapter(root: root)
        _ = try await store.perform(Request(ticket: "t", kind: "store-write", members: ["name": .string("box/a"), "slice": .string("out")]), slices: slices)
        _ = try await store.perform(Request(ticket: "t", kind: "store-write", members: ["name": .string("box/b"), "slice": .string("out")]), slices: slices)
        let read = try await store.perform(Request(ticket: "t", kind: "store-read", members: ["name": .string("box/a"), "slice": .string("in"), "limit": .number(64)]), slices: slices)
        XCTAssertEqual(read, .object(["found": true]))
        XCTAssertEqual(slices.text("in"), "kept")
        let listed = try await store.perform(Request(ticket: "t", kind: "store-list", members: ["name": .string("box")]), slices: slices)
        XCTAssertEqual(listed, .object(["names": .array([.string("a"), .string("b")])]))
        let missing = try await store.perform(Request(ticket: "t", kind: "store-read", members: ["name": .string("box/none"), "slice": .string("in"), "limit": .number(64)]), slices: slices)
        XCTAssertEqual(missing, .object(["found": false]))
        _ = try await store.perform(Request(ticket: "t", kind: "store-delete", members: ["name": .string("box")]), slices: slices)
        XCTAssertFalse(files.fileExists(atPath: root.appendingPathComponent("box").path))
        let gone = try await store.perform(Request(ticket: "t", kind: "store-list", members: ["name": .string("box")]), slices: slices)
        XCTAssertEqual(gone, .object(["names": .array([])]))
    }

    func testReadListAndDeleteRefuseAnIntermediateSymlinkOutsideTheStore() async throws {
        let files = FileManager.default
        let fixture = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        let root = fixture.appendingPathComponent("store")
        let outside = fixture.appendingPathComponent("outside")
        defer { try? files.removeItem(at: fixture) }
        for directory in [root, outside] { try files.createDirectory(at: directory, withIntermediateDirectories: true) }
        try "outside secret".write(to: outside.appendingPathComponent("secret"), atomically: true, encoding: .utf8)
        try files.createSymbolicLink(at: root.appendingPathComponent("link"), withDestinationURL: outside)
        let slices = SliceStore()
        let store = StoreAdapter(root: root)
        let requests = [
            Request(ticket: "t", kind: "store-read", members: ["name": .string("link/secret"), "slice": .string("in"), "limit": .number(64)]),
            Request(ticket: "t", kind: "store-list", members: ["name": .string("link")]),
            Request(ticket: "t", kind: "store-delete", members: ["name": .string("link/secret")]),
        ]
        for request in requests {
            do {
                _ = try await store.perform(request, slices: slices)
                XCTFail("\(request.kind) followed a symlink out of the store")
            } catch CapabilityError.notAllowed {}
        }
        XCTAssertEqual(try String(contentsOf: outside.appendingPathComponent("secret"), encoding: .utf8), "outside secret")
        XCTAssertEqual(slices.text("in"), "")
        _ = try await store.perform(Request(ticket: "t", kind: "store-delete", members: ["name": .string("link")]), slices: slices)
        XCTAssertEqual(try files.contentsOfDirectory(atPath: outside.path), ["secret"])
        XCTAssertFalse(files.fileExists(atPath: root.appendingPathComponent("link").path))
    }
}

