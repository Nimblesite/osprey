import AVFoundation
import Foundation

// [SYNTH-RENDER] Osprey exports run only on the session thread. The device reads a preallocated ring.
final class OrchestraAudio {
    private let engine = AVAudioEngine()
    private var source: AVAudioSourceNode?
    private var voices: [VoiceState] = []
    private var buffer = [Float](repeating: 0, count: 128)
    private var capture = Data()
    var capturesAudio = false
    func writeCapture(to path: String) throws { try capture.write(to: URL(fileURLWithPath: path), options: .atomic) }
    private(set) var sampleRate = 48000.0
    private(set) var renderedFrames: UInt64 = 0
    private(set) var peak = 0.0
    private(set) var level = 0.0
    var activeNotes: Int { voices.filter { $0.target > 0 }.count }
    func setVolume(_ value: Float) { engine.mainMixerNode.outputVolume = value }
    private(set) var failure: String?

    func start() throws {
        guard orchestra_ring_lock_free() != 0 else { throw problem("Audio atomics are not lock-free") }
        let rate = engine.outputNode.inputFormat(forBus: 0).sampleRate
        guard rate >= 8000, rate <= 384000,
              let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1) else {
            throw problem("No supported audio output format")
        }
        sampleRate = rate
        let node = makeSource()
        source = node
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
        fill()
        try engine.start()
    }

    private func makeSource() -> AVAudioSourceNode {
        AVAudioSourceNode { _, _, count, list in
            let buffers = UnsafeMutableAudioBufferListPointer(list)
            guard buffers.count == 1 else { return kAudio_ParamError }
            for output in buffers {
                guard output.mDataByteSize >= count * 4, let raw = output.mData else { return kAudio_ParamError }
                orchestra_ring_read(raw.assumingMemoryBound(to: Float.self), Int(count))
            }
            return noErr
        }
    }

    func update() throws {
        guard let pointer = osprey_orchestra_voices() else { throw problem("Missing voice snapshot") }
        defer { osprey_release_string(pointer) }
        let data = Data(String(cString: pointer).utf8)
        guard let rows = try JSONSerialization.jsonObject(with: data) as? [[String: Any]], rows.count <= 80 else {
            throw problem("Invalid voice snapshot")
        }
        let controls = try rows.map(decodeControl)
        guard Set(controls.map(\.id)).count == controls.count else { throw problem("Duplicate voice identity") }
        reconcile(controls)
    }

    private func decodeControl(_ row: [String: Any]) throws -> VoiceControl {
        guard let id = row["id"] as? String, let frequency = row["frequency"] as? Double,
              let gain = row["gain"] as? Double, frequency.isFinite, gain.isFinite,
              frequency > 0, frequency < sampleRate / 2, gain >= 0, gain <= 0.1 else {
            throw problem("Invalid voice values")
        }
        return VoiceControl(id: id, frequency: frequency, gain: gain)
    }

    private func reconcile(_ controls: [VoiceControl]) {
        let old = Dictionary(uniqueKeysWithValues: voices.map { ($0.id, $0) })
        voices = controls.map { control in
            VoiceState(id: control.id, frequency: control.frequency, target: control.gain,
                       gain: old[control.id]?.gain ?? 0, phase: old[control.id]?.phase ?? 0)
        }
        let live = Set(controls.map(\.id))
        // Bound release tails to the unused part of the eighty-voice pool.
        voices += old.values.filter { !live.contains($0.id) && $0.gain > 0 }
            .sorted { $0.gain > $1.gain }.prefix(80 - voices.count).map {
            VoiceState(id: $0.id, frequency: $0.frequency, target: 0, gain: $0.gain, phase: $0.phase)
        }
    }

    // Bounded linear smoothing avoids note-on/off discontinuities; voice phases survive snapshots.
    func render(_ count: Int) -> [Float] {
        var output = [Float](repeating: 0, count: count)
        level = 0
        for frame in 0..<count {
            let sample = nextSample()
            if !sample.isFinite || abs(sample) > 0.800001 {
                failure = "Audio sample exceeded the bounded ensemble range"
                voices.removeAll()
                return output
            }
            level = max(level, abs(sample))
            peak = max(peak, abs(sample))
            output[frame] = Float(sample)
        }
        finishBlock(output)
        return output
    }

    private func nextSample() -> Double {
        var sample = 0.0
        var totalGain = 0.0
        for index in voices.indices {
            let voice = voices[index]
            let gain = voice.gain + min(0.0002, max(-0.0002, voice.target - voice.gain))
            sample += osprey_orchestra_sine(voice.phase) * gain
            totalGain += gain
            let phase = voice.phase + voice.frequency / sampleRate
            voices[index] = VoiceState(id: voice.id, frequency: voice.frequency, target: voice.target,
                                       gain: gain, phase: phase - floor(phase))
        }
        // Normalise by the gain sum, including release tails; the ensemble stays below 0.8.
        return sample / max(1, totalGain / 0.8)
    }

    private func finishBlock(_ output: [Float]) {
        voices.removeAll { $0.target == 0 && $0.gain == 0 }
        renderedFrames += UInt64(output.count)
        if capturesAudio {
            if capture.count + output.count * 4 <= Int(sampleRate * 30) * 4 {
                output.withUnsafeBytes { capture.append(contentsOf: $0) }
            } else { failure = "Diagnostic recording exceeds thirty seconds" }
        }
    }

    func fill() {
        while orchestra_ring_queued() < 1024 {
            buffer = render(128)
            let written = buffer.withUnsafeBufferPointer { orchestra_ring_write($0.baseAddress, $0.count) }
            if written != buffer.count { failure = "Audio ring rejected a prepared block"; return }
        }
    }

    func stop() { engine.stop() }
}

func problem(_ message: String) -> NSError {
    NSError(domain: "org.ospreylang.orchestra", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
}
