typeDiagram
union Waveform { Sine {} Saw {} Pulse {} Triangle {} }
type OscillatorConfig { sampleRate: Float frequency: Float pulseWidth: Float waveform: Waveform }
type OscillatorState { phase: Float }
type OscillatorSample { sample: Float next: OscillatorState }
type EnvelopeConfig { sampleRate: Float attack: Float decay: Float sustain: Float release: Float }
union EnvelopeStage { Idle {} Attack {} Decay {} Sustain {} Release {} }
type EnvelopeState { stage: EnvelopeStage level: Float releaseStep: Float }
type FilterConfig { sampleRate: Float cutoff: Float resonance: Float }
type FilterCoefficients { a1: Float a2: Float a3: Float k: Float }
type FilterState { integratorOne: Float integratorTwo: Float }
type FilterSample { low: Float band: Float high: Float next: FilterState }
