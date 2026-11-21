typeDiagram
type Complex { re: Float im: Float }
type ComplexSplit { evens: List<Complex> odds: List<Complex> }
type ComplexHalves { lower: List<Complex> upper: List<Complex> }
union MorphMode { Blend {} Envelope {} Partial {} }
union SpectralTransform { SpectralPass {} SpectralFreeze { held: Bool } SpectralBand { low: Float high: Float } SpectralShift { bins: Int } SpectralSmear { amount: Float } SpectralMorph { amount: Float mode: MorphMode } SpectralPitch { ratio: Float formant: Float } SpectralSeries { steps: List<SpectralTransform> } }
type SpectralConfig { sampleRate: Float size: Int hop: Int transform: SpectralTransform mix: Float }
type SpectralState { input: List<Float> side: List<Float> output: List<Float> memory: List<Complex> motion: List<Complex> countdown: Int frames: Int captured: Bool }
type SpectralSample { sample: Float next: SpectralState }
type SpectralFrame { bins: List<Complex> memory: List<Complex> motion: List<Complex> captured: Bool }
type SpectralBin { index: Int frequency: Float magnitude: Float re: Float im: Float }
type SpectralView { channel: Int position: Int size: Int bins: List<SpectralBin> }
type PeakScan { latest: Int owners: List<Int> }
type SpectrumBand { centre: Float level: Float peak: Float }
union SpectrumSchedule { LiveSpectrum {} FixedSpectrum { config: SpectralConfig } }
