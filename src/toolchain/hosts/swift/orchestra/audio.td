typeDiagram
type VoiceControl { id: String peer: Int instrument: Int frequency: Float gain: Float tone: Float echo: Float reverb: Float warp: Float }
type VoiceState { control: VoiceControl bank: Int target: Float gain: Float phase: Float tone: Float warp: Float }
