#if canImport(UIKit) && !canImport(AppKit)
import Foundation
import OspreyShared
import SwiftUI
import UIKit

// Implements [UI-MACHINE-SELF-TEST] on iOS: the running app plays an application's self-test.json,
// the same file the browser, macOS and Android hosts play, through the shared SelfTestRegions.
// Each step acts on what a screen reader names and reaches the app as the control's own event;
// each `shot` draws the app's window to Documents/shots/<name>.png beside its outline. ios.sh
// compares the shots with the app's iOS goldens and reads the verdict from Documents/self-test.txt.
@MainActor
final class IOSJourney {
    static let patience: TimeInterval = 30
    private static let settle = Duration.milliseconds(700)

    private let store: RegionStore
    private let script: URL

    init(store: RegionStore, script: String) {
        self.store = store
        self.script = documents.appendingPathComponent(script)
    }

    /// The script a launch names after `--self-test`, a path inside the app's Documents.
    nonisolated static func named(in launch: [String]) -> String? {
        guard let at = launch.firstIndex(of: "--self-test"), at + 1 < launch.count else { return nil }
        return launch[at + 1]
    }

    /// Plays the script, writes the verdict and ends the process with it.
    func run() async {
        var verdict = "OSPREY_IOS_SELF_TEST_OK"
        do {
            try await play()
        } catch {
            verdict = "OSPREY_IOS_SELF_TEST_FAILED \(error)\n\(regions.outline)"
        }
        print(verdict)
        try? verdict.write(to: documents.appendingPathComponent("self-test.txt"), atomically: true, encoding: .utf8)
        exit(verdict.hasPrefix("OSPREY_IOS_SELF_TEST_OK") ? 0 : 1)
    }

    private var regions: SelfTestRegions {
        SelfTestRegions(nodes: store.cells.map { (key: $0.key, node: $0.value.node) })
    }

    private var shots: URL { documents.appendingPathComponent("shots") }

    private func play() async throws {
        let steps = try SelfTestStep.parse(String(contentsOf: script, encoding: .utf8))
        try? FileManager.default.removeItem(at: shots)
        try FileManager.default.createDirectory(at: shots, withIntermediateDirectories: true)
        for (index, step) in steps.enumerated() {
            try await perform(step)
            print("ok \(index + 1) \(step.verb) \(step.words.joined(separator: " · "))")
        }
    }

    private func perform(_ step: SelfTestStep) async throws {
        let deadline = Date().addingTimeInterval(Self.patience)
        while true {
            if let failure = store.failure { throw SelfTestProblem(failure) }
            switch try regions.attempt(step) {
            case let .sending(send):
                if let send, let cell = store.cell(send.region) {
                    store.send(region: send.region, revision: cell.revision, ordinal: send.ordinal, value: send.value)
                }
                return
            case let .recording(name):
                try await record(name)
                return
            case let .sleeping(seconds):
                try await Task.sleep(for: .seconds(seconds))
                return
            case let .waiting(reason):
                guard Date() < deadline else { throw SelfTestProblem("Gave up after \(Int(Self.patience)) s: \(reason)") }
                try await Task.sleep(for: .milliseconds(50))
            default:
                throw SelfTestProblem("\(step.verb) has no touch-screen form")
            }
        }
    }

    /// The app's window as the person sees it, without the system's status bar.
    private func record(_ name: String) async throws {
        try await Task.sleep(for: Self.settle)
        let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap(\.windows)
        guard let window = windows.first(where: \.isKeyWindow) ?? windows.first else { throw SelfTestProblem("The app has no window to draw") }
        // 8-bit sRGB, the PNG every host's goldens compare; the display's wide range would write 16 bits.
        let format = UIGraphicsImageRendererFormat(for: window.traitCollection)
        format.preferredRange = .standard
        let picture = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        guard let data = picture.pngData() else { throw SelfTestProblem("\(name) cannot be written") }
        try data.write(to: shots.appendingPathComponent("\(name).png"))
        try regions.outline.write(to: shots.appendingPathComponent("\(name).access.txt"), atomically: true, encoding: .utf8)
    }
}

/// The app's Documents folder, where a journey's script, shots and verdict live.
private var documents: URL {
    FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first ?? URL(fileURLWithPath: NSTemporaryDirectory())
}
#endif
