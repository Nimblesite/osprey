import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

// [CAST-DESIGN-ICON] Package the existing identity; UIKit supplies its mask.
func icon(_ path: String) throws -> CGImage {
    guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
          let context = CGContext(data: nil, width: 1024, height: 1024, bitsPerComponent: 8,
                                  bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else {
        throw CocoaError(.fileReadCorruptFile)
    }
    let bounds = CGRect(x: 0, y: 0, width: 1024, height: 1024)
    context.setFillColor(CGColor(srgbRed: 1, green: 0.973, blue: 0.945, alpha: 1))
    context.fill(bounds)
    context.interpolationQuality = .high
    context.draw(image, in: bounds)
    guard let result = context.makeImage() else { throw CocoaError(.fileWriteUnknown) }
    return result
}

func write(_ image: CGImage, to catalog: String) throws {
    let folder = URL(fileURLWithPath: catalog).appendingPathComponent("AppIcon.appiconset")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    guard let output = CGImageDestinationCreateWithURL(folder.appendingPathComponent("Icon.png") as CFURL,
                                                      UTType.png.identifier as CFString, 1, nil) else {
        throw CocoaError(.fileWriteUnknown)
    }
    CGImageDestinationAddImage(output, image, nil)
    guard CGImageDestinationFinalize(output) else { throw CocoaError(.fileWriteUnknown) }
    let manifest: [String: Any] = ["images": [["filename": "Icon.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"]],
                                   "info": ["author": "xcode", "version": 1]]
    try JSONSerialization.data(withJSONObject: manifest).write(to: folder.appendingPathComponent("Contents.json"))
}

do {
    guard CommandLine.arguments.count == 3 else { throw CocoaError(.fileReadInvalidFileName) }
    try write(icon(CommandLine.arguments[1]), to: CommandLine.arguments[2])
} catch {
    fputs("Cannot build the casting app icon: \(error.localizedDescription)\n", stderr)
    exit(1)
}
