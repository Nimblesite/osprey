typeDiagram
union Excitation { ImpulseExcitation {} NoiseExcitation { colour: Float } }
type ExciterConfig { strength: Float duration: Int interval: Int excitation: Excitation }
type ExciterState { age: Float seed: Float smoothed: Float }
type ExciterSample { sample: Float next: ExciterState }
union ResonatorScale { PeakScale {} ImpulseScale {} }
type ResonatorConfig { sampleRate: Float frequency: Float decay: Float gain: Float scale: ResonatorScale }
type ResonatorCoefficients { input: Float cosine: Float sine: Float }
type ResonatorState { real: Float imag: Float }
type ResonatorSample { sample: Float next: ResonatorState }
union ModeSet { HarmonicModes {} BarModes {} MembraneModes {} BellModes {} }
type ModalConfig { sampleRate: Float frequency: Float decay: Float brightness: Float strike: Float count: Int modes: ModeSet }
type ModalBank { modes: List<ResonatorCoefficients> }
type ModalState { modes: List<ResonatorState> }
type ModalSample { sample: Float next: ModalState }
type DelayLine { samples: List<Float> }
type LossConfig { gain: Float brightness: Float }
type LossState { previous: Float }
type LossSample { sample: Float next: LossState }
type AllpassState { input: Float output: Float }
type AllpassSample { sample: Float next: AllpassState }
type WaveguideConfig { sampleRate: Float frequency: Float decay: Float brightness: Float dispersion: Float pickup: Float capacity: Int }
type WaveguideState { line: DelayLine loss: LossState dispersion: AllpassState }
type WaveguideSample { sample: Float next: WaveguideState }
union ShaperCurve { TanhCurve {} HardCurve {} CubicCurve {} FoldCurve {} }
type ShaperConfig { sampleRate: Float curve: ShaperCurve drive: Float bias: Float level: Float antialias: Bool }
type ShaperState { previous: Float blockerInput: Float blockerOutput: Float }
type ShaperSample { sample: Float next: ShaperState }
type OversampledState { shaper: ShaperState up: List<Float> down: List<Float> }
type OversampledSample { sample: Float next: OversampledState }
type ReverbConfig { sampleRate: Float size: Float decay: Float damping: Float diffusion: Float mix: Float freeze: Bool }
type ReverbState { lines: List<DelayLine> damped: List<Float> diffusers: List<DelayLine> }
type DiffuserSample { sample: Float lines: List<DelayLine> }
type ReverbSample { sample: Float next: ReverbState }
type ShifterConfig { sampleRate: Float shift: Float ring: Bool mix: Float }
type HilbertSection { input: Float older: Float output: Float oldest: Float }
type ShifterState { first: List<HilbertSection> second: List<HilbertSection> delayed: Float phase: Float }
type ShifterSample { sample: Float next: ShifterState }
type StringVoice { note: Int velocity: Float config: WaveguideConfig guide: WaveguideState exciter: ExciterState active: Bool releasing: Bool age: Int released: Int }
type StringPool { voices: List<StringVoice> excitation: ExciterConfig release: Float stolen: Int }
type StringPoolSample { sample: Float next: StringPool }
type SmootherConfig { sampleRate: Float time: Float linear: Bool }
type SmootherState { value: Float }
type BowConfig { sampleRate: Float frequency: Float decay: Float brightness: Float position: Float pressure: Float velocity: Float capacity: Int }
type BowState { neck: DelayLine bridge: DelayLine loss: LossState }
type BowSample { sample: Float next: BowState }
type SmootherSample { sample: Float next: SmootherState }
type MeshConfig { sampleRate: Float width: Int height: Int courant: Float decay: Float exciteX: Int exciteY: Int pickX: Int pickY: Int }
type MeshState { current: List<List<Float>> previous: List<List<Float>> }
type MeshSample { sample: Float next: MeshState }
type StrikeConfig { sampleRate: Float frequency: Float position: Float capacity: Int }
type LineSample { sample: Float next: DelayLine }
type GuideCoefficients { tuning: Float pickup: Float attenuation: LossConfig dispersion: Float }
union PhysicalSchedule { LivePhysical {} FixedExciter { config: ExciterConfig } FixedModal { config: ModalConfig } FixedGuide { coefficients: GuideCoefficients } FixedShaper { config: ShaperConfig weights: List<Float> } }
