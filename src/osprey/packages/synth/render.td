typeDiagram
type KernelState { identity: Int phase: Float integratorOne: Float integratorTwo: Float delay: List<Float> }
type NodeSample { identity: Int value: Float }
type SampleFrame { states: List<KernelState> values: List<NodeSample> }
type Meter { identity: Int peak: Float energy: Float count: Int }
type Rendered { states: List<KernelState> samples: List<Float> position: Int meters: List<Meter> }
type Timeline<T> { audio: Rendered observation: T }
