import AppKit
import Foundation
import SwiftUI
import XCTest
@testable import OspreyShared

// Scripts, region trees, picture comparisons and scripted HTTP stay local.
// Button geometry also exercises a real AppKit window's layout.
final class SelfTestTests: RenderingTestCase {
    @MainActor
    func testButtonFrameIsRecordedWhenRecordingStartsAfterWindowLayout() async throws {
        _ = NSApplication.shared
        defer {
            ButtonFrames.recording = false
            ButtonFrames.frames.removeAll()
        }
        let node = control("button", "Project folder", 1, ["label": "Project folder", "enabled": .bool(true)])
        let size = NSRect(x: 0, y: 0, width: 320, height: 180)
        let window = NSWindow(contentRect: size, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let view = NSHostingView(rootView: ButtonView(node: node) {})
        view.frame = size
        window.contentView = view
        view.layoutSubtreeIfNeeded()
        XCTAssertNil(ButtonFrames.frames["Project folder"])
        ButtonFrames.recording = true
        for _ in 0..<10 where ButtonFrames.frames["Project folder"] == nil {
            try await Task.sleep(for: .milliseconds(20))
            view.layoutSubtreeIfNeeded()
        }
        let frame = try XCTUnwrap(ButtonFrames.frames["Project folder"])
        XCTAssertGreaterThan(frame.width, 0)
        XCTAssertGreaterThan(frame.height, 0)
    }

    /// The paste step waits for a message field whose paste monitor will act, not just any text
    /// view holding the keyboard; once it may paste, Cmd-V lands as a marker in the words.
    @MainActor
    func testAPictureIsPastedOnlyOnceTheMessageFieldIsReadyToTakeIt() async throws {
        _ = NSApplication.shared
        var sent: [String] = []
        let node = control("field", "Message", 2, ["multiline": .bool(true), "commit": "edit", "value": "hi"])
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 120), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        XCTAssertTrue(PastedImage.receivers.isEmpty, "no field is on screen yet")
        window.contentView = NSHostingView(rootView: FieldView(node: node, revision: 1) { sent.append($0) })
        window.makeKeyAndOrderFront(nil)
        for _ in 0..<50 where PastedImage.receivers.isEmpty { try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertEqual(PastedImage.receivers.count, 1, "the focused message field takes pastes")
        XCTAssertTrue(window.firstResponder is NSText)
        let board = NSPasteboard(name: NSPasteboard.Name("Osprey paste test"))
        board.clearContents()
        XCTAssertTrue(board.setData(try XCTUnwrap(NSImage(size: NSSize(width: 2, height: 2), flipped: false) { NSColor.systemTeal.setFill(); $0.fill(); return true }.tiffRepresentation), forType: .tiff))
        PastedImage.pasteboard = board
        defer { PastedImage.pasteboard = nil }
        let paste = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command, timestamp: 0, windowNumber: window.windowNumber, context: nil, characters: "v", charactersIgnoringModifiers: "v", isARepeat: false, keyCode: 9))
        NSApp.postEvent(paste, atStart: true)
        if let event = NSApp.nextEvent(matching: .keyDown, until: Date(timeIntervalSinceNow: 1), inMode: .default, dequeue: true) { NSApp.sendEvent(event) }
        for _ in 0..<50 where sent.isEmpty { try await Task.sleep(for: .milliseconds(20)) }
        let words = try XCTUnwrap(sent.last)
        XCTAssertTrue(words.hasPrefix("hi [image: ") && words.hasSuffix(".png]"), words)
        let name = String(words.dropFirst("hi [image: ".count).dropLast())
        try FileManager.default.removeItem(at: PastedImage.folder.appendingPathComponent(name))
        window.close()
        window.contentView = nil
        for _ in 0..<50 where !PastedImage.receivers.isEmpty { try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertTrue(PastedImage.receivers.isEmpty, "a field that leaves the screen takes no more pastes")
    }

    @MainActor
    func testControlsSurviveFramesButRejectChangedBindings() {
        let picker = control("picker", "Speed", 4, ["selected": "1", "choices": .array([.object(["key": "1.5", "label": "1.5×"])])])
        let cell = RegionCell(key: "root", revision: 1, node: picker)
        let first = PickerView(node: picker) { _ in XCTFail("Rendering must not send a choice") }
        XCTAssertEqual(first, PickerView(node: picker) { _ in })
        XCTAssertEqual(cell.controlRevision(for: picker, value: .string("1.5")), 1)
        cell.replace(revision: 20, node: .object(["body": .object(["children": .array([picker])])]))
        XCTAssertEqual(cell.controlRevision(for: picker, value: .string("1.5")), 20)
        XCTAssertNil(cell.controlRevision(for: picker, value: .string("unknown")))
        for (field, replacement) in [("event", Tree.number(5)), ("title", .string("Volume")), ("selected", .string("1.5")), ("choices", .array([]))] {
            var changed = picker.object ?? [:]; changed[field] = replacement
            let node = Tree.object(changed)
            XCTAssertNotEqual(first, PickerView(node: node) { _ in })
            cell.replace(revision: 21, node: node)
            XCTAssertNil(cell.controlRevision(for: picker, value: .string("1.5")), field)
        }
        cell.replace(revision: 22, node: .null)
        XCTAssertNil(cell.controlRevision(for: picker, value: .string("1.5")))
        let button = control("button", "Stop", 5, ["enabled": .bool(true)])
        cell.replace(revision: 23, node: button)
        XCTAssertEqual(cell.controlRevision(for: button, value: .null), 23)
        XCTAssertNil(cell.controlRevision(for: button, value: .string("unexpected")))
        cell.replace(revision: 24, node: .object(["children": .array([button])]))
        XCTAssertEqual(cell.controlRevision(for: button, value: .null), 24)
        for (field, replacement) in [("event", Tree.number(6)), ("title", .string("Resume")), ("enabled", .bool(false))] {
            var changed = button.object ?? [:]; changed[field] = replacement
            cell.replace(revision: 25, node: .object(changed))
            XCTAssertNil(cell.controlRevision(for: button, value: .null), field)
        }
        let toggle = control("toggle", "Loop", 5)
        cell.replace(revision: 26, node: toggle)
        XCTAssertNil(cell.controlRevision(for: toggle, value: .bool(true)))
    }

    private func step(_ verb: String, _ words: String...) -> SelfTestStep {
        SelfTestStep(verb: verb, words: words)
    }

    private func control(_ kind: String, _ title: String, _ event: Int, _ extra: [String: Tree] = [:]) -> Tree {
        var members = extra
        members["kind"] = .string(kind)
        members["title"] = .string(title)
        members["event"] = .number(Double(event))
        return .object(members)
    }

    private func regions() -> SelfTestRegions {
        let deepText = (0 ..< 3).reduce(Tree.object(["text": .string("Ready")])) { node, _ in
            .object(["children": .array([node])])
        }
        let children: [Tree] = [
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

    private func send(_ ordinal: Int, _ value: Tree) -> SelfTestAttempt {
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

    func testPanelStepParsesAndOnlyCancels() throws {
        let steps = try SelfTestStep.parse(#"{"steps":[{"panel":"cancel"}]}"#)
        XCTAssertEqual(steps, [step("panel", "cancel")])
        XCTAssertEqual(try regions().attempt(steps[0]), .panel)
        XCTAssertThrowsError(try regions().attempt(step("panel")))
        XCTAssertThrowsError(try regions().attempt(step("panel", "open")))
    }

    func testSelectionStepsParseIntoTheirAttempts() throws {
        let steps = try SelfTestStep.parse(#"{"steps":[{"select":["quick","lazy"]},{"selected":"quick brown"},{"selected-contains":"brown"},{"copy":"quick brown"},{"click":"lazy"},{"select-all":""}]}"#)
        let regions = regions()
        XCTAssertEqual(try regions.attempt(steps[0]), .selecting(["quick", "lazy"]))
        XCTAssertEqual(try regions.attempt(steps[1]), .checkingSelected("quick brown", contains: false))
        XCTAssertEqual(try regions.attempt(steps[2]), .checkingSelected("brown", contains: true))
        XCTAssertEqual(try regions.attempt(steps[3]), .copying("quick brown"))
        XCTAssertEqual(try regions.attempt(steps[4]), .clicking("lazy"))
        XCTAssertEqual(try regions.attempt(steps[5]), .keyPress("a"))
        XCTAssertThrowsError(try regions.attempt(step("select", "quick")))
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

    private func composerSend(_ ordinal: Int, _ value: Tree) -> SelfTestAttempt {
        .sending(SelfTestSend(region: "composer", ordinal: ordinal, value: value))
    }

    func testChoosePrefersAnExactChoiceOverASubstring() throws {
        let model = control("picker", "Model", 20, ["choices": .array([
            .object(["key": .string("zai/glm-5.3-highspeed"), "label": .string("GLM · GLM-5.3 Highspeed")]),
            .object(["key": .string("zai/glm-5.3"), "label": .string("GLM · GLM-5.3")]),
        ])])
        let effort = control("picker", "Effort", 21, ["choices": .array([
            .object(["key": .string("low"), "label": .string("Low")]),
            .object(["key": .string("high"), "label": .string("High")]),
            .object(["key": .string("max"), "label": .string("Max")]),
        ])])
        let regions = SelfTestRegions(nodes: [("composer", .object(["children": .array([model, effort])]))])
        XCTAssertEqual(try regions.attempt(step("choose", "High")), composerSend(21, .string("high")))
        XCTAssertEqual(try regions.attempt(step("choose", "GLM-5.3")), composerSend(20, .string("zai/glm-5.3")))
        XCTAssertEqual(try regions.attempt(step("choose", "Highspeed")), composerSend(20, .string("zai/glm-5.3-highspeed")))
        XCTAssertEqual(try regions.attempt(step("choose", "Max")), composerSend(21, .string("max")))
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

    /// An `http` ask as the frame's rows carry it: method 1, URL 2, headers 3, body 4, limit 5, deadline 6, slice 7.
    private func httpRequest(_ body: String, _ slice: String, url: String = "https://example.test/chat", method: String = "POST", authorized: Bool = true) -> Ask {
        let headers = authorized ? [AskField(field: 3, number: 0, text: "Authorization"), AskField(field: 3, number: 0, text: "Bearer k")] : []
        let fields = [AskField(field: 1, number: 0, text: method), AskField(field: 2, number: 0, text: url)] + headers + [
            AskField(field: 4, number: 0, text: body), AskField(field: 5, number: 0, text: ""),
            AskField(field: 6, number: 1000, text: ""), AskField(field: 7, number: 0, text: slice),
        ]
        return Ask(code: Int64((sharedCapabilities.firstIndex(of: "http") ?? 9) + 1), ticket: "t", fields: fields)
    }

    func testHttpDoubleRefusesWhatTheRealAdapterWould() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-double-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("double.json")
        try #"{"replies": [{"status": 200, "body": "a"}, {"status": 200, "body": "b"}, {"status": 200, "body": "c"}]}"#.write(to: file, atomically: true, encoding: .utf8)
        let double = try HttpDoubleAdapter(file: file.path, requests: folder.appendingPathComponent("requests.jsonl"))
        for bad in [httpRequest("x", "s", url: "http://example.test/chat"), httpRequest("x", "s", url: "http://127.0.0.1.example.test/chat"), httpRequest("x", "s", method: "GET"), httpRequest("x", "s", authorized: false)] {
            do {
                _ = try await double.perform(bad, slices: SliceStore())
                XCTFail("The double took \(bad.fields)")
            } catch is CapabilityError {}
        }
        for loopback in ["http://localhost:1/mcp", "http://127.0.0.1:1/mcp", "http://[::1]:1/mcp"] {
            XCTAssertNoThrow(try HttpAdapter.outgoing(httpRequest("x", "s", url: loopback), timeout: 1000))
        }
        XCTAssertThrowsError(try HttpAdapter.outgoing(httpRequest("x", "s", url: "http://example.test/mcp"), timeout: 1000))
        XCTAssertThrowsError(try HttpAdapter.outgoing(httpRequest("x", "s", url: "ftp://localhost/mcp"), timeout: 1000))
    }

    func testHttpDoubleRepliesInOrderRecordsBodiesAndRejectsExtraRequests() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-double-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let doubles = folder.appendingPathComponent("doubles.json")
        let requests = folder.appendingPathComponent("requests.txt")
        let document = Tree.object(["replies": .array([
            .object(["status": 201, "body": .string("first reply")]),
            .object(["status": 202, "body": .string("second reply"), "delayMs": 80]),
        ])])
        try document.fixtureText.write(to: doubles, atomically: true, encoding: .utf8)
        let adapter = try HttpDoubleAdapter(file: doubles.path, requests: requests)
        let slices = SliceStore()
        let first = try await adapter.perform(httpRequest("first body", "one"), slices: slices)
        XCTAssertEqual(first, .document(.object(["status": 201, "headers": .array([])])))
        XCTAssertEqual(slices.text("one"), "first reply")
        let started = Date()
        let second = try await adapter.perform(httpRequest("second body", "two"), slices: slices)
        XCTAssertGreaterThanOrEqual(Date().timeIntervalSince(started), 0.08)
        XCTAssertEqual(second, .document(.object(["status": 202, "headers": .array([])])))
        XCTAssertEqual(slices.text("two"), "second reply")
        let recorded = try String(contentsOf: requests, encoding: .utf8)
            .split(separator: "\n").compactMap { Tree.fixture(Data($0.utf8)) }
        XCTAssertEqual(recorded.count, 2)
        XCTAssertEqual(recorded[0]["url"]?.string, "https://example.test/chat")
        XCTAssertEqual(recorded[0]["body"]?.string, "first body")
        XCTAssertEqual(recorded[1]["url"]?.string, "https://example.test/chat")
        XCTAssertEqual(recorded[1]["body"]?.string, "second body")
        XCTAssertFalse(recorded[0].fixtureText.contains("Bearer"))
        do {
            _ = try await adapter.perform(httpRequest("extra", "three"), slices: slices)
            XCTFail("A request past the scripted replies succeeded")
        } catch {
            XCTAssertEqual(error as? CapabilityError, .platform("The double has no reply for request 3"))
        }
    }

    func testHttpDoubleAnswersStrictUrlAndBearerAsTheRealServerWould() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("osprey-double-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let doubles = folder.appendingPathComponent("doubles.json")
        let requests = folder.appendingPathComponent("requests.jsonl")
        let document = Tree.object(["replies": .array([
            .object(["status": 200, "body": .string("right url"), "requireUrl": .string("https://api.z.ai/api/coding/paas/v4/chat/completions")]),
            .object(["status": 200, "body": .string("wrong url never shown"), "requireUrl": .string("https://api.z.ai/api/coding/paas/v4/chat/completions")]),
            .object(["status": 200, "body": .string("bearer ok"), "requireBearer": true]),
        ])])
        try document.fixtureText.write(to: doubles, atomically: true, encoding: .utf8)
        let adapter = try HttpDoubleAdapter(file: doubles.path, requests: requests)
        let slices = SliceStore()
        let right = try await adapter.perform(httpRequest("a", "one", url: "https://api.z.ai/api/coding/paas/v4/chat/completions"), slices: slices)
        XCTAssertEqual(right, .document(.object(["status": 200, "headers": .array([])])))
        XCTAssertEqual(slices.text("one"), "right url")
        let wrong = try await adapter.perform(httpRequest("b", "two", url: "https://api.z.ai/api/paas/v4/chat/completions"), slices: slices)
        XCTAssertEqual(wrong, .document(.object(["status": 404, "headers": .array([])])))
        let bearer = try await adapter.perform(httpRequest("c", "three"), slices: slices)
        XCTAssertEqual(bearer, .document(.object(["status": 200, "headers": .array([])])))
        XCTAssertEqual(slices.text("three"), "bearer ok")
        // The key never reaches the records: only where each answered request went and its body are written.
        let lines = try String(contentsOf: requests, encoding: .utf8).split(separator: "\n").map(String.init)
        XCTAssertEqual(lines.count, 2)
        XCTAssertTrue(lines[0].contains("coding"))
        for line in lines { XCTAssertFalse(line.contains("Bearer")) }
    }
}
