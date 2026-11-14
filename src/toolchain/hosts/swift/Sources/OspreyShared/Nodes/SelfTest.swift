import Foundation

#if os(macOS)
import AppKit
import SwiftUI

// Implements [UI-MACHINE-SELF-TEST]: the macOS window half. `SelfTestRun` plays a script against
// the real window and compares its pictures with goldens; the tooling a script names is in
// SelfTestOptions.swift.
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
    /// the route a person's paste takes; their clipboard stays theirs. It waits for a field to hold
    /// the keyboard with its paste monitor on: a paste while a picker still has it, or before the
    /// field's focus lands, lands nowhere.
    private func pasteImage() throws -> String? {
        guard window.firstResponder is NSText, !PastedImage.receivers.isEmpty else { return "no message field holds the keyboard to paste into" }
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
        return nil
    }

    /// A real click where the button named `name` is drawn, so a control that only looks like a
    /// button, or sits under something else, fails here where `invoke` would not.
    private func pressOn(_ name: String) -> String? {
        let content = window.contentView
        content?.layoutSubtreeIfNeeded()
        guard let frame = ButtonFrames.frames[name], let height = content?.bounds.height else { return "no button on screen is named \"\(name)\"" }
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
                guard let problem = try pasteImage() else { return }
                try await hold(problem, until: deadline)
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
            store.start(dark: NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua)
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
