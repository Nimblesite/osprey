import Foundation

struct FilmCue: Equatable {
    let start: Double
    let end: Double
    let text: String
}

enum CaptionFailure: LocalizedError {
    case invalid(String)
    var errorDescription: String? { if case let .invalid(message) = self { message } else { nil } }
}

// Implements [LESSONS-FRAMEWORK]: bundled WebVTT is timed against playback,
// and malformed or missing captions prevent an uncaptioned silent fallback.
enum FilmCaptions {
    static func load(_ path: String) throws -> [FilmCue] {
        guard let url = Bundled.url(path) else { throw CaptionFailure.invalid("Invalid caption path") }
        return try parse(Bounded.text(Bounded.read(url.path, limit: 65536)))
    }

    static func parse(_ source: String) throws -> [FilmCue] {
        guard source.utf8.count <= 65536 else { throw CaptionFailure.invalid("Captions exceed 64 KiB") }
        let unmarked = source.hasPrefix("\u{FEFF}") ? String(source.dropFirst()) : source
        let normalized = unmarked.replacingOccurrences(of: "\r\n", with: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        let blocks = normalized.components(separatedBy: "\n\n").filter { !$0.isEmpty }
        let header = blocks.first?.components(separatedBy: "\n").first ?? ""
        guard header == "WEBVTT" || header.hasPrefix("WEBVTT ") || header.hasPrefix("WEBVTT\t") else { throw CaptionFailure.invalid("Missing WEBVTT header") }
        let cues = try blocks.dropFirst().filter { !$0.hasPrefix("NOTE") }.map(cue)
        guard !cues.isEmpty else { throw CaptionFailure.invalid("The caption track has no cues") }
        return cues
    }

    static func text(_ cues: [FilmCue], at seconds: Double) -> String {
        cues.filter { $0.start <= seconds && seconds < $0.end }.map(\.text).joined(separator: "\n")
    }

    private static func cue(_ block: String) throws -> FilmCue {
        let lines = block.components(separatedBy: "\n")
        guard let index = lines.firstIndex(where: { $0.contains(" --> ") }), index <= 1 else { throw CaptionFailure.invalid("Invalid caption timing") }
        let timing = lines[index].components(separatedBy: " --> ")
        guard timing.count == 2, let finish = timing.last?.split(separator: " ").first else { throw CaptionFailure.invalid("Invalid caption timing") }
        let start = try timestamp(timing[0]), end = try timestamp(String(finish))
        let raw = lines.dropFirst(index + 1).joined(separator: "\n")
        guard end > start, !raw.isEmpty, raw.range(of: "</?[A-Za-z][^>]*>", options: .regularExpression) == nil else { throw CaptionFailure.invalid("Invalid or unsupported caption cue") }
        return FilmCue(start: start, end: end, text: entities(raw))
    }

    private static func entities(_ source: String) -> String {
        [("&lt;", "<"), ("&gt;", ">"), ("&nbsp;", "\u{00A0}"), ("&lrm;", "\u{200E}"), ("&rlm;", "\u{200F}"), ("&amp;", "&")]
            .reduce(source) { text, entity in text.replacingOccurrences(of: entity.0, with: entity.1) }
    }

    private static func timestamp(_ source: String) throws -> Double {
        let parts = source.components(separatedBy: ":")
        guard parts.count == 2 || parts.count == 3 else { throw CaptionFailure.invalid("Invalid caption timestamp") }
        let tail = Array(parts.suffix(2))
        guard let minutes = Int(tail[0]), minutes >= 0, minutes < 60, let seconds = Double(tail[1]), seconds.isFinite, seconds >= 0, seconds < 60 else { throw CaptionFailure.invalid("Invalid caption timestamp") }
        let hours = parts.count == 3 ? Int(parts[0]) : 0
        guard let hours, hours >= 0 else { throw CaptionFailure.invalid("Invalid caption timestamp") }
        return Double(hours) * 3600 + Double(minutes) * 60 + seconds
    }
}
