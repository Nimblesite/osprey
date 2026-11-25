typeDiagram
union Rate { Audio {} Control {} Events {} }
union Unit { Amplitude {} Hertz {} Seconds {} Normalised {} Ratio {} Notes {} }
type Port { name: String rate: Rate channels: Int unit: Unit }
type Parameter { key: String label: String unit: Unit minimum: Float maximum: Float initial: Float automatable: Bool }
type TableLevel { harmonics: Int samples: List<Float> }
type TableFrame { levels: List<TableLevel> }
type Wavetable { size: Int harmonics: Int frames: List<TableFrame> }
type SampleResource { name: String sampleRate: Int sourceChannels: Int rootFrequency: Float samples: List<Float> }
union Processor { SamplePlayer { resource: SampleResource } Granular { resource: SampleResource capacity: Int seed: Int } Exciter { seed: Int } Resonator {} Modal {} Waveguide { capacity: Int } FractionalDelay { capacity: Int } LossFilter {} Allpass {} TableOscillator { table: Wavetable } Additive {} Spectral { size: Int hop: Int } Shaper {} Reverb {} Shifter {} PhaseOperator {} Bowed { capacity: Int } Smoother {} StrikeComb { capacity: Int } Downmix {} Lfo {} ModulatedGain {} MidiNotes {} FormulaNotes { formula: String } Stereo {} Panner {} Split {} Join {} Route { channelMap: List<Int> } Oscillator {} Filter {} Amplifier {} Mixer {} Sink {} Delay { samples: Int } External { identifier: String latency: Int } }
type Node { identity: Int processor: Processor inputs: List<Port> outputs: List<Port> parameters: List<Parameter> }
type Cable { source: Int outlet: Int destination: Int inlet: Int }
type Graph { nodes: List<Node> cables: List<Cable> }
type Prepared { graph: Graph order: List<Int> feedback: List<Cable> }
type RankedNode { component: Node ordinal: Int }
type RouteIndex<Inlets, Fanout, Counts> { occupiedInlets: Inlets destinations: Fanout dependencies: Counts delayedInputs: List<Cable> }
union ReadyNodes { NoReadyNode {} ReadyNode { ordinal: Int depth: Int left: ReadyNodes right: ReadyNodes } }
type ReadyQueue<Counts> { readyNodes: ReadyNodes remainingInputs: Counts }
union GraphFault { DuplicateNode {} MissingNode {} InvalidNode {} InvalidPort {} IncompatiblePorts {} OccupiedInput {} DuplicateParameter {} InvalidParameter {} Cycle {} MissingParameter {} StaleRevision {} InvalidFrame {} }
union Checked<T> { Valid { value: T } Invalid { fault: GraphFault } }
type ParameterValue { node: Int key: String value: Float }
type ComponentState { node: Int activeVoices: Int peak: Float rms: Float }
type Observation { revision: Int samplePosition: Int parameters: List<ParameterValue> components: List<ComponentState> }
type ParameterChange { node: Int key: String value: Float revision: Int samplePosition: Int }
type SnapshotSlot { latest: Observation published: Int delivered: Int }
type SlotRead { observation: Observation coalesced: Int next: SnapshotSlot }
union CommandStatus { Pending { edit: ParameterChange } Applied { revision: Int } Rejected { fault: GraphFault } }
type CommandReceipt { status: CommandStatus snapshot: Observation }
type MacroTarget { node: Int key: String minimum: Float maximum: Float exponent: Float }
type Macro { name: String targets: List<MacroTarget> }
type SavedNode { identity: Int kind: String version: Int settings: List<Int> resource: String values: List<ParameterValue> inputs: Int }
type PatchDocument { format: Int nodes: List<SavedNode> cables: List<Cable> macros: List<Macro> }
type NamedResource { name: String sample: SampleResource }
type NamedTable { name: String table: Wavetable }
type PatchResources { samples: List<NamedResource> tables: List<NamedTable> }
type ResourceIdentity { name: String hash: Int channels: Int sampleRate: Int frames: Int version: Int }
type PathLatency { latencyNode: Int arrival: Int }
type Alignment { alignedGraph: Graph arrivals: List<PathLatency> lastDelay: Int reservedSamples: Int }
