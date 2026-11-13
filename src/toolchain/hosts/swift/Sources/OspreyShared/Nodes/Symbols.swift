import SwiftUI

// Implements [UI-COMPONENTS]: symbol artwork is a host resource, never app logic. A bundle may
// supply SVG artwork for the symbols an application names; a name without artwork falls back to
// the SF Symbol it names, then to the semantic table, so every application's vocabulary works.

#if os(macOS)
/// A bundle's `symbols.json` manifest: each semantic symbol name to its SVG resource path.
struct SymbolAssets {
    private let images: [String: NSImage]

    static func load(root: URL) throws -> SymbolAssets? {
        let manifest = try resource("symbols.json", root: root)
        guard FileManager.default.fileExists(atPath: manifest.path) else { return nil }
        let names = try JSONDecoder().decode([String: String].self, from: boundedData(manifest, limit: 65536))
        guard names.count <= 128 else { throw SymbolAssetFailure.invalid("Too many bundled symbols") }
        let entries = try names.map { name, path in
            guard name.range(of: "^[A-Za-z0-9._-]{1,128}$", options: .regularExpression) != nil else { throw SymbolAssetFailure.invalid("Invalid semantic symbol name") }
            return (name, try artwork(path, root: root))
        }
        return SymbolAssets(images: Dictionary(uniqueKeysWithValues: entries))
    }

    func image(_ name: String) -> NSImage? {
        images[name]
    }

    private static func artwork(_ path: String, root: URL) throws -> NSImage {
        guard path.count <= 256, path.range(of: "^(?:[A-Za-z0-9_-]+/)*[A-Za-z0-9_-]+\\.svg$", options: .regularExpression) != nil else { throw SymbolAssetFailure.invalid("Invalid bundled symbol path") }
        let file = try resource(path, root: root)
        guard let image = NSImage(data: try svgData(file)), image.size.width > 0, image.size.height > 0 else { throw SymbolAssetFailure.invalid("Bundled symbol cannot be decoded: \(path)") }
        image.isTemplate = true
        return image
    }

    private static func svgData(_ file: URL) throws -> Data {
        let data = try boundedData(file, limit: 262144)
        guard let source = String(data: data, encoding: .utf8), !source.contains("<!DOCTYPE"), !source.contains("<!ENTITY") else { throw SymbolAssetFailure.invalid("Bundled symbol must contain SVG without document entities") }
        let document = try XMLDocument(data: data, options: [.nodeLoadExternalEntitiesNever])
        guard document.rootElement()?.name == "svg", document.rootElement()?.uri == "http://www.w3.org/2000/svg" else { throw SymbolAssetFailure.invalid("Bundled symbol must contain an SVG document") }
        return data
    }

    private static func resource(_ path: String, root: URL) throws -> URL {
        let base = root.resolvingSymlinksInPath().standardizedFileURL
        let file = base.appendingPathComponent(path).resolvingSymlinksInPath().standardizedFileURL
        guard base.isFileURL, file.path.hasPrefix(base.path + "/") else { throw SymbolAssetFailure.invalid("Bundled symbol escapes its resources") }
        return file
    }

    private static func boundedData(_ file: URL, limit: Int) throws -> Data {
        let handle = try FileHandle(forReadingFrom: file)
        let data = try handle.read(upToCount: limit + 1) ?? Data()
        try handle.close()
        guard data.count <= limit else { throw SymbolAssetFailure.invalid("Bundled symbol resource exceeds its size limit") }
        return data
    }
}

enum SymbolAssetFailure: LocalizedError {
    case invalid(String)

    var errorDescription: String? {
        switch self { case let .invalid(reason): return reason }
    }
}

private struct SymbolAssetsKey: EnvironmentKey {
    static let defaultValue: Result<SymbolAssets?, Error> = Result {
        guard let root = Bundle.main.resourceURL else { return nil }
        return try SymbolAssets.load(root: root)
    }
}

extension EnvironmentValues {
    var ospreySymbols: Result<SymbolAssets?, Error> {
        get { self[SymbolAssetsKey.self] }
        set { self[SymbolAssetsKey.self] = newValue }
    }
}
#endif

/// The symbol an application names: bundled artwork where the bundle has it, else the SF Symbol
/// the name spells, else the semantic table's, else the host's visible refusal mark. A system
/// symbol keeps the font it was given, so a host without bundled artwork draws as it did before.
struct NativeSymbol: View {
    let name: String
    var size: CGFloat
    var weight: Font.Weight = .regular

    var body: some View {
        #if os(macOS)
        if let image = artwork {
            Image(nsImage: image).renderingMode(.template).resizable().scaledToFit()
                .frame(width: size, height: size).accessibilityHidden(true)
        } else {
            Image(systemName: resolved).font(.system(size: size, weight: weight)).accessibilityHidden(true)
        }
        #else
        Image(systemName: Symbols.name(name)).font(.system(size: size, weight: weight)).accessibilityHidden(true)
        #endif
    }

    #if os(macOS)
    @Environment(\.ospreySymbols) private var symbols

    /// The bundle's artwork for this name; a bundle without a manifest has none.
    private var artwork: NSImage? {
        switch symbols {
        case let .success(assets): return assets?.image(name)
        default: return nil
        }
    }

    /// A name the platform knows as its own, else the semantic table's SF Symbol.
    private var resolved: String {
        NSImage(systemSymbolName: name, accessibilityDescription: nil) != nil ? name : Symbols.name(name)
    }
    #endif
}
