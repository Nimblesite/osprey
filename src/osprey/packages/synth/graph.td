typeDiagram
union Rate { Audio {} Control {} Events {} }
union Unit { Amplitude {} Hertz {} Seconds {} Normalised {} Ratio {} Notes {} }
type Port { name: String rate: Rate channels: Int unit: Unit }
type Parameter { key: String label: String unit: Unit minimum: Float maximum: Float initial: Float automatable: Bool }
union Processor { Oscillator {} Filter {} Amplifier {} Mixer {} Sink {} Delay { samples: Int } External { identifier: String } }
type Node { identity: Int processor: Processor inputs: List<Port> outputs: List<Port> parameters: List<Parameter> }
type Cable { source: Int outlet: Int destination: Int inlet: Int }
type Graph { nodes: List<Node> cables: List<Cable> }
type Prepared { graph: Graph order: List<Int> feedback: List<Cable> }
union GraphFault { DuplicateNode {} MissingNode {} InvalidNode {} InvalidPort {} IncompatiblePorts {} OccupiedInput {} DuplicateParameter {} InvalidParameter {} Cycle {} Capacity {} MissingParameter {} StaleRevision {} InvalidFrame {} }
union Checked<T> { Valid { value: T } Invalid { fault: GraphFault } }
type ParameterValue { node: Int key: String value: Float }
type ComponentState { node: Int activeVoices: Int peak: Float rms: Float }
type Observation { revision: Int samplePosition: Int parameters: List<ParameterValue> components: List<ComponentState> }
type ParameterChange { node: Int key: String value: Float revision: Int samplePosition: Int }
