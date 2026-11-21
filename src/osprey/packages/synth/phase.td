typeDiagram
type PulseControls { index: Float ratio: Float offset: Float }
union PhaseShape { PlainPhase {} DistortedPhase { breakpoint: Float } WindowedPhase { index: Float ratio: Float } WavePulsePhase { breakpoint: Float first: PulseControls second: PulseControls } }
type OperatorConfig { sampleRate: Float frequency: Float shape: PhaseShape depth: Float spread: Float feedback: Float amplitude: Float }
type OperatorState { phase: Float previous: Float older: Float sync: Float }
type OperatorSample { sample: Float next: OperatorState }
type OversampledOperator { operator: OperatorState down: List<Float> }
type OversampledOperatorSample { sample: Float next: OversampledOperator }
type OperatorControls { selector: Float breakpoint: Float first: PulseControls second: PulseControls frequency: Float depth: Float spread: Float feedback: Float amplitude: Float oversample: Float }
union OperatorSchedule { LiveOperator {} FixedOperator { config: OperatorConfig oversample: Float } }
