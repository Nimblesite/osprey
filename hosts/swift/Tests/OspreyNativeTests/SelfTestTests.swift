import AppKit
import Foundation
import XCTest
@testable import OspreyNative

// The self-test host is exercised without a window: scripts, region trees,
// picture comparisons, and scripted HTTP all stay deterministic and local.
final class SelfTestTests: XCTestCase {
    private func step(_ verb: String, _ words: String...) -> SelfTestStep {
        SelfTestStep(verb: verb, words: words)
    }

    private func control(_ kind: String, _ title: String, _ event: Int, _ extra: [String: JSONValue] = [:]) -> JSONValue {
        var members = extra
        members["kind"] = .string(kind)
        members["title"] = .string(title)
        members["event"] = .number(Double(event))
        return .object(members)
    }

    private func regions() -> SelfTestRegions {
        let deepText = (0 ..< 3).reduce(JSONValue.object(["text": .string("Ready")])) { node, _ in
            .object(["children": .array([node])])
        }
        let children: [JSONValue] = [
            control("button", "Save", 7),
            control("button", "Delete", 8, ["enabled": .bool(false)]),
            control("disclosure", "Advanced", 9, ["expanded": .bool(false)]),
            control("button", "Save draft copy", 10),
            control("field", "Name", 11),
            control("toggle", "Enabled", 12, ["on": .bool(false)]),
            control("list", "Rows", 13, ["rows": .array([.object(["key": .string("alpha"), "text": .string("Alpha row")])])]),
            control("list", "Choices", 14, ["choices": .array([.object(["key": .string("beta"), "label": .string("Beta choice")])])]),
            deepText,
        ]
        return SelfTestRegions(nodes: [("main", .object(["children": .array(children)]))])
    }

    private func send(_ ordinal: Int, _ value: JSONValue) -> SelfTestAttempt {
        .sending(SelfTestSend(region: "main", ordinal: ordinal, value: value))
    }

    func testOptionsParseEveryFlagAndKeepApplicationArgumentsInOrder() throws {
        XCTAssertNil(try SelfTestOptions.parse(["--headless", "application"]))
        let options = try XCTUnwrap(SelfTestOptions.parse([
            "before", "--self-test", "script.json", "shots", "--headless", "--goldens", "golden",
            "--update-goldens", "--http-double", "double.json", "--store", "state", "middle",
            "--folder-double", "folder", "--patience", "2.5", "after", "last",
        ]))
        XCTAssertEqual(options.script, "script.json")
        XCTAssertEqual(options.shots, "shots")
        XCTAssertTrue(options.headless)
        XCTAssertTrue(options.updateGoldens)
        XCTAssertEqual(options.goldens, "golden")
        XCTAssertEqual(options.httpDouble, "double.json")
        XCTAssertEqual(options.store, "state")
        XCTAssertEqual(options.folderDouble, "folder")
        XCTAssertEqual(options.patience, 2.5)
        XCTAssertEqual(options.appArguments, ["before", "middle", "after", "last"])
    }

    func testOptionsRejectMissingValuesAndBadPatience() {
        let missing = [
            ["--self-test"], ["--self-test", "script.json"],
            ["--self-test", "s", "shots", "--goldens"], ["--self-test", "s", "shots", "--http-double"],
            ["--self-test", "s", "shots", "--store"], ["--self-test", "s", "shots", "--folder-double"],
            ["--self-test", "s", "shots", "--patience"],
        ]
        for arguments in missing { XCTAssertThrowsError(try SelfTestOptions.parse(arguments), "accepted \(arguments)") }
        for seconds in ["soon", "0", "-1", "nan"] {
            XCTAssertThrowsError(try SelfTestOptions.parse(["--self-test", "s", "shots", "--patience", seconds]))
        }
    }

    func testStepsParseOneOrSeveralWordsAndRejectMalformedSteps() throws {
        let steps = try SelfTestStep.parse(#"{"steps":[{"wait":"Ready"},{"type":["Name","Ada Lovelace"]}]}"#)
        XCTAssertEqual(steps.count, 2)
        XCTAssertEqual(steps[0], step("wait", "Ready"))
        XCTAssertEqual(steps[1], step("type", "Name", "Ada Lovelace"))
        XCTAssertThrowsError(try SelfTestStep.parse(#"{"steps":[{"wait":"Ready","gone":"Gone"}]}"#))
        XCTAssertThrowsError(try SelfTestStep.parse(#"{"steps":[{"wait":"Ready"}]"#))
        XCTAssertThrowsError(try SelfTestStep.parse(#"{"steps":{"wait":"Ready"}}"#))
    }

    func testWaitAndGoneFindWordsNestedDeepInChildren() throws {
        let regions = regions()
        XCTAssertEqual(try regions.attempt(step("wait", "Ready")), .sending(nil))
        XCTAssertEqual(try regions.attempt(step("wait", "Missing")), .waiting("nothing shows \"Missing\""))
        XCTAssertEqual(try regions.attempt(step("gone", "Ready")), .waiting("\"Ready\" still shows"))
        XCTAssertEqual(try regions.attempt(step("gone", "Missing")), .sending(nil))
        XCTAssertThrowsError(try regions.attempt(step("wait")))
        XCTAssertThrowsError(try regions.attempt(step("gone")))
    }

    func testInvokeTypeAndToggleSendTheControlValues() throws {
        let regions = regions()
        XCTAssertEqual(try regions.attempt(step("invoke", "Save")), send(7, .null))
        XCTAssertEqual(try regions.attempt(step("invoke", "Delete")), .waiting("no enabled control is named \"Delete\""))
        XCTAssertEqual(try regions.attempt(step("invoke", "Advanced")), send(9, .bool(true)))
        XCTAssertEqual(try regions.attempt(step("invoke", "draft")), send(10, .null))
        XCTAssertEqual(try regions.attempt(step("type", "Name", "Ada")), send(11, .string("Ada")))
        XCTAssertEqual(try regions.attempt(step("toggle", "Enabled")), send(12, .bool(true)))
        XCTAssertThrowsError(try regions.attempt(step("type", "Name")))
    }

    func testChooseShotSleepAndUnknownSteps() throws {
        let regions = regions()
        XCTAssertEqual(try regions.attempt(step("choose", "Alpha")), send(13, .string("alpha")))
        XCTAssertEqual(try regions.attempt(step("choose", "Beta")), send(14, .string("beta")))
        XCTAssertEqual(try regions.attempt(step("choose", "Missing")), .waiting("nothing offers \"Missing\""))
        XCTAssertEqual(try regions.attempt(step("shot", "finished")), .recording("finished"))
        XCTAssertEqual(try regions.attempt(step("sleep", "0.25")), .sleeping(0.25))
        XCTAssertThrowsError(try regions.attempt(step("sleep", "later")))
        XCTAssertThrowsError(try regions.attempt(step("dance", "Save")))
        XCTAssertThrowsError(try regions.attempt(step("shot")))
    }

    func testOutlineListsControlsAndVisibleText() {
        let outline = regions().outline
        XCTAssertTrue(outline.contains("button \"Save\"\n"))
        XCTAssertTrue(outline.contains("button \"Delete\" disabled\n"))
        XCTAssertTrue(outline.contains("disclosure \"Advanced\"\n"))
        XCTAssertTrue(outline.contains("field \"Name\"\n"))
        XCTAssertTrue(outline.contains("toggle \"Enabled\"\n"))
        XCTAssertTrue(outline.contains("text \"Ready\"\n"))
        XCTAssertTrue(outline.hasSuffix("\n"))
    }

    private func picture(width: Int, height: Int, red: UInt8, green: UInt8, blue: UInt8) throws -> Data {
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0), let bytes = bitmap.bitmapData else {
            throw SelfTestProblem("Could not create test picture")
        }
        for y in 0 ..< height {
            for x in 0 ..< width {
                let at = y * bitmap.bytesPerRow + x * 4
                bytes[at] = red; bytes[at + 1] = green; bytes[at + 2] = blue; bytes[at + 3] = 255
            }
        }
        guard let data = bitmap.representation(using: .png, properties: [:]) else { throw SelfTestProblem("Could not encode test picture") }
        return data
    }

    func testPictureDifferenceMeasuresPixelsAndRejectsIncomparableData() throws {
        let black = try picture(width: 3, height: 2, red: 0, green: 0, blue: 0)
        let blackAgain = try picture(width: 3, height: 2, red: 0, green: 0, blue: 0)
        let white = try picture(width: 3, height: 2, red: 255, green: 255, blue: 255)
        let differentSize = try picture(width: 2, height: 2, red: 0, green: 0, blue: 0)
        XCTAssertEqual(SelfTestPicture.difference(black, blackAgain), 0)
        XCTAssertEqual(SelfTestPicture.difference(black, white), 1)
        XCTAssertNil(SelfTestPicture.difference(black, differentSize))
        XCTAssertNil(SelfTestPicture.difference(black, Data("not a picture".utf8)))
        XCTAssertNil(SelfTestPicture.difference(Data(), black))
    }

    private func httpRequest(_ body: String, _ slice: String) -> Request {
        Request(ticket: "t", kind: "http", members: ["body": .string(body), "slice": .string(slice)])
    }

    func testHttpDoubleRepliesInOrderRecordsBodiesAndRejectsExtraRequests() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-double-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let doubles = folder.appendingPathComponent("doubles.json")
        let requests = folder.appendingPathComponent("requests.txt")
        let document = JSONValue.object(["replies": .array([
            .object(["status": 201, "body": .string("first reply")]),
            .object(["status": 202, "body": .string("second reply"), "delayMs": 80]),
        ])])
        try document.text.write(to: doubles, atomically: true, encoding: .utf8)
        let adapter = try HttpDoubleAdapter(file: doubles.path, requests: requests)
        let slices = SliceStore()
        let first = try await adapter.perform(httpRequest("first body", "one"), slices: slices)
        XCTAssertEqual(first, .object(["status": 201, "headers": .array([])]))
        XCTAssertEqual(slices.text("one"), "first reply")
        let started = Date()
        let second = try await adapter.perform(httpRequest("second body", "two"), slices: slices)
        XCTAssertGreaterThanOrEqual(Date().timeIntervalSince(started), 0.08)
        XCTAssertEqual(second, .object(["status": 202, "headers": .array([])]))
        XCTAssertEqual(slices.text("two"), "second reply")
        XCTAssertEqual(try String(contentsOf: requests, encoding: .utf8), "first body\nsecond body\n")
        do {
            _ = try await adapter.perform(httpRequest("extra", "three"), slices: slices)
            XCTFail("A request past the scripted replies succeeded")
        } catch {
            XCTAssertEqual(error as? CapabilityError, .platform("The double has no reply for request 3"))
        }
    }
}
