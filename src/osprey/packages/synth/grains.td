typeDiagram
union GrainWindow { HannWindow AsrWindow { attack: Float release: Float } }
type GrainControls { window: GrainWindow start: Float rate: Float duration: Int amplitude: Float pan: Float }
type GrainVoice { identity: Int controls: GrainControls cursor: Float age: Int }
type GrainPool { source: List<Float> capacity: Int voices: List<GrainVoice> revision: Int position: Int nextIdentity: Int }
type GrainAdmission { pool: GrainPool accepted: Bool }
type GrainObservation { revision: Int samplePosition: Int activeGrains: Int capacity: Int requestedGrains: Int acceptedGrains: Int rejectedGrains: Int stolenGrains: Int requestedPerFrame: Float acceptedPerFrame: Float }
type GrainSample { left: Float right: Float next: GrainPool }
type GrainContribution { left: Float right: Float next: GrainVoice }
type GrainBlock { left: List<Float> right: List<Float> next: GrainPool }
type GrainEvent { position: Int controls: GrainControls }
type GrainTimelineState { audio: GrainBlock events: List<GrainEvent> }
union GrainTiming { PeriodicTiming { period: Int jitter: Int } ExponentialTiming { mean: Float maximum: Int } }
type GrainClock { position: Int nextOnset: Int timing: GrainTiming seed: Int }
type GrainOnsets { positions: List<Int> next: GrainClock }
type GrainCloudState { pool: GrainPool clock: GrainClock }
type GrainCloudBlock { left: List<Float> right: List<Float> next: GrainCloudState }
type GrainCloudObservation { playback: GrainObservation timing: GrainTiming nextOnset: Int }
type GrainNodeControls { playback: GrainControls timing: GrainTiming }
type MidiVoice<K> { kernel: K pitch: Int channel: Int noteIdentity: Int velocity: Float active: Bool }
union ProcessorMemory<K> { ChannelMemory { channels: List<K> } MidiMemory { voice: MidiVoice<K> } GrainMemory { cloud: GrainCloudState } }
