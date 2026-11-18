typeDiagram
type GrainControls { start: Float rate: Float duration: Int amplitude: Float pan: Float }
type GrainVoice { identity: Int controls: GrainControls cursor: Float age: Int }
type GrainPool { source: List<Float> capacity: Int voices: List<GrainVoice> revision: Int position: Int nextIdentity: Int }
type GrainObservation { revision: Int samplePosition: Int activeGrains: Int capacity: Int }
type GrainSample { left: Float right: Float next: GrainPool }
type GrainContribution { left: Float right: Float next: GrainVoice }
type GrainBlock { left: List<Float> right: List<Float> next: GrainPool }
