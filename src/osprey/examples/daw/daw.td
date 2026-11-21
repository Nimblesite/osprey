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
type Bus { name: String output: String gain: Float equaliser: Equaliser compressor: Compressor }
type Afterglow { start: Float ratio: Float rise: Float hold: Float fade: Float gain: Float reverb: Float }
type Intro { recording: String tail: Float crossfade: Float }
type Arrangement { title: String tempo: Float sampleRate: Int drive: Float space: Space intro: Intro sustains: List<Afterglow> buses: List<Bus> tracks: List<Track> }
type NoteRun<S> { held: S samples: List<Float> fault: String }
type NoteSpan { gate: Float total: Int }
type VoicePair<A, B> { one: A other: B }
# [DAW-RENDER-PLAY] Playback state: cued notes, sounding notes, per-bus processor state and the Space rooms.
type Cue { due: Int note: Note }
type Sounding { at: Int lanes: List<List<Float>> }
# A note's sound depends on its pitch, length and velocity alone, so each distinct note renders once.
type Made { struck: Note chord: List<List<Float>> }
type TrackPlay { pending: List<Cue> sounding: List<Sounding> sends: List<AudioSend> made: List<Made> }
type Advanced { play: TrackPlay buses: Buses }
type Heard { plays: List<TrackPlay> buses: Buses }
type Pad { glow: Afterglow sound: Sounding }
type BusState { name: String leftBand: BandState rightBand: BandState level: Float }
type Mastering { buses: Buses states: List<BusState> }
type Room<C, H> { config: C held: H }
type Playing<C, H> { cursor: Int tracks: List<TrackPlay> pads: List<Pad> states: List<BusState> near: Room<C, H> far: Room<C, H> }
type Played<C, H> { playing: Playing<C, H> left: List<Float> right: List<Float> }
type Take<C, H> { playing: Playing<C, H> lefts: List<List<Float>> rights: List<List<Float>> }
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
type GraphSnapshot<G> { savedGraph: G savedSong: Arrangement savedTrack: Int savedPatchTrack: Int savedTimelineNote: Int savedNextIdentity: Int savedPositions: List<NodePosition> savedBindings: List<PluginBinding> savedSelected: Int savedNote: Int }
# [DAW-PAGES] The daw's three views of one session: a track's patch, the timeline and the mix.
# Skin marks classified once per role.
type CardMarks { cardRole: String cardMarks: List<Stamp> }
# Drawn module artwork and controls, reusable while its appearance key matches.
type CardArt { artNode: Int artKey: String artShapes: List<Primitive> artControls: List<Accessible> }
union Page { PatchPage {} TimelinePage {} MixPage {} }
type SendPort { portKey: String portCaption: String portNode: Int portOutlet: Int portChannels: Int connections: List<String> }
type SendDraft { routeOffset: Int editingId: Int sourceKey: String targetBus: String levelText: String sendActive: Bool options: List<SendPort> }
union OutputEdit { BrowsingSends { offset: Int } EditingBus { busName: String } EditingSend { draft: SendDraft } OutputClosed {} ChoosingOutput { focus: Int } NamingGroup { proposed: String } }
type RollMotion { anchorX: Float anchorY: Float rowSize: Float stem: Bool original: Note preview: Note }
union RollDrag { RollIdle {} RollHeld { motion: RollMotion } }
type Shortcut { command: String keys: String }
type Keymap { name: String shortcuts: List<Shortcut> }
# [DAW-CONTROLS-DIAL] The dial being turned: where the press began and the value it began with.
union DialTurn { DialIdle {} DialHeld { identity: Int key: String startY: Float startValue: Float moved: Bool } }
type Editor<G,P,S,M> { skin: M graph: G past: List<GraphSnapshot<G>> future: List<GraphSnapshot<G>> gesture: List<GraphSnapshot<G>> assetFolder: String paletteShown: Bool detailsShown: Bool keyboardShown: Bool keyboardWidth: Float paletteDrag: String note: Int armedNote: Int scopeSamples: List<Float> positions: List<NodePosition> dragging: Int grabX: Float grabY: Float layoutRevision: Int selected: Int nextIdentity: Int source: Int outlet: Int wireDragging: Bool wireMonitor: Bool wireX: Float wireY: Float selectedCable: Int revision: Int message: String workerPath: String audioPath: String pluginPath: String catalog: List<PluginClass> pluginBindings: List<PluginBinding> sampleSerial: Int sampleTicket: String pluginSerial: Int pluginTicket: String pluginClassId: String renderPending: List<P> renderBindings: List<PluginBinding> renderSerial: Int renderTicket: String renderSessions: List<RenderSession> renderOpening: Bool renderInput: String renderDevicePending: List<DevicePending<G,S>> renderDeviceKey: String renderEmptyCallbacks: Int renderContinuous: Bool renderStopping: Bool cards: List<CardMarks> art: List<CardArt> page: Page outputEdit: OutputEdit inspectSpace: Bool lanePage: Int laneZoom: Float laneStart: Float laneTraces: List<List<PathStep>> overview: List<PathStep> song: Arrangement songPath: String renderer: String track: Int patchTrack: Int selectedNote: Int rollDrag: RollDrag rollBar: Int playEnd: Int playBeat: Float playClock: Float looping: Bool songTicket: String keymap: Keymap keysPath: String dial: DialTurn dialEntry: String fineHeld: Bool }
union EditorAction { Undo {} Redo {} Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } RemoveCable {} Remove {} EditParameter { identity: Int key: String } Modulate { identity: Int key: String } Audition {} PlayGraph {} StopGraph {} Surface {} Keyboard {} ShowModules {} ShowDetails {} ShowKeyboard {} Strike { note: Int } PluginPath {} LoadSample {} BuiltinSample {} ScanVst {} CreateVst { identifier: String } ShowPage { page: Page } PickTrack { index: Int bar: Int } SelectNote { index: Int } SetVelocity { value: Float } AdjustMix { index: Int gain: Float pan: Float reverb: Float } ScrollLanes { page: Int } ZoomLanes { factor: Float } ChooseOutput { index: Int offset: Int } NewGroup {} BrowseBuses {} BrowseSends { offset: Int } PickBus {} RouteBus {} BeginSend { identity: Int } PagePortRoutes { offset: Int } SendSource {} SendDestination {} SendLevel {} FlipSend {} CommitSend {} DeleteSend {} GroupName {} CommitGroup {} CloseOutput {} InspectBus { space: Bool } AssignBus { index: Int bus: String } OpenTrack { index: Int } PlaySong {} RenderSong {} StopSong {} ToggleLoop {} MoveEnd { bar: Int } SaveProject {} UseKeymap { name: String } LoadKeymap {} Controls {} }
