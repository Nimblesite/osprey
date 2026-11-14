import AVFoundation
import Foundation

// [SYNTH-RENDER] Osprey exports run only on the session thread. The device reads a preallocated ring.
final class OrchestraAudio {
    private let engine = AVAudioEngine()
    private var configurationObserver: NSObjectProtocol?
    var isRunning: Bool { engine.isRunning }
    private var source: AVAudioSourceNode?
    private var voices: [VoiceState] = []
    private var racks = (0..<8).map { _ in OrchestraEffects(rate: 48000) }
    private var sums = [Double](repeating: 0, count: 8)
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
        racks = (0..<8).map { _ in OrchestraEffects(rate: rate) }
        let node = makeSource()
        source = node
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
        fill()
        try engine.start()
        configurationObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { _ in
            performSession {
                log("audio.configuration_changed", ["running": self.engine.isRunning, "sampleRate": self.engine.outputNode.inputFormat(forBus: 0).sampleRate])
                self.recoverOutput()
            }
        }
        log("audio.started", ["sampleRate": sampleRate])
    }

    // Configuration changes stop AVAudioEngine even while the phone session stays connected.
    func recoverOutput() {
        guard source != nil, !engine.isRunning else { return }
        do {
            let format = engine.outputNode.inputFormat(forBus: 0)
            guard format.sampleRate >= 8000, format.sampleRate <= 384000, format.channelCount > 0 else {
                throw problem("Audio output is unavailable")
            }
            fill()
            try engine.start()
            log("audio.restarted", ["deviceSampleRate": format.sampleRate, "sourceSampleRate": sampleRate])
        } catch { log("audio.recovery_failed", ["reason": error.localizedDescription]) }
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
        guard let id = row["id"] as? String, let peer = row["peer"] as? Int64,
              let instrument = row["instrument"] as? Int64, (0...3).contains(instrument),
              let frequency = row["frequency"] as? Double, let gain = row["gain"] as? Double,
              frequency.isFinite, gain.isFinite, frequency > 0, frequency < sampleRate / 2,
              gain >= 0, gain <= 0.1 else { throw problem("Invalid voice values") }
        let amounts = try ["tone", "echo", "reverb", "warp"].map { key -> Double in
            guard let value = row[key] as? Double, value.isFinite, (0...1).contains(value) else {
                throw problem("Invalid sound parameter")
            }
            return value
        }
        return VoiceControl(id: id, peer: peer, instrument: instrument, frequency: frequency, gain: gain,
                            tone: amounts[0], echo: amounts[1], reverb: amounts[2], warp: amounts[3])
    }

    private func allocateRacks(_ controls: [VoiceControl]) {
        let peers = Set(controls.map(\.peer))
        for peer in peers where !racks.contains(where: { $0.peer == peer }) {
            guard let bank = racks.firstIndex(where: { $0.peer == nil || !peers.contains($0.peer ?? -1) }) else { continue }
            voices.removeAll { $0.bank == bank }
            racks[bank].clear()
            racks[bank].peer = peer
        }
    }

    private func reconcile(_ controls: [VoiceControl]) {
        allocateRacks(controls)
        let old = Dictionary(uniqueKeysWithValues: voices.map { ($0.control.id, $0) })
        voices = controls.compactMap { control -> VoiceState? in
            guard let bank = racks.firstIndex(where: { $0.peer == control.peer }) else { return nil }
            racks[bank].configure(control)
            return VoiceState(control: control, bank: Int64(bank), target: control.gain,
                              gain: old[control.id]?.gain ?? 0, phase: old[control.id]?.phase ?? 0,
                              tone: old[control.id]?.tone ?? control.tone, warp: old[control.id]?.warp ?? control.warp)
        }
        let live = Set(controls.map(\.id))
        // Bound release tails to the unused part of the eighty-voice pool.
        voices += old.values.filter { !live.contains($0.control.id) && $0.gain > 0 }
            .sorted { $0.gain > $1.gain }.prefix(80 - voices.count).map {
            VoiceState(control: $0.control, bank: $0.bank, target: 0, gain: $0.gain, phase: $0.phase, tone: $0.tone, warp: $0.warp)
        }
    }

    func clearEffects() { for rack in racks { rack.clear() } }
    func clearEffects(peer: Int64) { for rack in racks where rack.peer == peer { rack.clear() } }
    func updateEffects(peer: Int64, echo: Double, reverb: Double) {
        for rack in racks where rack.peer == peer { rack.configure(echo: echo, reverb: reverb) }
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
        for bank in sums.indices { sums[bank] = 0 }
        for index in voices.indices {
            let voice = voices[index], control = voice.control
            let gain = voice.gain + min(0.0002, max(-0.0002, voice.target - voice.gain))
            let tone = voice.tone + min(0.0002, max(-0.0002, control.tone - voice.tone))
            let warp = voice.warp + min(0.0002, max(-0.0002, control.warp - voice.warp))
            let step = control.frequency / sampleRate
            sums[Int(voice.bank)] += osprey_orchestra_wave(control.instrument, voice.phase, step, tone, warp) * gain
            let phase = voice.phase + step
            voices[index] = VoiceState(control: control, bank: voice.bank, target: voice.target,
                                       gain: gain, phase: phase - floor(phase), tone: tone, warp: warp)
        }
        var sample = 0.0
        for bank in racks.indices { sample += racks[bank].next(sums[bank]) }
        // A smooth saturator bounds dry voices and overlapping feedback tails together.
        return sample / (1 + abs(sample) / 0.8)
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

    func stop() {
        if let configurationObserver { NotificationCenter.default.removeObserver(configurationObserver) }
        engine.stop()
        log("audio.stopped", ["frames": renderedFrames])
    }
}

func problem(_ message: String) -> NSError {
    NSError(domain: "org.ospreylang.orchestra", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
}
