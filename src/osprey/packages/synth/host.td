typeDiagram
type HostNote { kind: Int offset: Int pitch: Int channel: Int identity: Int velocity: Float }
type HostParameter { identity: Int offset: Int value: Float }
type HostBlock { frames: Int notes: List<HostNote> parameters: List<HostParameter> samples: List<Float> }
type HostLayout { inputChannels: Int outputChannels: Int acceptsNotes: Bool }
type HostControl { identity: Int name: String initial: Float automatable: Bool }
type HostDescriptor { identifier: String name: String inputChannels: Int outputChannels: Int notes: Bool controls: List<HostControl> }
type HostRendered { renderChannels: Int renderFrames: Int renderLatency: Int renderSamples: List<Float> }
