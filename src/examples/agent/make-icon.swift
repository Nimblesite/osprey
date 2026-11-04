import AppKit

// Draws Osprey Agent's icon.png: the Osprey bird on the app's dark tile, with the
// margin and shadow a Mac icon has. `src/hosts/swift/run.sh` turns icon.png into the bundle's icon.
//   swift make-icon.swift ../../vscode-extension/icon.png icon.png
let side = 1024
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let space = CGColorSpaceCreateDeviceRGB()

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(red: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
}

/// The art's pixels with its near-white background taken out, whether or not the file has alpha: lightness above the bird's lightest blue fades to clear.
func cutOut(_ art: CGImage) -> CGImage {
    let context = CGContext(data: nil, width: art.width, height: art.height, bitsPerComponent: 8, bytesPerRow: art.width * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.draw(art, in: CGRect(x: 0, y: 0, width: art.width, height: art.height))
    let pixels = context.data!.bindMemory(to: UInt8.self, capacity: art.width * art.height * 4)
    for index in stride(from: 0, to: art.width * art.height * 4, by: 4) {
        // How far a pixel is from white, with red the channel that separates blue from white.
        let keep = min(255, max(0, (255 - Int(pixels[index])) * 255 / 40))
        pixels[index] = UInt8(Int(pixels[index]) * keep / 255)
        pixels[index + 1] = UInt8(Int(pixels[index + 1]) * keep / 255)
        pixels[index + 2] = UInt8(Int(pixels[index + 2]) * keep / 255)
        pixels[index + 3] = UInt8(Int(pixels[index + 3]) * keep / 255)
    }
    return context.makeImage()!
}

let source = NSImage(contentsOfFile: CommandLine.arguments[1])!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
let canvas = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: side * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let shape = CGPath(roundedRect: tile, cornerWidth: 186, cornerHeight: 186, transform: nil)

canvas.saveGState()
canvas.setShadow(offset: CGSize(width: 0, height: -14), blur: 34, color: color(0x000000, 0.42))
canvas.addPath(shape)
canvas.setFillColor(color(0x17171C))
canvas.fillPath()
canvas.restoreGState()

canvas.saveGState()
canvas.addPath(shape)
canvas.clip()
let fill = CGGradient(colorsSpace: space, colors: [color(0x2A2A35), color(0x101014)] as CFArray, locations: nil)!
canvas.drawLinearGradient(fill, start: CGPoint(x: 512, y: tile.maxY), end: CGPoint(x: 512, y: tile.minY), options: [])
let glow = CGGradient(colorsSpace: space, colors: [color(0x6FD8FF, 0.20), color(0x6FD8FF, 0)] as CFArray, locations: nil)!
canvas.drawRadialGradient(glow, startCenter: CGPoint(x: 540, y: 520), startRadius: 0, endCenter: CGPoint(x: 540, y: 520), endRadius: 440, options: [])
canvas.draw(cutOut(source), in: CGRect(x: 137, y: 172, width: 750, height: 750))
canvas.restoreGState()

let output = NSBitmapImageRep(cgImage: canvas.makeImage()!).representation(using: .png, properties: [:])!
try output.write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
