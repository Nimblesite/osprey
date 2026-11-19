typeDiagram
type WaveSamples { sampleRate: Int channels: Int frames: Int samples: List<Float> }
type WaveChunks { formatStart: Int formatBytes: Int dataStart: Int dataBytes: Int }
type WaveFormat { encoding: Int channels: Int sampleRate: Int sampleBytes: Int blockAlign: Int dataStart: Int frames: Int }
type WaveField { offset: Int width: Int }
