// Compares two window captures pixel by pixel inside a rectangle given in capture pixels and
// writes a mask of the differing pixels. Usage: swift diff.swift ref.png port.png x0 y0 x1 y1 out.png
import AppKit
import CoreGraphics

func pixels(_ path: String) -> (CGImage, [UInt8], Int, Int) {
    let image = NSImage(contentsOfFile: path)!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
    let w = image.width, h = image.height
    var data = [UInt8](repeating: 0, count: w * h * 4)
    let context = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
    return (image, data, w, h)
}

let a = CommandLine.arguments
let (_, left, lw, _) = pixels(a[1])
let (_, right, rw, _) = pixels(a[2])
let x0 = Int(a[3])!, y0 = Int(a[4])!, x1 = Int(a[5])!, y1 = Int(a[6])!
var differing = 0, worst = 0
var mask = [UInt8](repeating: 255, count: (x1 - x0) * (y1 - y0) * 4)
for y in y0..<y1 {
    for x in x0..<x1 {
        var delta = 0
        for c in 0..<3 {
            delta = max(delta, abs(Int(left[(y * lw + x) * 4 + c]) - Int(right[(y * rw + x) * 4 + c])))
        }
        if delta > 0 {
            differing += 1
            worst = max(worst, delta)
            let i = ((y - y0) * (x1 - x0) + (x - x0)) * 4
            mask[i] = 255; mask[i + 1] = 0; mask[i + 2] = 0
        }
    }
}
let total = (x1 - x0) * (y1 - y0)
print("region \(x1 - x0)x\(y1 - y0) px: \(differing) differing pixels (\(String(format: "%.4f", Double(differing) * 100 / Double(total)))%), largest channel delta \(worst)")
let out = CGContext(data: &mask, width: x1 - x0, height: y1 - y0, bitsPerComponent: 8, bytesPerRow: (x1 - x0) * 4,
                    space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let rep = NSBitmapImageRep(cgImage: out.makeImage()!)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[7]))
