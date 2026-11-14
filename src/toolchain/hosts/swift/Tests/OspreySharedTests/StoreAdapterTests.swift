import Foundation
import XCTest

@testable import OspreyShared

// [UI-EFFECTS] Store requests must stay inside the app's private folder.
final class StoreAdapterTests: RenderingTestCase {
    private func write(_ name: String, slice: String = "out") -> Ask {
        Ask(code: 7, ticket: "t", fields: [
            .init(field: 1, number: 0, text: name),
            .init(field: 2, number: 0, text: slice),
        ])
    }

    private func read(_ name: String, slice: String = "in", limit: Int = 64) -> Ask {
        Ask(code: 6, ticket: "t", fields: [
            .init(field: 1, number: 0, text: name),
            .init(field: 2, number: Int64(limit), text: ""),
            .init(field: 3, number: 0, text: slice),
        ])
    }

    private func list(_ name: String) -> Ask {
        Ask(code: 8, ticket: "t", fields: [.init(field: 1, number: 0, text: name)])
    }

    private func delete(_ name: String) -> Ask {
        Ask(code: 9, ticket: "t", fields: [.init(field: 1, number: 0, text: name)])
    }

    func testWriteCreatesPrivateDirectoriesAndReplacesAnEntry() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let slices = SliceStore()
        let store = StoreAdapter(root: root)
        let directory = root.appendingPathComponent("nested")
        let entry = directory.appendingPathComponent("entry")
        for text in ["original", "new"] {
            slices.write("out", text)
            let result = try await store.perform(write("nested/entry"), slices: slices)
            XCTAssertEqual(result, .document(.object(["path": .string(entry.path)])))
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
        do {
            _ = try await store.perform(write("link/secret"), slices: slices)
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
        _ = try await store.perform(write("box/a"), slices: slices)
        _ = try await store.perform(write("box/b"), slices: slices)
        let found = try await store.perform(read("box/a"), slices: slices)
        XCTAssertEqual(found, .document(.object(["found": true])))
        XCTAssertEqual(slices.text("in"), "kept")
        let listed = try await store.perform(list("box"), slices: slices)
        XCTAssertEqual(listed, .document(.object(["names": .array([.string("a"), .string("b")])])))
        let missing = try await store.perform(read("box/none"), slices: slices)
        XCTAssertEqual(missing, .document(.object(["found": false])))
        _ = try await store.perform(delete("box"), slices: slices)
        XCTAssertFalse(files.fileExists(atPath: root.appendingPathComponent("box").path))
        let gone = try await store.perform(list("box"), slices: slices)
        XCTAssertEqual(gone, .document(.object(["names": .array([])])))
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
        let asks = [read("link/secret"), list("link"), delete("link/secret")]
        for ask in asks {
            do {
                _ = try await store.perform(ask, slices: slices)
                XCTFail("\(ask.kind) followed a symlink out of the store")
            } catch CapabilityError.notAllowed {}
        }
        XCTAssertEqual(try String(contentsOf: outside.appendingPathComponent("secret"), encoding: .utf8), "outside secret")
        XCTAssertEqual(slices.text("in"), "")
        _ = try await store.perform(delete("link"), slices: slices)
        XCTAssertEqual(try files.contentsOfDirectory(atPath: outside.path), ["secret"])
        XCTAssertFalse(files.fileExists(atPath: root.appendingPathComponent("link").path))
    }

    // Extra: a name that would leave the store — an absolute path, a parent reference, an
    // empty name or one holding a NUL — is refused before anything is created on disk.
    func testWriteRefusesAnIntermediateSymlinkOutsideTheStore_extraEscapingNamesAreRefusedBeforeAnyFileIsTouched() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let store = StoreAdapter(root: root)
        for name in ["/tmp/escape", "../escape", "box/../../escape", "", "nul\0byte"] {
            do {
                _ = try await store.perform(write(name), slices: SliceStore())
                XCTFail("store accepted the escaping name \(name.debugDescription)")
            } catch CapabilityError.notAllowed, CapabilityError.invalidArgument {}
        }
        XCTAssertEqual(try files.contentsOfDirectory(atPath: root.path), [])
    }

    // Extra: a missing leaf in an existing folder, a missing folder and a missing entry to
    // delete are each ordinary answers — found false, no names, no error — and create nothing.
    func testReadListAndDeleteRefuseAnIntermediateSymlinkOutsideTheStore_extraMissingEntriesAnswerOrdinarilyAndCreateNothing() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let slices = SliceStore()
        slices.write("out", "x")
        let store = StoreAdapter(root: root)
        _ = try await store.perform(write("kept/a"), slices: slices)
        let leaf = try await store.perform(read("kept/none"), slices: slices)
        XCTAssertEqual(leaf, .document(.object(["found": false])))
        let folder = try await store.perform(read("missing/none"), slices: slices)
        XCTAssertEqual(folder, .document(.object(["found": false])))
        let listed = try await store.perform(list("missing"), slices: slices)
        XCTAssertEqual(listed, .document(.object(["names": .array([])])))
        _ = try await store.perform(delete("kept/none"), slices: slices)
        XCTAssertEqual(try files.contentsOfDirectory(atPath: root.appendingPathComponent("kept").path), ["a"])
        XCTAssertFalse(files.fileExists(atPath: root.appendingPathComponent("missing").path))
    }

    // Extra: an atomic write leaves no temporary residue beside the entry it replaced.
    func testWriteCreatesPrivateDirectoriesAndReplacesAnEntry_extraAtomicReplacementLeavesNoTemporaryResidue() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let slices = SliceStore()
        let store = StoreAdapter(root: root)
        for round in 0..<5 { slices.write("out", "revision \(round)"); _ = try await store.perform(write("entry"), slices: slices) }
        XCTAssertEqual(try files.contentsOfDirectory(atPath: root.path), ["entry"])
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("entry"), encoding: .utf8), "revision 4")
    }

    // Extra: a listing names directories and files together, sorted, without `.` and `..`.
    func testReadListAndDeleteWorkInsideTheStore_extraListingsSortNamesAndIncludeDirectories() async throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("osprey-store-\(UUID())")
        defer { try? files.removeItem(at: root) }
        let slices = SliceStore()
        slices.write("out", "x")
        let store = StoreAdapter(root: root)
        _ = try await store.perform(write("box/a-entry"), slices: slices)
        _ = try await store.perform(write("box/b-entry"), slices: slices)
        _ = try await store.perform(write("box/z-folder/inner"), slices: slices)
        let listed = try await store.perform(list("box"), slices: slices)
        XCTAssertEqual(listed, .document(.object(["names": .array([.string("a-entry"), .string("b-entry"), .string("z-folder")])])))
    }
}

