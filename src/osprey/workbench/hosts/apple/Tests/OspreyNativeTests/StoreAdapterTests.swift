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
}

