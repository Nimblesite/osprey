typeDiagram
# Design contract [PRISM-TIMELINE-GRAPH] [DAW-GRAPH]. Bind SavedNode and Port to the synth
# package's SavedNode and Port. Bind Instrument and Notes to the DAW's
# Instrument and List<Note>. The arrangement retains its tempo and master settings.
# The project graph owns shared buses. Definition keys and instance keys are
# stable identities. Captions are presentation only. A socket key is local
# to its node and direction. Pins in definitions map to boundary nodes.
union Role { Session {} Voice {} Track {} GroupBus {} ReturnBus {} MasterBus {} Rack {} }
union Direction { Inlet {} Outlet {} }
union PinUse { Signal {} MainOutput {} SendTap {} Sidechain {} }
# Project audio follows the named speakers in project order. ModulePort keeps
# the synth descriptor for an internal audio, control or timestamped note port.
union SignalType<Port> { ProjectAudio {} MidiStream {} OscStream {} ModulePort { descriptor: Port } }
type Pin<Port> { key: String direction: Direction use: PinUse signalType: SignalType<Port> }
type Socket { node: String pin: String }
type Wire { key: String source: Socket destination: Socket }
union NoteSource { NoteHertz {} NoteVelocity {} }
type StrikeBinding { path: List<String> parameter: String source: NoteSource scale: Float }
union UnitNode<SavedNode, Instrument> {
    Module { state: SavedNode }
    BuiltInVoice { instrument: Instrument }
    Instance { definition: String }
    Voices { definition: String attackSeconds: Float releaseSeconds: Float bindings: List<StrikeBinding> }
    Boundary { pin: String }
    Sum { }
    Gain { linear: Float }
    Panorama { pan: Float }
    Send { linear: Float enabled: Bool }
    DeviceOut { }
}
type Vertex<SavedNode, Port, Instrument> { key: String name: String unit: UnitNode<SavedNode, Instrument> pins: List<Pin<Port>> }
type Definition<SavedNode, Port, Instrument> { key: String name: String role: Role pins: List<Pin<Port>> nodes: List<Vertex<SavedNode, Port, Instrument>> wires: List<Wire> }
type TrackLane<Notes> { key: String name: String instance: String noteIn: String notes: Notes }
type RoutingProject<SavedNode, Port, Instrument, Notes> { revision: Int root: String speakers: List<String> definitions: List<Definition<SavedNode, Port, Instrument>> lanes: List<TrackLane<Notes>> }
