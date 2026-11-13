import SwiftUI

// Implements [UI-COMPONENTS-MEDIA] for the leaves `image` and `video`. Media and file paths the
// application names are relative to the host's bundled resources, or to the application folder
// its launcher named. A film the person starts never autoplays, and its captions are timed
// against playback, with malformed captions preventing an uncaptioned silent fallback.

/// Media and file paths the application names, resolved inside the host's resources.
public enum Bundled {
    /// The application's own folder, when its launcher named one: a path the bundle holds no
    /// copy of resolves against it, so an app's media travel with its sources.
    public static var appFolder: URL?

    /// `path` inside the bundled resources or the application folder; nil when it escapes them.
    public static func url(_ path: String) -> URL? {
        let parts = path.split(separator: "/")
        guard !path.isEmpty, !path.hasPrefix("/"), !parts.contains("..") else { return nil }
        if let held = Bundle.main.resourceURL?.appendingPathComponent(path), FileManager.default.fileExists(atPath: held.path) { return held }
        guard let folder = appFolder else { return nil }
        let file = folder.appendingPathComponent(path)
        return file.standardizedFileURL.path.hasPrefix(folder.standardizedFileURL.path + "/") ? file : nil
    }

    static func image(_ path: String) -> Image? {
        guard let url = url(path) else { return nil }
        #if os(macOS)
        return NSImage(contentsOf: url).map { Image(nsImage: $0) }
        #else
        return UIImage(contentsOfFile: url.path).map { Image(uiImage: $0) }
        #endif
    }
}

/// [UI-COMPONENTS-MEDIA] A bundled picture at its authored aspect, or a visible note when missing.
struct StillLeaf: View {
    let node: Tree

    private var width: CGFloat { CGFloat(node.count("width")) }
    private var height: CGFloat { CGFloat(max(node.count("height"), 1)) }

    var body: some View {
        if let picture = Bundled.image(node.text("source")) {
            picture.resizable().aspectRatio(width / height, contentMode: .fit)
                .frame(maxWidth: width).clipShape(RoundedRectangle(cornerRadius: 8)).accessibilityLabel(node.text("alt"))
        } else {
            Label("This picture is missing: \(node.text("alt"))", systemImage: "photo").foregroundStyle(.secondary)
        }
    }
}

#if canImport(AVKit)
import AVKit
import Combine

/// [UI-COMPONENTS-MEDIA] A film the person starts: poster and play control until then, with its
/// length on the poster and its captions over playback.
struct FilmLeaf: View {
    let node: Tree
    @State private var player: AVPlayer?
    @State private var cues: [FilmCue] = []
    @State private var failure = ""

    private var seconds: Int { node.count("seconds") }

    var body: some View {
        VStack {
            if let player { FilmPlayer(player: player, cues: cues, onFailure: playbackFailed) } else { posterButton }
            if !failure.isEmpty { Text(failure).foregroundStyle(.red).accessibilityLabel(failure) }
        }
        .onDisappear { player?.pause() }
    }

    private var posterButton: some View {
        Button { start() } label: {
            ZStack(alignment: .bottomTrailing) {
                StillLeaf(node: node)
                Label(String(format: "%d:%02d", seconds / 60, seconds % 60), systemImage: "play.fill")
                    .padding(8).background(.ultraThinMaterial, in: Capsule()).padding(10)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Play: \(node.text("alt"))")
    }

    private func playbackFailed(_ message: String) {
        player?.pause()
        player = nil
        failure = "The film could not be opened: " + message
    }

    private func start() {
        do {
            guard let url = Bundled.url(node.text("source")), FileManager.default.fileExists(atPath: url.path) else { throw CaptionFailure.invalid("The film is missing") }
            cues = try FilmCaptions.load(node.text("captions"))
            let next = AVPlayer(url: url)
            next.appliesMediaSelectionCriteriaAutomatically = true
            player = next
            failure = ""
            next.play()
        } catch { failure = "The film could not be opened: \(error.localizedDescription)" }
    }
}

struct FilmPlayer: View {
    let player: AVPlayer
    let cues: [FilmCue]
    let onFailure: (String) -> Void

    private var status: AnyPublisher<AVPlayerItem.Status, Never> {
        player.currentItem?.publisher(for: \.status).receive(on: RunLoop.main).eraseToAnyPublisher() ?? Just(.unknown).eraseToAnyPublisher()
    }

    var body: some View {
        VideoPlayer(player: player) {
            TimelineView(.periodic(from: .now, by: 0.1)) { _ in
                FilmSubtitle(text: FilmCaptions.text(cues, at: player.currentTime().seconds))
            }
        }
        .aspectRatio(16.0 / 9.0, contentMode: .fit)
        .onReceive(status) { value in
            if value == .failed { onFailure(player.currentItem?.error?.localizedDescription ?? "The video could not be decoded") }
        }
    }
}

private struct FilmSubtitle: View {
    let text: String

    var body: some View {
        VStack {
            Spacer()
            if !text.isEmpty {
                Text(text).font(.custom("Geist", size: 18)).multilineTextAlignment(.center)
                    .foregroundStyle(.white).padding(8).background(.black.opacity(0.85), in: RoundedRectangle(cornerRadius: 4)).padding(24)
            }
        }
        .allowsHitTesting(false)
    }
}
#endif

struct FilmCue: Equatable {
    let start: Double
    let end: Double
    let text: String
}

enum CaptionFailure: LocalizedError {
    case invalid(String)
    var errorDescription: String? { if case let .invalid(message) = self { message } else { nil } }
}

/// Bundled WebVTT, timed against playback; malformed or missing captions prevent an
/// uncaptioned silent fallback.
enum FilmCaptions {
    static func load(_ path: String) throws -> [FilmCue] {
        guard let url = Bundled.url(path) else { throw CaptionFailure.invalid("Invalid caption path") }
        return try parse(try Bounded.text(try Bounded.read(url.path, limit: 65536)))
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
