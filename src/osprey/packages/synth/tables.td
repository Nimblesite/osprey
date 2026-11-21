typeDiagram
type TableSpectrum { amplitudes: List<Float> phases: List<Float> }
type TableHarmonic { number: Int amplitude: Float phase: Float }
type WavetableConfig { sampleRate: Float frequency: Float position: Float amplitude: Float phase: Float }
type WavetableState { phase: Float }
type WavetableSample { sample: Float next: WavetableState }
union PartialLayout { HarmonicLayout {} StiffLayout { inharmonicity: Float stretch: Float } RatioLayout { ratios: List<Float> } }
type PartialConfig { sampleRate: Float frequency: Float count: Float tilt: Float parity: Float decay: Float damping: Float amplitude: Float layout: PartialLayout }
type PartialState { phases: List<Float> age: Float }
type PartialSample { sample: Float next: PartialState }
type PartialVoice { sample: Float gain: Float phases: List<Float> }
type PartialCoefficient { increment: Float weight: Float }
type PartialSettings { config: PartialConfig interval: Float }
union PartialSchedule { DynamicPartials {} FixedPartials { settings: PartialSettings coefficients: List<PartialCoefficient> } }
