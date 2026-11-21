typeDiagram
type KernelState { identity: Int phase: Float integratorOne: Float integratorTwo: Float delay: List<Float> }
type NodeSample { identity: Int value: Float }
type SampleFrame { states: List<KernelState> values: List<NodeSample> }
type Meter { identity: Int peak: Float energy: Float count: Int }
type Rendered { states: List<KernelState> samples: List<Float> position: Int meters: List<Meter> }
type Timeline<T> { audio: Rendered observation: T }
union Stage { Ready {} Running {} Releasing { remaining: Int } Disposed {} }
union Transition { Prepare { rate: Float block: Int } Activate {} Reset {} ReleaseTail {} Dispose {} }
type Instance<P> { stage: Stage rate: Float block: Int plan: P audio: Rendered }
