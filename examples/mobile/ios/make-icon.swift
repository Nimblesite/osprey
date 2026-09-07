import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

// Native vector artwork, rasterized into the generated Xcode asset catalog.
let directory = URL(fileURLWithPath: CommandLine.arguments[1]).appendingPathComponent("AppIcon.appiconset")
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
guard let context = CGContext(data: nil, width: 1024, height: 1024, bitsPerComponent: 8,
                              bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                              bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else {
    throw CocoaError(.fileWriteUnknown)
}
context.setFillColor(CGColor(gray: 0, alpha: 1))
context.fill(CGRect(x: 0, y: 0, width: 1024, height: 1024))
context.scaleBy(x: 1024 / 108, y: 1024 / 108)
context.translateBy(x: 0, y: 108)
context.scaleBy(x: 1, y: -1)
context.setStrokeColor(CGColor(red: 0.275, green: 0.875, blue: 0.76, alpha: 1))
context.setLineWidth(5)
context.setLineCap(.round)
context.setLineJoin(.round)
for points in [
    [(29, 35), (79, 35), (84, 66), (84, 77), (24, 77), (24, 66), (29, 35)],
    [(25, 61), (42, 61), (46, 68), (62, 68), (66, 61), (83, 61)],
    [(39, 45), (69, 45)], [(43, 53), (65, 53)]
] {
    context.addLines(between: points.map { CGPoint(x: CGFloat($0.0), y: CGFloat($0.1)) })
    context.strokePath()
}
guard let image = context.makeImage(),
      let destination = CGImageDestinationCreateWithURL(directory.appendingPathComponent("Icon.png") as CFURL,
                                                       UTType.png.identifier as CFString, 1, nil) else {
    throw CocoaError(.fileWriteUnknown)
}
CGImageDestinationAddImage(destination, image, nil)
guard CGImageDestinationFinalize(destination) else { throw CocoaError(.fileWriteUnknown) }
let manifest: [String: Any] = ["images": [["filename": "Icon.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"]],
                               "info": ["author": "xcode", "version": 1]]
try JSONSerialization.data(withJSONObject: manifest).write(to: directory.appendingPathComponent("Contents.json"))
