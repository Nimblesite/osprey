#if os(macOS)
import AppKit
import UniformTypeIdentifiers

/// A picture pasted into a field: saved as a PNG in one folder, named in the words by a marker
/// the application reads. A file copied in Finder counts when it is a picture.
enum PastedImage {
    /// Where a paste reads; a self-test points this at a named pasteboard.
    @MainActor static var pasteboard: NSPasteboard?
    /// The multiline fields holding the keyboard with their paste monitor on: a Cmd-V reaches one
    /// of them as a picture, so a self-test pastes only once this is not empty.
    @MainActor static var receivers: Set<UUID> = []
    private static let longestSide: CGFloat = 2048

    static var folder: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/Osprey/pasted", isDirectory: true)
    }

    /// The marker for the picture on the pasteboard; nil when it holds none, or words to paste instead.
    @MainActor static func take() -> String? {
        let board = pasteboard ?? .general
        guard let image = picture(on: board), let data = png(image) else { return nil }
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            let name = UUID().uuidString.lowercased() + ".png"
            try data.write(to: folder.appendingPathComponent(name), options: .atomic)
            return "[image: \(name)]"
        } catch {
            Log.events.error("A pasted picture could not be saved: \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }

    private static func picture(on board: NSPasteboard) -> NSImage? {
        let files = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingContentsConformToTypes: [UTType.image.identifier]]) as? [URL]
        if let file = files?.first { return NSImage(contentsOf: file) }
        let hasPicture = board.availableType(from: [.png, .tiff]) != nil
        return hasPicture && board.string(forType: .string) == nil ? NSImage(pasteboard: board) : nil
    }

    private static func png(_ image: NSImage) -> Data? {
        let scale = min(1, longestSide / max(image.size.width, image.size.height, 1))
        let size = NSSize(width: (image.size.width * scale).rounded(), height: (image.size.height * scale).rounded())
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width), pixelsHigh: Int(size.height), bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
              let context = NSGraphicsContext(bitmapImageRep: bitmap) else { return nil }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        image.draw(in: NSRect(origin: .zero, size: size))
        NSGraphicsContext.restoreGraphicsState()
        return bitmap.representation(using: .png, properties: [:])
    }
}
#endif
