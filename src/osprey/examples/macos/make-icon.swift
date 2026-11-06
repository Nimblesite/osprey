import AppKit
import UniformTypeIdentifiers

// The app icon: the internal-drive symbol the volume gauge shows, in a red
// gradient on a graphite tile. The Mac gets its rounded tile with a margin and
// shadow in every size it asks for; iPhone gets one full-bleed square that the
// system masks.
let catalog = URL(fileURLWithPath: CommandLine.arguments[1])
let directory = catalog.appendingPathComponent("AppIcon.appiconset")
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

func color(_ hex: UInt32) -> CGColor {
    CGColor(red: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

func gradient(_ colors: [UInt32]) -> CGGradient {
    CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors.map(color) as CFArray, locations: nil)!
}

/// The symbol as a mask the size of `rect`.
func symbolMask(_ rect: CGRect) -> CGImage? {
    let configuration = NSImage.SymbolConfiguration(pointSize: rect.height, weight: .regular)
    guard let symbol = NSImage(systemSymbolName: "internaldrive.fill", accessibilityDescription: nil)?
        .withSymbolConfiguration(configuration) else { return nil }
    var proposed = CGRect(origin: .zero, size: symbol.size)
    return symbol.cgImage(forProposedRect: &proposed, context: nil, hints: nil)
}

func draw(_ context: CGContext, side: CGFloat, mac: Bool) {
    let tile = mac ? CGRect(x: side * 0.1, y: side * 0.1, width: side * 0.8, height: side * 0.8) : CGRect(x: 0, y: 0, width: side, height: side)
    let shape = CGPath(roundedRect: tile, cornerWidth: mac ? tile.width * 0.225 : 0, cornerHeight: mac ? tile.width * 0.225 : 0, transform: nil)
    if mac {
        context.saveGState()
        context.setShadow(offset: CGSize(width: 0, height: -side * 0.012), blur: side * 0.03, color: CGColor(gray: 0, alpha: 0.35))
        context.addPath(shape)
        context.setFillColor(color(0x1B1E23))
        context.fillPath()
        context.restoreGState()
    }
    context.saveGState()
    context.addPath(shape)
    context.clip()
    context.drawLinearGradient(gradient([0x3A3F48, 0x15171B]), start: CGPoint(x: 0, y: tile.maxY), end: CGPoint(x: 0, y: tile.minY), options: [])
    context.drawRadialGradient(gradient([0x4B2A2A, 0x15171B]), startCenter: CGPoint(x: tile.midX, y: tile.midY), startRadius: 0,
                               endCenter: CGPoint(x: tile.midX, y: tile.midY), endRadius: tile.width * 0.75, options: [.drawsAfterEndLocation])
    context.setBlendMode(.normal)
    guard let mask = symbolMask(tile) else { return context.restoreGState() }
    let aspect = CGFloat(mask.width) / CGFloat(mask.height)
    let width = tile.width * 0.7
    let glyph = CGRect(x: tile.midX - width / 2, y: tile.midY - width / aspect / 2, width: width, height: width / aspect)
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -tile.width * 0.02), blur: tile.width * 0.05, color: CGColor(red: 0.9, green: 0.2, blue: 0.15, alpha: 0.45))
    context.clip(to: glyph, mask: mask)
    context.drawLinearGradient(gradient([0xFF8A7A, 0xF0473B, 0xC92A22]), start: CGPoint(x: 0, y: glyph.maxY), end: CGPoint(x: 0, y: glyph.minY), options: [])
    context.restoreGState()
    context.restoreGState()
}

func write(pixels: Int, mac: Bool, name: String) throws {
    guard let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: (mac ? CGImageAlphaInfo.premultipliedLast : .noneSkipLast).rawValue) else {
        throw CocoaError(.fileWriteUnknown)
    }
    draw(context, side: CGFloat(pixels), mac: mac)
    guard let image = context.makeImage(),
        let destination = CGImageDestinationCreateWithURL(directory.appendingPathComponent(name) as CFURL, UTType.png.identifier as CFString, 1, nil)
    else { throw CocoaError(.fileWriteUnknown) }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else { throw CocoaError(.fileWriteUnknown) }
}

var images: [[String: String]] = []
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let name = "mac-\(points)@\(scale)x.png"
        try write(pixels: points * scale, mac: true, name: name)
        images.append(["filename": name, "idiom": "mac", "size": "\(points)x\(points)", "scale": "\(scale)x"])
    }
}
try write(pixels: 1024, mac: false, name: "ios.png")
images.append(["filename": "ios.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"])
let info = ["author": "xcode", "version": 1] as [String: Any]
try JSONSerialization.data(withJSONObject: ["images": images, "info": info]).write(to: directory.appendingPathComponent("Contents.json"))
try JSONSerialization.data(withJSONObject: ["info": info]).write(to: catalog.appendingPathComponent("Contents.json"))
