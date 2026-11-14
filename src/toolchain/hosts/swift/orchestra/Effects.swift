import Foundation

// [ORCHESTRA-SOUND] Delay storage lives on the producer thread, outside the device callback.
private final class DelayLine {
    private var samples: [Double]
    private var cursor = 0
    init(_ count: Int) { samples = Array(repeating: 0, count: count) }
    func read() -> Double { samples[cursor] }
    func write(_ value: Double) {
        samples[cursor] = abs(value) < 1e-18 ? 0 : value
        cursor = (cursor + 1) % samples.count
    }
    func clear() { samples = Array(repeating: 0, count: samples.count); cursor = 0 }
}

// One independent rack per controller. Four parallel combs feed two all-pass diffusers.
final class OrchestraEffects {
    var peer: Int64?
    private let echo: DelayLine
    private let combs: [DelayLine]
    private let diffusers: [DelayLine]
    private var echoTarget = 0.0
    private var spaceTarget = 0.0
    private var echoMix = 0.0
    private var spaceMix = 0.0
    init(rate: Double) {
        echo = DelayLine(Int(rate * 0.31))
        combs = [0.0297, 0.0371, 0.0411, 0.0437].map { DelayLine(Int(rate * $0)) }
        diffusers = [0.005, 0.0017].map { DelayLine(Int(rate * $0)) }
    }
    func configure(_ control: VoiceControl) { configure(echo: control.echo, reverb: control.reverb) }
    func configure(echo: Double, reverb: Double) { echoTarget = echo; spaceTarget = reverb }
    func clear() {
        echo.clear()
        for line in combs + diffusers { line.clear() }
        echoTarget = 0; spaceTarget = 0
        echoMix = 0; spaceMix = 0
    }
    func next(_ input: Double) -> Double {
        echoMix += min(0.0002, max(-0.0002, echoTarget - echoMix))
        spaceMix += min(0.0002, max(-0.0002, spaceTarget - spaceMix))
        let delayed = echo.read()
        echo.write(input + delayed * 0.45)
        let echoed = input + delayed * echoMix * 0.5
        return echoed + reverberate(echoed) * spaceMix
    }
    private func reverberate(_ input: Double) -> Double {
        var room = 0.0
        for line in combs {
            let previous = line.read()
            line.write(input + previous * 0.72)
            room += previous * 0.07
        }
        for line in diffusers {
            let previous = line.read()
            let diffused = previous - room * 0.5
            line.write(room + diffused * 0.5)
            room = diffused
        }
        return room
    }
}
