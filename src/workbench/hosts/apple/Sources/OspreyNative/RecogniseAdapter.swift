import AppKit
import Foundation
import Vision

// Implements [UI-EFFECTS-RECOGNISE] for the Apple host: the strokes held
// under the named slices are drawn into a picture and read with Vision's text
// recogniser. The result is the text read, bounded by the request's limit.

public struct RecogniseAdapter: CapabilityAdapter {
    public let kinds = ["recognise"]
    public init() {}

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let limit = try request.number("limit")
        let strokes = try request.texts("slices").compactMap { try? JSONDecoder().decode(Stroke.self, from: Data(slices.text($0).utf8)) }
        guard !strokes.isEmpty, let picture = RecogniseAdapter.picture(strokes) else { throw CapabilityError.invalidArgument("No strokes to read") }
        let text = try await RecogniseAdapter.read(picture)
        guard text.utf8.count <= limit else { throw CapabilityError.tooLarge(text.utf8.count, limit) }
        return .object(["text": .string(text)])
    }

    /// Black strokes on white, framed by their bounds with a margin.
    static func picture(_ strokes: [Stroke]) -> CGImage? {
        let points = strokes.flatMap(\.points).filter { $0.count == 2 }
        guard let left = points.map({ $0[0] }).min(), let right = points.map({ $0[0] }).max(), let top = points.map({ $0[1] }).min(), let bottom = points.map({ $0[1] }).max() else { return nil }
        let margin = 24, width = right - left + 2 * margin, height = bottom - top + 2 * margin
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue) else { return nil }
        context.setFillColor(gray: 1, alpha: 1)
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.setStrokeColor(gray: 0, alpha: 1)
        context.setLineCap(.round)
        context.setLineJoin(.round)
        for stroke in strokes {
            let mapped = stroke.points.filter { $0.count == 2 }.map { CGPoint(x: $0[0] - left + margin, y: height - ($0[1] - top + margin)) }
            guard let first = mapped.first else { continue }
            context.setLineWidth(CGFloat(max(stroke.width, 3)))
            context.move(to: first)
            mapped.dropFirst().forEach { context.addLine(to: $0) }
            if mapped.count == 1 { context.addLine(to: CGPoint(x: first.x + 0.5, y: first.y)) }
            context.strokePath()
        }
        return context.makeImage()
    }

    static func read(_ picture: CGImage) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            let request = VNRecognizeTextRequest { request, error in
                if let error { return continuation.resume(throwing: CapabilityError.platform(error.localizedDescription)) }
                let lines = (request.results as? [VNRecognizedTextObservation] ?? []).compactMap { $0.topCandidates(1).first?.string }
                continuation.resume(returning: lines.joined(separator: " "))
            }
            request.recognitionLevel = .accurate
            request.usesLanguageCorrection = false
            do { try VNImageRequestHandler(cgImage: picture).perform([request]) } catch { continuation.resume(throwing: CapabilityError.platform(error.localizedDescription)) }
        }
    }
}
