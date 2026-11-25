typeDiagram
# Device contract [DEVICE-PROFILE] [DEVICE-LINK] [PRISM-INSTRUMENT-DEVICE].
# Bind PatchDocument to the synth package's PatchDocument (graph.td) and
# MidiBinding to the DAW's MIDI learn binding. Millimetres are panel
# coordinates from the top-left corner of the top face.
union ControlKind { Knob {} TwoWay {} ThreeWay {} Footswitch {} }
type PanelPoint { x: Float y: Float }
type PhysicalControl { key: String kind: ControlKind position: PanelPoint labelWidth: Float }
union AudioPortKind { LineOut {} Headphones {} LineOrInstrumentIn {} }
type AudioPort { key: String kind: AudioPortKind channels: List<String> }
union MidiPortKind { UsbDualRole {} TrsIn {} TrsOut {} }
type ProcessorCost { kind: String nanosecondsPerBlock: Int }
type Profile { key: String name: String panelWidth: Float panelHeight: Float controls: List<PhysicalControl> audio: List<AudioPort> midi: List<MidiPortKind> sampleRates: List<Int> blockFrames: Int slots: Int maxVoices: Int costs: List<ProcessorCost> budgetShare: Float }
union FootAction { Toggle {} Hold {} }
union Assignment { KnobMacro { macro: String } SwitchMacro { macro: String } FootMacro { macro: String action: FootAction } Bypass {} NextSlot {} }
type FaceAssignment { control: String assignment: Assignment }
union PickupMode { Pickup {} Jump {} }
type DeviceFace { profile: String assignments: List<FaceAssignment> pickup: PickupMode }
type SlotPatch<PatchDocument, MidiBinding> { name: String patch: PatchDocument face: DeviceFace midi: List<MidiBinding> }
type SlotSummary { slot: Int name: String hash: String }
union Role { Source {} Effect {} SourceAndEffect {} }
union Refusal { UnknownNode { node: Int kind: String } HostedPlugin { node: Int } MissingFile { node: Int file: String } OverBudget { node: Int nanoseconds: Int } NoRole {} KindMismatch { control: String macro: String } InvalidGraph { detail: String } BadSignature {} FrameTooLarge { bytes: Int } }
union Fault { NonFinite {} MissedDeadline { count: Int } Throttled {} PatchFailed { detail: String } }
type Meter { output: String peak: Float rms: Float }
union DeviceMessage { Hello { serial: String firmware: String engine: Int profile: Profile active: Int slots: List<SlotSummary> } ControlMoved { control: String amount: Float } Playing { slot: Int role: Role } Meters { meters: List<Meter> faults: List<Fault> } PatchContents { slot: Int contents: String } Deployed { slot: Int hash: String } Refused { reasons: List<Refusal> } UpdateResult { healthy: Bool firmware: String } }
union HostMessage<PatchDocument, MidiBinding> { GetPatch { slot: Int } Deploy { slot: Int contents: SlotPatch<PatchDocument, MidiBinding> } SelectSlot { slot: Int } SetMacro { macro: String amount: Float } SetPickup { mode: PickupMode } Update { image: String signature: String } }
