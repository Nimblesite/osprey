import Foundation
import SwiftUI
#if os(macOS)
import AppKit
#else
import UIKit
#endif

// Implements [UI-EFFECTS]: native panels and system navigation. Each needs a
// person's gesture and reports what was chosen or that nothing was. On iOS
// the document picker and share sheet stand in for the panels (Documents.swift).
// Running applications are NSWorkspace observations with no interpretation.

public struct PanelAdapter: CapabilityAdapter {
    public let kinds = ["choose-file", "choose-folder", "save-file", "reveal", "open-settings", "running-applications"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        switch request.kind {
        case "choose-file": return try await chooseFile(request, slices: slices)
        case "choose-folder": return try await choose(folders: true, types: [])
        case "save-file": return try await save(request, slices: slices)
        case "reveal": return try await reveal(try request.text("path"))
        case "open-settings": return try await openSettings(try request.text("pane"))
        default: return await running()
        }
    }

    private func chooseFile(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let chosen = try await choose(folders: false, types: try request.texts("types"))
        let slice = try request.text("slice"), limit = try request.number("limit")
        guard let path = chosen.object?["path"]?.string, !path.isEmpty, !slice.isEmpty, limit > 0 else { return chosen }
        slices.write(slice, try Bounded.text(try Bounded.read(path, limit: limit)))
        return chosen
    }

    @MainActor
    private func choose(folders: Bool, types: [String]) async throws -> JSONValue {
        #if os(macOS)
        let panel = NSOpenPanel()
        panel.canChooseDirectories = folders
        panel.canChooseFiles = !folders
        panel.allowsMultipleSelection = false
        panel.resolvesAliases = false
        if !types.isEmpty { panel.allowedContentTypes = types.compactMap { UTType(filenameExtension: $0) } }
        let response = await panel.begin()
        guard response == .OK, let url = panel.url else { return .object(["path": "", "cancelled": true]) }
        return .object(["path": .string(url.path), "cancelled": false])
        #else
        guard !folders else { throw CapabilityError.platform("Choosing a folder is unsupported on this platform") }
        return try await Documents.choose(types: types)
        #endif
    }

    @MainActor
    private func save(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        #if os(macOS)
        let panel = NSSavePanel()
        panel.nameFieldStringValue = try request.text("name")
        let response = await panel.begin()
        guard response == .OK, let url = panel.url else { return .object(["path": "", "cancelled": true]) }
        try Data(slices.text(try request.text("slice")).utf8).write(to: url, options: [.atomic])
        return .object(["path": .string(url.path), "cancelled": false])
        #else
        return try await Documents.save(name: try request.text("name"), data: Data(slices.text(try request.text("slice")).utf8))
        #endif
    }

    @MainActor
    private func reveal(_ path: String) async throws -> JSONValue {
        #if os(macOS)
        guard path.hasPrefix("/") else { throw CapabilityError.invalidArgument("Reveal needs an absolute path") }
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
        return .object([:])
        #else
        throw CapabilityError.platform("No file viewer on this platform")
        #endif
    }

    @MainActor
    private func openSettings(_ pane: String) async throws -> JSONValue {
        #if os(macOS)
        guard pane == "privacy", let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy") else { throw CapabilityError.invalidArgument("Unknown settings pane \(pane)") }
        NSWorkspace.shared.open(url)
        #else
        guard let url = URL(string: UIApplication.openSettingsURLString) else { throw CapabilityError.platform("Settings unavailable") }
        await UIApplication.shared.open(url)
        #endif
        return .object([:])
    }

    @MainActor
    private func running() async -> JSONValue {
        #if os(macOS)
        return .array(NSWorkspace.shared.runningApplications.map { app in
            .object(["pid": .number(Double(app.processIdentifier)), "launched": .number(app.launchDate?.timeIntervalSince1970 ?? 0), "active": .bool(app.isActive), "hidden": .bool(app.isHidden), "policy": .string(PanelAdapter.policy(app.activationPolicy))])
        })
        #else
        return .array([])
        #endif
    }

    #if os(macOS)
    static func policy(_ value: NSApplication.ActivationPolicy) -> String {
        switch value {
        case .regular: return "regular"
        case .accessory: return "accessory"
        case .prohibited: return "prohibited"
        @unknown default: return "unknown"
        }
    }
    #endif
}

#if os(macOS)
import UniformTypeIdentifiers
#endif

/// Every adapter this backend offers; the application target picks its store location.
public enum Adapters {
    public static func standard(store: StoreAdapter) -> [CapabilityAdapter] {
        [RunAdapter(), ListDirectoriesAdapter(), ReadPropertyListsAdapter(), ReadTextsAdapter(), FingerprintAdapter(), store, HttpAdapter(), PanelAdapter()]
    }
}
