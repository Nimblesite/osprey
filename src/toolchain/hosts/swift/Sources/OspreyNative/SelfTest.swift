#if os(macOS)
import AppKit
import Foundation
import SwiftUI

// Implements [UI-MACHINE-SELF-TEST]: one script beside an application, with or
// without a window. Steps name only what a person reads, and act through the
// same events the views send. `--headless` keeps the window off screen, and
// `--http-double` answers `http` from a file, so a run needs no display or
// network. Without them the real window makes the real requests.

/// The host's own flags; whatever else is on the command line belongs to the application.
public struct SelfTestOptions: Equatable {
    public var script = ""
    public var shots = ""
    public var headless = false
    public var goldens: String?
    public var updateGoldens = false
    public var httpDouble: String?
    public var store: String?
    public var folderDouble: String?
    public var patience = 60.0
    public var appArguments: [String] = []

    /// Nil when this launch is not a self-test.
    public static func parse(_ arguments: [String]) throws -> SelfTestOptions? {
        guard arguments.contains("--self-test") else { return nil }
        var options = SelfTestOptions()
        var given = arguments.makeIterator()
        func value(_ flag: String) throws -> String {
            guard let next = given.next() else { throw SelfTestProblem("\(flag) needs a value") }
            return next
        }
        while let argument = given.next() {
            switch argument {
            case "--self-test":
                options.script = try value(argument)
                options.shots = try value(argument)
            case "--headless": options.headless = true
            case "--update-goldens": options.updateGoldens = true
            case "--goldens": options.goldens = try value(argument)
            case "--http-double": options.httpDouble = try value(argument)
            case "--store": options.store = try value(argument)
            case "--folder-double": options.folderDouble = try value(argument)
            case "--patience":
                guard let seconds = Double(try value(argument)), seconds > 0 else { throw SelfTestProblem("--patience needs a number of seconds") }
                options.patience = seconds
            default: options.appArguments.append(argument)
            }
        }
        return options
    }
}

public struct SelfTestProblem: Error, Equatable, CustomStringConvertible {
    public let description: String
    public init(_ description: String) { self.description = description }
}

/// One step of a script: a verb and its words.
public struct SelfTestStep: Equatable {
    public let verb: String
    public let words: [String]

    /// `{"steps": [{"verb": "word"}, {"verb": ["word", "word"]}]}`.
    public static func parse(_ text: String) throws -> [SelfTestStep] {
        guard let script = try? JSONDecoder().decode(JSONValue.self, from: Data(text.utf8)), let steps = script["steps"]?.array else {
            throw SelfTestProblem("A script holds a list of steps")
        }
        return try steps.map { step in
            guard let members = step.object, members.count == 1, let (verb, value) = members.first else { throw SelfTestProblem("A step is one verb: \(step.text)") }
            return SelfTestStep(verb: verb, words: value.array?.compactMap(\.string) ?? value.string.map { [$0] } ?? [])
        }
    }
}

/// One event a step sends, bound to its region's current revision when it is sent.
public struct SelfTestSend: Equatable {
    public let region: String
    public let ordinal: Int
    public let value: JSONValue
}

/// What a step needs of the regions as they stand.
public enum SelfTestAttempt: Equatable {
    case waiting(String)
    case sending(SelfTestSend?)
    case recording(String)
    case sleeping(Double)
    case panel
    /// A drag between two words, or `click` on one, through the window's real events; a frame's
    /// selection group ([UI-COMPONENTS-FRAMED]) answers.
    case selecting([String])
    case checkingSelected(String, contains: Bool)
    case copying(String)
    case clicking(String)
    case pressing(String)
    case pastingImage
    case keyPress(String)
}

/// The regions as words and controls, with no host or window behind them.
public struct SelfTestRegions {
    public let nodes: [(key: String, node: JSONValue)]

    /// A region no slot draws is not on show; without a `root`, every region is.
    public init(nodes: [(key: String, node: JSONValue)]) {
        let all = Dictionary(nodes.map { ($0.key, $0.node) }, uniquingKeysWith: { first, _ in first })
        var reached: [String] = []
        func follow(_ node: JSONValue) {
            if node.text("kind") == "slot", all[node.text("key")] != nil, !reached.contains(node.text("key")) {
                reached.append(node.text("key"))
                follow(all[node.text("key")] ?? .null)
            }
            Self.inside(node).forEach(follow)
        }
        if let root = all["root"] {
            reached.append("root")
            follow(root)
        } else {
            reached = Array(all.keys)
        }
        self.nodes = reached.sorted().compactMap { key in all[key].map { (key: key, node: $0) } }
    }

    // What a closed disclosure holds is not on show.
    private static func inside(_ node: JSONValue) -> [JSONValue] {
        if node.text("kind") == "disclosure", !node.flag("expanded") { return [] }
        return node.object?.values.flatMap { $0.array ?? [$0] }.filter { $0.object != nil } ?? []
    }

    private static func holds(_ node: JSONValue, _ words: String) -> Bool {
        let own = node.object?.values.contains { $0.string?.contains(words) == true || $0.array?.contains { $0.string?.contains(words) == true } == true } ?? false
        return own || inside(node).contains { holds($0, words) }
    }

    private static func find(_ node: JSONValue, _ wanted: (JSONValue) -> Bool) -> JSONValue? {
        if wanted(node) { return node }
        for child in inside(node) { if let found = find(child, wanted) { return found } }
        return nil
    }

    public func shows(_ words: String) -> Bool { nodes.contains { Self.holds($0.node, words) } }

    private func control(_ wanted: (JSONValue) -> Bool) -> (region: String, node: JSONValue)? {
        for (key, root) in nodes {
            if let node = Self.find(root, { $0["event"]?.int != nil && wanted($0) }) { return (key, node) }
        }
        return nil
    }

    private func named(_ label: String) -> (region: String, node: JSONValue)? {
        control { $0.text("label") == label || $0.text("title") == label }
    }

    /// A label read in full, or else the first control whose label holds the words: a disclosure's title counts what it did.
    private func invocable(_ words: String) -> (region: String, node: JSONValue)? {
        named(words) ?? control { $0.text("label").contains(words) || $0.text("title").contains(words) }
    }

    private func enabled(_ node: JSONValue) -> Bool { node["enabled"]?.bool ?? true }

    private func chooser(_ words: String) -> SelfTestSend? {
        // An exact choice wins over a substring, so `High` picks an effort picker's `High` before a model named `Highspeed`, and `GLM-5.3` picks `GLM · GLM-5.3` before `GLM · GLM-5.3 Highspeed`.
        for exact in [true, false] {
            for field in ["rows", "choices"] {
                for (key, root) in nodes {
                    let wanted = { (row: JSONValue) in
                        Self.holds(row, words) && (!exact || row.text("label") == words || row.text("label").hasSuffix(" · " + words) || row.text("key") == words)
                    }
                    guard let node = Self.find(root, { $0["event"]?.int != nil && $0.list(field).contains(where: wanted) }) else { continue }
                    if let row = node.list(field).first(where: wanted) {
                        return SelfTestSend(region: key, ordinal: node.count("event"), value: .string(row.text("key")))
                    }
                }
            }
        }
        return nil
    }

    public func attempt(_ step: SelfTestStep) throws -> SelfTestAttempt {
        func word(_ index: Int) throws -> String {
            guard index < step.words.count else { throw SelfTestProblem("\(step.verb) needs \(index + 1) words") }
            return step.words[index]
        }
        switch step.verb {
        case "wait": return shows(try word(0)) ? .sending(nil) : .waiting("nothing shows \"\(try word(0))\"")
        case "gone": return shows(try word(0)) ? .waiting("\"\(try word(0))\" still shows") : .sending(nil)
        case "invoke":
            guard let found = invocable(try word(0)), enabled(found.node) else { return .waiting("no enabled control is named \"\(try word(0))\"") }
            let value: JSONValue = found.node.text("kind") == "disclosure" ? .bool(!found.node.flag("expanded")) : .null
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: value))
        case "toggle":
            guard let found = named(try word(0)), enabled(found.node) else { return .waiting("no enabled switch is named \"\(try word(0))\"") }
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: .bool(!found.node.flag("on"))))
        case "type":
            guard let found = named(try word(0)) else { return .waiting("no field is named \"\(try word(0))\"") }
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: .string(try word(1))))
        case "choose":
            guard let send = chooser(try word(0)) else { return .waiting("nothing offers \"\(try word(0))\"") }
            return .sending(send)
        case "resize":
            guard let width = Double(try word(0)) else { throw SelfTestProblem("resize needs a width") }
            guard let found = control({ $0.text("kind") == "aside" }) else { return .waiting("no column can be resized") }
            return .sending(SelfTestSend(region: found.region, ordinal: found.node.count("event"), value: .number(width)))
        case "shot": return .recording(try word(0))
        case "panel":
            guard try word(0) == "cancel" else { throw SelfTestProblem("panel needs cancel") }
            return .panel
        case "select":
            guard step.words.count == 2 else { throw SelfTestProblem("select needs two words") }
            return .selecting(step.words)
        case "selected", "selected-contains":
            return .checkingSelected(try word(0), contains: step.verb == "selected-contains")
        case "copy": return .copying(try word(0))
        case "click": return .clicking(try word(0))
        case "press": return .pressing(try word(0))
        case "paste-image": return .pastingImage
        case "select-all": return .keyPress("a")
        case "sleep":
            guard let seconds = Double(try word(0)) else { throw SelfTestProblem("sleep needs a number of seconds") }
            return .sleeping(seconds)
        default: throw SelfTestProblem("Unknown step \(step.verb)")
        }
    }

    /// The tree as an agent reads it: one line for each control, then the words on show.
    public var outline: String {
        var lines: [String] = []
        func walk(_ node: JSONValue) {
            let name = node.text("label").isEmpty ? node.text("title") : node.text("label")
            if node["event"]?.int != nil {
                lines.append("\(node.text("kind")) \"\(name)\"\(enabled(node) ? "" : " disabled")")
            } else if !node.text("text").isEmpty {
                lines.append("text \"\(node.text("text"))\"")
            }
            Self.inside(node).sorted { $0.text < $1.text }.forEach(walk)
        }
        nodes.forEach { walk($0.node) }
        return lines.joined(separator: "\n") + "\n"
    }
}

/// Answers `http` from a file of replies, in order, and keeps each request beside the run.
public struct HttpDoubleAdapter: CapabilityAdapter {
    public let kinds = ["http"]
    private let replies: [JSONValue]
    private let models: JSONValue?
    private let requests: URL
    private let position = Position()

    private final class Position: @unchecked Sendable {
        private let lock = NSLock()
        private var next = 0
        func take() -> Int {
            lock.lock()
            defer { lock.unlock() }
            next += 1
            return next - 1
        }
    }

    /// `{"models": {"deepseek-chat": ...}, "replies": [{"status": 200, "body": "...", "delayMs": 0}]}`.
    /// A provider's model list is a GET that takes no reply: `models` answers it for any
    /// provider, and with no `models` it answers what a provider with no such list would.
    public init(file: String, requests: URL) throws {
        let value = try JSONDecoder().decode(JSONValue.self, from: Data(contentsOf: URL(fileURLWithPath: file)))
        guard let replies = value["replies"]?.array else { throw SelfTestProblem("A double holds a list of replies") }
        self.replies = replies
        self.models = value["models"]
        self.requests = requests
    }

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        // A server on this machine is real in a doubled run: only the model is the double.
        if let url = URL(string: try request.text("url")), HttpAdapter.isLoopback(url) { return try await HttpAdapter().perform(request, slices: slices) }
        let outgoing = try HttpAdapter.outgoing(request, timeout: max(try request.number("timeout"), 1))
        if outgoing.httpMethod == "GET", outgoing.url?.path.hasSuffix("/models") == true, outgoing.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Bearer ") == true {
            guard let models else { return .object(["status": .number(404), "headers": .array([])]) }
            slices.write(try request.text("slice"), models.text)
            return .object(["status": .number(200), "headers": .array([])])
        }
        let index = position.take()
        guard index < replies.count else { throw CapabilityError.platform("The double has no reply for request \(index + 1)") }
        guard outgoing.httpMethod == "POST", outgoing.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Bearer ") == true else {
            throw CapabilityError.notAllowed("The double takes only an authorized POST")
        }
        // A strict double answers the status the real server would, without logging the key.
        let entry = replies[index]
        if let wanted = entry["requireUrl"]?.string, wanted != outgoing.url?.absoluteString {
            return .object(["status": .number(404), "headers": .array([])])
        }
        if entry["requireBearer"]?.bool == true, outgoing.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Bearer ") != true {
            return .object(["status": .number(401), "headers": .array([])])
        }
        if let delay = entry["delayMs"]?.int { try await Task.sleep(for: .milliseconds(delay)) }
        // The record holds where the request went and what it carried, never the key.
        let record = JSONValue.object(["url": .string(outgoing.url?.absoluteString ?? ""), "body": .string(try request.text("body"))])
        let line = Data((record.text + "\n").utf8)
        if let handle = try? FileHandle(forWritingTo: requests) {
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: line)
            try? handle.close()
        } else {
            try line.write(to: requests)
        }
        slices.write(try request.text("slice"), entry.text("body"))
        return .object(["status": .number(Double(entry.count("status"))), "headers": .array([])])
    }
}

/// Answers the folder picker, which no script can reach, with one folder.
public struct ChooseFolderDouble: CapabilityAdapter {
    public let kinds = ["choose-folder"]
    private let path: String
    public init(path: String) { self.path = path }

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        .object(["path": .string(path), "cancelled": .bool(false)])
    }
}

/// Pictures compared by share of pixels that differ, so a stray antialiased edge is not a failure.
public enum SelfTestPicture {
    public static let tolerance = 0.005
    private static let channelDelta = 24

    static func pixels(_ data: Data) -> (width: Int, height: Int, bytes: [UInt8])? {
        guard let image = NSBitmapImageRep(data: data)?.cgImage else { return nil }
        var bytes = [UInt8](repeating: 0, count: image.width * image.height * 4)
        let space = CGColorSpace(name: CGColorSpace.sRGB)!
        let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        return drawn ? (image.width, image.height, bytes) : nil
    }

    /// The share of pixels that differ; nil when the pictures cannot be compared.
    public static func difference(_ first: Data, _ second: Data) -> Double? {
        guard let one = pixels(first), let two = pixels(second), one.width == two.width, one.height == two.height, one.width > 0, one.height > 0 else { return nil }
        var differing = 0
        for pixel in 0 ..< one.width * one.height {
            let at = pixel * 4
            if (0 ..< 4).contains(where: { abs(Int(one.bytes[at + $0]) - Int(two.bytes[at + $0])) > channelDelta }) { differing += 1 }
        }
        return Double(differing) / Double(one.width * one.height)
    }
}

/// Plays a script against a live store, and keeps a picture and an outline of each `shot`.
@MainActor
public final class SelfTestRun {
    public static let size = NSSize(width: 1320, height: 860)
    private let options: SelfTestOptions
    private let store: RegionStore
    private let window: NSWindow

    public init(options: SelfTestOptions, store: RegionStore) {
        self.options = options
        self.store = store
        // SwiftUI stops updating a window the system sees as hidden, so "headless" is a real window nobody can see or reach.
        let frame = NSRect(origin: NSPoint(x: 40, y: 40), size: Self.size)
        window = NSWindow(contentRect: frame, styleMask: [.titled, .closable, .fullSizeContentView], backing: .buffered, defer: false)
        window.title = Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Osprey"
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: .darkAqua)
        // Whatever a person types or clicks meanwhile belongs to them, and must not reach the model.
        window.ignoresMouseEvents = true
        if options.headless { window.alphaValue = 0 }
        let content = NSHostingView(rootView: RootView(store: store))
        content.sizingOptions = []
        content.frame = NSRect(origin: .zero, size: Self.size)
        content.autoresizingMask = [.width, .height]
        window.contentView = content
    }

    private var regions: SelfTestRegions {
        SelfTestRegions(nodes: store.cells.map { (key: $0.key, node: $0.value.node) })
    }

    private func perform(_ send: SelfTestSend) {
        guard let cell = store.cell(send.region) else { return }
        store.send(region: send.region, revision: cell.revision, ordinal: send.ordinal, value: send.value)
    }

    private func settle() async {
        try? await Task.sleep(for: .milliseconds(300))
        window.contentView?.layoutSubtreeIfNeeded()
    }

    /// Every text view the window's selection groups hold, in the tree's order.
    private func selectableTexts() -> [SelectableTextView] {
        var found: [SelectableTextView] = []
        var pending = window.contentView.map { [$0] } ?? []
        while let view = pending.popLast() {
            if let text = view as? SelectableTextView { found.append(text) }
            pending.append(contentsOf: view.subviews)
        }
        return found
    }

    /// The groups' selected words, each group once, in the tree's order.
    private var selectedText: String {
        var seen: Set<ObjectIdentifier> = []
        var text = ""
        for view in selectableTexts() {
            guard let group = view.group, seen.insert(ObjectIdentifier(group)).inserted else { continue }
            if !text.isEmpty { text += "\n" }
            text += group.selectedText
        }
        return text
    }

    // In-process events built with the window's own number, so real hit testing runs through
    // SwiftUI; never global input.
    private func send(_ type: NSEvent.EventType, at point: NSPoint) {
        guard let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1) else { return }
        window.sendEvent(event)
    }

    /// A drag from the first word to the second; nil once sent, a reason while the words are not on show.
    /// A drag from the first word to the second through the window's real events, so SwiftUI's
    /// own hit testing runs; the span covers both words whether the drag runs up or down.
    private func selectAcross(_ words: [String]) -> String? {
        guard let from = selectableTexts().first(where: { $0.holds(words[0]) }), let group = from.group,
              let to = group.ordered.first(where: { $0.holds(words[1]) }),
              let fromAt = group.ordered.firstIndex(where: { $0 === from }), let toAt = group.ordered.firstIndex(where: { $0 === to }),
              let start = from.point(words[0], after: fromAt > toAt), let end = to.point(words[1], after: fromAt <= toAt) else {
            return "no selection group shows both \"\(words[0])\" and \"\(words[1])\""
        }
        send(.leftMouseDown, at: start)
        send(.leftMouseDragged, at: end)
        send(.leftMouseUp, at: end)
        return nil
    }

    /// A click on one word, which clears whatever was selected.
    private func clickOn(_ words: String) -> String? {
        guard let view = selectableTexts().first(where: { $0.holds(words) }), let at = view.point(words, after: true) else { return "no text in a selection group shows \"\(words)\"" }
        send(.leftMouseDown, at: at)
        send(.leftMouseUp, at: at)
        return nil
    }

    /// Cmd-V through the event queue with a small picture on this run's own pasteboard, which is
    /// the route a person's paste takes; their clipboard stays theirs.
    private func pasteImage() throws {
        let board = NSPasteboard(name: NSPasteboard.Name("Osprey self-test paste"))
        board.clearContents()
        let image = NSImage(size: NSSize(width: 4, height: 4), flipped: false) { rect in
            NSColor.systemTeal.setFill()
            rect.fill()
            return true
        }
        guard let tiff = image.tiffRepresentation, board.setData(tiff, forType: .tiff),
              let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command, timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber, context: nil, characters: "v", charactersIgnoringModifiers: "v", isARepeat: false, keyCode: 9) else {
            throw SelfTestProblem("the self-test could not put a picture on its pasteboard")
        }
        PastedImage.pasteboard = board
        NSApp.postEvent(event, atStart: false)
    }

    /// A real click where the button named `name` is drawn, so a control that only looks like a
    /// button, or sits under something else, fails here where `invoke` would not.
    private func pressOn(_ name: String) -> String? {
        guard let frame = ButtonFrames.frames[name], let height = window.contentView?.bounds.height else { return "no button on screen is named \"\(name)\"" }
        let at = NSPoint(x: frame.midX, y: height - frame.midY)
        send(.leftMouseDown, at: at)
        send(.leftMouseUp, at: at)
        return nil
    }

    /// Cmd-A: the key event goes to the window, and to the responder holding the selection,
    /// because a window that is not key drops key events before the responder chain runs.
    private func pressCommand(_ letter: String) -> String? {
        guard let responder = window.firstResponder, responder !== window,
              let character = letter.first,
              let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command, timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber, context: nil, characters: String(character), charactersIgnoringModifiers: String(character), isARepeat: false, keyCode: 0) else { return "no first responder answers a key" }
        window.sendEvent(event)
        responder.keyDown(with: event)
        // A window that is not key drops the key before its event loop reads it, so the command
        // travels the rest of the way as the action the key would have sent.
        NSApp.sendAction(#selector(NSTextView.selectAll(_:)), to: responder, from: nil)
        return nil
    }

    /// Copy through the responder chain, checked against the pasteboard this run named.
    private func copySelected(_ expected: String) -> String? {
        guard let board = SelectionGroup.pasteboard else { return "the self-test named no pasteboard" }
        guard let responder = window.firstResponder, responder !== window, NSApp.sendAction(#selector(NSTextView.copy(_:)), to: responder, from: nil) else {
            return "no responder in the window answers copy"
        }
        let copied = board.string(forType: .string) ?? ""
        return copied == expected ? nil : "copied \"\(copied)\" where \"\(expected)\" was wanted"
    }

    private func hold(_ reason: String, until deadline: Date) async throws {
        guard Date() < deadline else { throw SelfTestProblem("Gave up after \(Int(options.patience)) s: \(reason)") }
        try await Task.sleep(for: .milliseconds(50))
    }

    // Always twice the window's points, whatever screen the runner has.
    private func picture() throws -> Data {
        guard let view = window.contentView, let representation = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(Self.size.width) * 2, pixelsHigh: Int(Self.size.height) * 2, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else {
            throw SelfTestProblem("The window cannot be drawn to a picture")
        }
        representation.size = Self.size
        view.cacheDisplay(in: view.bounds, to: representation)
        guard let data = representation.representation(using: .png, properties: [:]) else { throw SelfTestProblem("The picture cannot be written") }
        return data
    }

    /// The first picture no different from the one 100 ms before: a fixed wait can catch a layout mid-move on a loaded machine.
    private func steadyPicture() async throws -> Data {
        var earlier = try picture()
        for _ in 0..<50 {
            try await Task.sleep(for: .milliseconds(100))
            window.contentView?.layoutSubtreeIfNeeded()
            let later = try picture()
            if let moved = SelfTestPicture.difference(earlier, later), moved <= SelfTestPicture.tolerance { return later }
            earlier = later
        }
        throw SelfTestProblem("The window never came to rest")
    }

    private func record(_ name: String) async throws {
        let shots = URL(fileURLWithPath: options.shots)
        let data = try await steadyPicture()
        try data.write(to: shots.appendingPathComponent("\(name).png"))
        try regions.outline.write(to: shots.appendingPathComponent("\(name).access.txt"), atomically: true, encoding: .utf8)
        guard let goldens = options.goldens else { return }
        let golden = URL(fileURLWithPath: goldens).appendingPathComponent("\(name).png")
        if options.updateGoldens {
            try FileManager.default.createDirectory(atPath: goldens, withIntermediateDirectories: true)
            try data.write(to: golden)
            return
        }
        guard let expected = try? Data(contentsOf: golden) else { throw SelfTestProblem("No golden for \(name) at \(golden.path)") }
        guard let difference = SelfTestPicture.difference(data, expected) else { throw SelfTestProblem("\(name) cannot be compared with its golden") }
        guard difference <= SelfTestPicture.tolerance else {
            throw SelfTestProblem("\(name) differs from its golden in \(String(format: "%.2f", difference * 100))% of its pixels")
        }
    }

    // A modal panel keeps the main actor's queue to itself, so this wait rides a timer in `.common`, the one mode set a panel's run loop still serves.
    private static func cancelOpenPanel(deadline: Date, message: String) async throws {
        try await withCheckedThrowingContinuation { continuation in
            let timer = Timer(timeInterval: 0.05, repeats: true) { timer in
                if let panel = NSApp.modalWindow as? NSOpenPanel, panel.isVisible {
                    timer.invalidate()
                    panel.cancel(nil)
                    continuation.resume()
                } else if Date() >= deadline {
                    timer.invalidate()
                    continuation.resume(throwing: SelfTestProblem(message))
                }
            }
            RunLoop.main.add(timer, forMode: .common)
        }
    }

    private func play(_ step: SelfTestStep) async throws {
        let deadline = Date().addingTimeInterval(options.patience)
        while true {
            if let failure = store.failure { throw SelfTestProblem(failure) }
            switch try regions.attempt(step) {
            case let .sending(send):
                if let send { perform(send) }
                return
            case let .recording(name):
                await settle()
                try await record(name)
                return
            case let .sleeping(seconds):
                try await Task.sleep(for: .seconds(seconds))
                return
            case .panel:
                try await Self.cancelOpenPanel(deadline: deadline, message: "Gave up after \(Int(options.patience)) s: no open panel is showing")
                return
            case let .selecting(words):
                guard let problem = selectAcross(words) else { return }
                try await hold(problem, until: deadline)
            case let .checkingSelected(words, contains):
                let selected = selectedText
                guard contains ? selected.contains(words) : selected == words else {
                    try await hold("the selection is \"\(selected)\"", until: deadline)
                    break
                }
                return
            case let .copying(expected):
                guard let problem = copySelected(expected) else { return }
                try await hold(problem, until: deadline)
            case let .clicking(words):
                guard let problem = clickOn(words) else { return }
                try await hold(problem, until: deadline)
            case let .pressing(name):
                guard let problem = pressOn(name) else { return }
                try await hold(problem, until: deadline)
            case .pastingImage:
                try pasteImage()
                return
            case let .keyPress(letter):
                guard let problem = pressCommand(letter) else { return }
                try await hold(problem, until: deadline)
            case let .waiting(reason):
                guard Date() < deadline else { throw SelfTestProblem("Gave up after \(Int(options.patience)) s: \(reason)") }
                try await Task.sleep(for: .milliseconds(50))
            }
        }
    }

    /// Runs the script; true when every step held. Every failure prints what was on show.
    public func run() async -> Bool {
        do {
            let steps = try SelfTestStep.parse(String(contentsOfFile: options.script, encoding: .utf8))
            try FileManager.default.createDirectory(atPath: options.shots, withIntermediateDirectories: true)
            ButtonFrames.recording = true
            // The person's clipboard stays theirs: copy checks read a pasteboard this run names.
            SelectionGroup.pasteboard = NSPasteboard(name: NSPasteboard.Name("Osprey self-test"))
            defer { SelectionGroup.pasteboard = nil }
            NSApp.setActivationPolicy(options.headless ? .accessory : .regular)
            window.orderFrontRegardless()
            store.start()
            for (index, step) in steps.enumerated() {
                try await play(step)
                print("ok \(index + 1) \(step.verb) \(step.words.first.map { String($0.prefix(60)) } ?? "")")
            }
            await settle()
            if let failure = store.failure { throw SelfTestProblem(failure) }
            print("self-test passed: \(steps.count) steps")
            return true
        } catch {
            print("FAILED: \(error)\n\(regions.outline)")
            return false
        }
    }
}

#endif
