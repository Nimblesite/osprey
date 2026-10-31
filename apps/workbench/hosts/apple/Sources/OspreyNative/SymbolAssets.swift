import AppKit

// Implements [UI-COMPONENTS]: a bundle may supply artwork for semantic symbols.
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
