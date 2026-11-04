import AVKit
import Combine
import SwiftUI

struct FilmView: View {
    let source: String
    let poster: String
    let captions: String
    let alt: String
    let seconds: Int
    @State private var player: AVPlayer?
    @State private var cues: [FilmCue] = []
    @State private var failure = ""

    var body: some View {
        VStack {
            if let player { FilmPlayer(player: player, cues: cues, onFailure: playbackFailed) }
            else { posterButton }
            if !failure.isEmpty { Text(failure).foregroundStyle(.red).accessibilityLabel(failure) }
        }.onDisappear { player?.pause() }
    }

    private var posterButton: some View {
        Button { start() } label: {
            ZStack(alignment: .bottomTrailing) {
                StillView(source: poster, alt: alt, width: 1920, height: 1080)
                Label(String(format: "%d:%02d", seconds / 60, seconds % 60), systemImage: "play.fill").padding(8).background(.ultraThinMaterial, in: Capsule()).padding(10)
            }
        }.buttonStyle(.plain).accessibilityLabel("Play: \(alt)")
    }

    private func playbackFailed(_ message: String) {
        player?.pause()
        player = nil
        failure = "The film could not be opened: " + message
    }

    private func start() {
        do {
            guard let url = Bundled.url(source), FileManager.default.fileExists(atPath: url.path) else { throw CaptionFailure.invalid("The film is missing") }
            cues = try FilmCaptions.load(captions)
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
        }.aspectRatio(16.0 / 9.0, contentMode: .fit)
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
        }.allowsHitTesting(false)
    }
}
