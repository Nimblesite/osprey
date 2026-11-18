typeDiagram
union GrainWindow { HannWindow AsrWindow { attack: Float release: Float } }
type GrainControls { window: GrainWindow start: Float rate: Float duration: Int amplitude: Float pan: Float }
type GrainVoice { identity: Int controls: GrainControls cursor: Float age: Int }
type GrainPool { source: List<Float> capacity: Int voices: List<GrainVoice> revision: Int position: Int nextIdentity: Int }
type GrainObservation { revision: Int samplePosition: Int activeGrains: Int capacity: Int }
type GrainSample { left: Float right: Float next: GrainPool }
type GrainContribution { left: Float right: Float next: GrainVoice }
type GrainBlock { left: List<Float> right: List<Float> next: GrainPool }
type GrainEvent { position: Int controls: GrainControls }
type GrainTimelineState { audio: GrainBlock events: List<GrainEvent> }
type GrainClock { position: Int nextOnset: Int period: Int jitter: Int seed: Int }
type GrainOnsets { positions: List<Int> next: GrainClock }
type GrainCloudState { pool: GrainPool clock: GrainClock }
type GrainCloudBlock { left: List<Float> right: List<Float> next: GrainCloudState }
type GrainCloudObservation { playback: GrainObservation period: Int jitter: Int nextOnset: Int }
type GrainNodeControls { playback: GrainControls period: Int jitter: Int }
type MidiVoice<K> { kernel: K pitch: Int channel: Int noteIdentity: Int velocity: Float active: Bool }
union ProcessorMemory<K> { ChannelMemory { channels: List<K> } MidiMemory { voice: MidiVoice<K> } GrainMemory { cloud: GrainCloudState } }
