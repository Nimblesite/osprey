typeDiagram
type HostNote { kind: Int offset: Int pitch: Int channel: Int identity: Int velocity: Float }
type HostParameter { identity: Int offset: Int value: Float }
type HostBlock { frames: Int notes: List<HostNote> parameters: List<HostParameter> samples: List<Float> }
type HostLayout { inputChannels: Int outputChannels: Int acceptsNotes: Bool }
type HostControl { identity: Int name: String initial: Float automatable: Bool }
type HostDescriptor { latency: Int identifier: String name: String inputChannels: Int outputChannels: Int notes: Bool controls: List<HostControl> }
type HostRendered { renderChannels: Int renderFrames: Int renderLatency: Int renderSamples: List<Float> }
type NodeClip { clipNode: Int clipChannels: Int clipSamples: List<Float> clipNotes: List<HostNote> }
union ClipAction { ClipReady { clips: List<NodeClip> } ClipWaiting { node: Int request: HostBlock clips: List<NodeClip> } }
type ClipKernel<K> { heldKernel: K kernelSamples: List<Float> }
type NodeKernels<K> { kernelNode: Int kernelMemory: K }
type ParameterStamp { stampKey: String stampValue: Float }
type HostedStamp { stampNode: Int stampControls: List<ParameterStamp> }
type GraphState<K> { graphPosition: Int graphHosted: List<HostedStamp> graphKernels: List<NodeKernels<K>> }
type GraphPending<G,K> { pendingGraph: G pendingState: GraphState<K> pendingClips: List<NodeClip> pendingFrames: Int pendingNode: Int }
union BlockAction<G,K> { BlockReady { held: GraphState<K> clips: List<NodeClip> } BlockWaiting { request: HostBlock pending: GraphPending<G,K> } }
