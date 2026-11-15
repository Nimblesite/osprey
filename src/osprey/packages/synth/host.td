typeDiagram
type HostNote { kind: Int offset: Int pitch: Int channel: Int identity: Int velocity: Float }
type HostParameter { identity: Int offset: Int value: Float }
type HostBlock { frames: Int notes: List<HostNote> parameters: List<HostParameter> samples: List<Float> }
type HostLayout { inputs: Int outputs: Int acceptsNotes: Bool }
