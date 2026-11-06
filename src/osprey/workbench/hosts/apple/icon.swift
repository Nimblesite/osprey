import AppKit

// Implements [WORKBENCH-DESIGN-COLOUR]: retain the brand bird on a contrasting Dock tile.
enum IconFailure: Error { case arguments, source, bitmap, context, encoding }

func bitmap() throws -> NSBitmapImageRep {
    guard let image = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1024, pixelsHigh: 1024,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else { throw IconFailure.bitmap }
    return image
}

func artwork(_ source: NSImage, into image: NSBitmapImageRep) throws {
    guard let context = NSGraphicsContext(bitmapImageRep: image) else { throw IconFailure.context }
    NSGraphicsContext.saveGraphicsState()
    defer { NSGraphicsContext.restoreGraphicsState() }
    NSGraphicsContext.current = context
    context.imageInterpolation = .high
    NSColor(srgbRed: 12 / 255, green: 19 / 255, blue: 37 / 255, alpha: 1).setFill()
    NSBezierPath(roundedRect: CGRect(x: 64, y: 64, width: 896, height: 896), xRadius: 196, yRadius: 196).fill()
    source.draw(in: CGRect(x: 32, y: 32, width: 960, height: 960), from: .zero, operation: .sourceOver, fraction: 1)
}

func writeIcon(_ source: String, to destination: String) throws {
    guard let brand = NSImage(contentsOfFile: source) else { throw IconFailure.source }
    let image = try bitmap()
    try artwork(brand, into: image)
    guard let png = image.representation(using: .png, properties: [:]) else { throw IconFailure.encoding }
    try png.write(to: URL(fileURLWithPath: destination), options: .atomic)
}

do {
    guard CommandLine.arguments.count == 3 else { throw IconFailure.arguments }
    try writeIcon(CommandLine.arguments[1], to: CommandLine.arguments[2])
} catch {
    FileHandle.standardError.write(Data("Cannot build Workbench Dock icon: \(error)\n".utf8))
    exit(1)
}
