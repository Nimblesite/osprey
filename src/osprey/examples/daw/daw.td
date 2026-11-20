typeDiagram
# osprey namespace daw
type Request { project: String folder: String from: Float beats: Float }
type Written { parts: List<String> fault: String }
type Tone { breakpoint: Float feedback: Float attack: Float release: Float bloom: Float detune: Float }
type Hit { sweep: Float noise: Float colour: Float decay: Float }
union Follow { Constant {} Pitched {} Struck {} }
type Setting { key: String value: Float follow: Follow }
type PatchNode { identity: Int component: String size: Int seed: Int asset: String settings: List<Setting> }
type PatchCable { source: Int outlet: Int destination: Int inlet: Int }
type Routing { nodes: List<PatchNode> cables: List<PatchCable> attack: Float release: Float }
union Instrument { PatchInstrument { routing: Routing } PluckInstrument { brightness: Float decay: Float } BowInstrument { pressure: Float brightness: Float vibrato: Float } BellInstrument { decay: Float brightness: Float } MalletInstrument { decay: Float brightness: Float } OperatorInstrument { tone: Tone } DrumInstrument { hit: Hit } }
type Note { pitch: Int start: Float length: Float velocity: Float }
type NodePosition { layoutNode: Int layoutX: Float layoutY: Float }
type AudioSend { sendId: Int source: Int outlet: Int bus: String level: Float enabled: Bool }
type TapLane { tapNode: Int tapOutlet: Int tapChannel: Int tapGain: Float }
type Capture<K> { captureState: K captureLanes: List<List<Float>> }
type Track { name: String instrument: Instrument gain: Float pan: Float reverb: Float bus: String sends: List<AudioSend> positions: List<NodePosition> notes: List<Note> }
type Space { size: Float decay: Float damping: Float }
type Equaliser { low: Float mid: Float high: Float }
type Compressor { threshold: Float ratio: Float knee: Float attack: Float release: Float sidechain: String }
type Bus { name: String gain: Float equaliser: Equaliser compressor: Compressor }
type Afterglow { start: Float ratio: Float rise: Float hold: Float fade: Float gain: Float reverb: Float }
type Intro { recording: String tail: Float crossfade: Float }
type Arrangement { title: String tempo: Float sampleRate: Int drive: Float space: Space intro: Intro sustains: List<Afterglow> buses: List<Bus> tracks: List<Track> }
type NoteRun<S> { held: S samples: List<Float> fault: String }
type NoteSpan { gate: Float total: Int }
type VoicePair<A, B> { one: A other: B }
type TrackMix { cursor: Int carry: List<Float> samples: List<Float> fault: String }
type Stereo { name: String left: List<Float> right: List<Float> }
type Keyed { name: String samples: List<Float> }
type Buses { left: List<Float> right: List<Float> wet: List<Float> groups: List<Stereo> keys: List<Keyed> }
type BandState { low: Float band: Float level: Float }
type Mixdown { left: List<Float> right: List<Float> peak: Float }

# osprey namespace synth
# osprey import ui::Stamp ui::Primitive ui::Accessible ui::PathStep
type PluginClass { classId: String className: String }
type PluginBinding { pluginNode: Int pluginModule: String pluginClass: String pluginName: String }
type RenderSession { sessionNode: Int sessionKey: String }
type DevicePending<G,S> { deviceGraph: G deviceState: S deviceChannels: Int }
type DeviceReceipt { acceptedFrames: Int returnedFrames: Int queuedFrames: Int deviceChannels: Int emptyCallbacks: Int deviceRunning: Bool }
type CablePoint { x: Float y: Float }
type CableCurve { sx: Float sy: Float dx: Float dy: Float }
type GraphSnapshot<G> { savedGraph: G savedPositions: List<NodePosition> savedBindings: List<PluginBinding> savedSelected: Int savedNote: Int }
# [DAW-PAGES] The daw's three views of one session: a track's patch, the timeline and the mix.
# Skin marks classified once per role.
type CardMarks { cardRole: String cardMarks: List<Stamp> }
# Drawn module artwork and controls, reusable while its appearance key matches.
type CardArt { artNode: Int artKey: String artShapes: List<Primitive> artControls: List<Accessible> }
union Page { PatchPage {} TimelinePage {} MixPage {} }
union OutputEdit { OutputClosed {} ChoosingOutput { focus: Int } NamingGroup { proposed: String } }
type Editor<G,P,S,M> { skin: M graph: G past: List<GraphSnapshot<G>> future: List<GraphSnapshot<G>> gesture: List<GraphSnapshot<G>> assetFolder: String paletteShown: Bool detailsShown: Bool keyboardShown: Bool keyboardWidth: Float paletteDrag: String note: Int armedNote: Int scopeSamples: List<Float> positions: List<NodePosition> dragging: Int grabX: Float grabY: Float layoutRevision: Int selected: Int nextIdentity: Int source: Int outlet: Int wireDragging: Bool wireMonitor: Bool wireX: Float wireY: Float selectedCable: Int revision: Int message: String workerPath: String audioPath: String pluginPath: String catalog: List<PluginClass> pluginBindings: List<PluginBinding> sampleSerial: Int sampleTicket: String pluginSerial: Int pluginTicket: String pluginClassId: String renderPending: List<P> renderBindings: List<PluginBinding> renderSerial: Int renderTicket: String renderSessions: List<RenderSession> renderOpening: Bool renderInput: String renderDevicePending: List<DevicePending<G,S>> renderDeviceKey: String renderEmptyCallbacks: Int renderContinuous: Bool renderStopping: Bool cards: List<CardMarks> art: List<CardArt> page: Page outputEdit: OutputEdit inspectSpace: Bool lanePage: Int laneTraces: List<List<PathStep>> overview: List<PathStep> song: Arrangement songPath: String renderer: String track: Int patchTrack: Int selectedNote: Int rollBar: Int songTicket: String }
union EditorAction { Undo {} Redo {} Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } RemoveCable {} Remove {} EditParameter { identity: Int key: String } Modulate { identity: Int key: String } Audition {} PlayGraph {} StopGraph {} Surface {} Keyboard {} ShowModules {} ShowDetails {} ShowKeyboard {} Strike { note: Int } PluginPath {} LoadSample {} BuiltinSample {} ScanVst {} CreateVst { identifier: String } ShowPage { page: Page } PickTrack { index: Int bar: Int } SelectNote { index: Int } SetVelocity { value: Float } AdjustMix { index: Int gain: Float pan: Float reverb: Float } ScrollLanes { page: Int } ChooseOutput { index: Int offset: Int } NewGroup {} GroupName {} CommitGroup {} CloseOutput {} InspectBus { space: Bool } AssignBus { index: Int bus: String } OpenTrack { index: Int } PlaySong {} RenderSong {} StopSong {} SaveProject {} }
