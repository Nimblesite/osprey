typeDiagram
union GrainWindow { HannWindow AsrWindow { attack: Float release: Float } }
type GrainControls { window: GrainWindow start: Float rate: Float duration: Int amplitude: Float pan: Float }
union GrainBoundary { LoopSource {} OneShotSource {} }
type GrainRegion { first: Int limit: Int boundary: GrainBoundary }
type GrainVoice { region: GrainRegion identity: Int controls: GrainControls cursor: Float age: Int }
type GrainPool { region: GrainRegion source: List<Float> retiredSource: List<Float> retiredBefore: Int capacity: Int voices: List<GrainVoice> revision: Int position: Int nextIdentity: Int }
type GrainAdmission { pool: GrainPool accepted: Bool }
type GrainVoiceObservation { identity: Int cursor: Float age: Int duration: Int sourceFrames: Int fromPreviousSource: Bool region: GrainRegion }
type GrainObservation { sourceFrames: Int retiredFrames: Int region: GrainRegion voices: List<GrainVoiceObservation> revision: Int samplePosition: Int activeGrains: Int capacity: Int requestedGrains: Int acceptedGrains: Int rejectedGrains: Int stolenGrains: Int requestedPerFrame: Float acceptedPerFrame: Float }
type GrainSample { left: Float right: Float next: GrainPool }
type GrainContribution { left: Float right: Float next: GrainVoice }
type GrainBlock { left: List<Float> right: List<Float> next: GrainPool }
type GrainSeeds { position: Int rate: Int duration: Int amplitude: Int pan: Int }
type GrainSpread { position: Float rate: Float duration: Int amplitude: Float pan: Float }
type GrainDistribution { minimum: GrainControls maximum: GrainControls }
type GrainDistributedEvents { events: List<GrainEvent> next: GrainSeeds }
type GrainEvent { position: Int controls: GrainControls }
type GrainTimelineState { audio: GrainBlock events: List<GrainEvent> }
union GrainTiming { PeriodicTiming { period: Int jitter: Int } ExponentialTiming { mean: Float maximum: Int } }
type GrainClock { position: Int nextOnset: Int timing: GrainTiming seed: Int }
type GrainOnsets { positions: List<Int> next: GrainClock }
type GrainCloudState { sourceSamples: List<Float> pool: GrainPool clock: GrainClock seeds: GrainSeeds }
type GrainCloudBlock { left: List<Float> right: List<Float> next: GrainCloudState }
type GrainCloudObservation { playback: GrainObservation timing: GrainTiming nextOnset: Int }
type GrainNodeControls { playback: GrainControls timing: GrainTiming }
type MidiVoice<K> { kernel: K pitch: Int channel: Int noteIdentity: Int velocity: Float active: Bool releaseRemaining: Int releaseLength: Int }
union ProcessorMemory<K> { ChannelMemory { channels: List<K> } MidiMemory { voice: MidiVoice<K> } GrainMemory { cloud: GrainCloudState } }
type GrainCaptureState { capacity: Int sampleRate: Int written: Int samples: List<Float> frozen: Bool }
type PitchMarkClock { position: Int belowZero: Bool }
type PitchMarkBlock { positions: List<Int> next: PitchMarkClock }
