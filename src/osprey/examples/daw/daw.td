typeDiagram
# osprey namespace daw
# osprey import synth::Prepared synth::Rendered synth::OperatorState synth::GraphState synth::ProcessorMemory synth::KernelState
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
type Track { name: String instrument: Instrument gain: Float pan: Float bus: String sends: List<AudioSend> positions: List<NodePosition> notes: List<Note> }
type Equaliser { low: Float mid: Float high: Float }
type Compressor { threshold: Float ratio: Float knee: Float attack: Float release: Float sidechain: String }
type Bus { name: String output: String gain: Float equaliser: Equaliser compressor: Compressor }
type Arrangement { title: String tempo: Float meterTop: Int meterBottom: Int sampleRate: Int buses: List<Bus> tracks: List<Track> }
type NoteRun<S> { held: S samples: List<Float> fault: String }
type NoteSpan { gate: Float total: Int }
type VoicePair<A, B> { one: A other: B }
# [DAW-RENDER-PLAY] Playback state: cued notes, sounding notes and per-bus processor state.
type Cue { due: Int note: Note }
union NoteMemory { ScalarNote { audio: Rendered } MultichannelNote { clock: GraphState<ProcessorMemory<KernelState>> } }
type ActivePatch { plan: Prepared ports: List<TapLane> span: NoteSpan attack: Float release: Float memory: NoteMemory }
type ActiveOperator { span: NoteSpan tone: Tone firstHz: Float secondHz: Float phases: VoicePair<OperatorState, OperatorState> sends: List<AudioSend> }
union BuiltInState { Plucked { guide: WaveguideState burst: ExciterState capacity: Int brightness: Float decay: Float } Bowed { strand: BowState capacity: Int pressure: Float brightness: Float depth: Float } Rung { bank: ModalBank modes: ModalState } Drummed { tone: OperatorState burst: ExciterState noise: ExciterConfig hit: Hit } }
type ActiveVoice { span: NoteSpan current: BuiltInState sends: List<AudioSend> }
union NoteDriver { PerformingNote { patch: ActivePatch } PerformingOperator { pair: ActiveOperator } PerformingVoice { voice: ActiveVoice } }
type Sounding { cue: Cue at: Int lanes: List<List<Float>> driver: NoteDriver }
type TrackPlay { pending: List<Cue> sounding: List<Sounding> sends: List<AudioSend> }
type Advanced { play: TrackPlay buses: Buses }
type Heard { plays: List<TrackPlay> buses: Buses }
type BusState { name: String leftBand: BandState rightBand: BandState level: Float }
type Mastering { buses: Buses states: List<BusState> }
type Playing { score: Arrangement cursor: Int tracks: List<TrackPlay> states: List<BusState> }
type LiveStream { playing: Playing controlVersion: Int controlScore: Arrangement }
type Played { playing: Playing left: List<Float> right: List<Float> }
type Take { playing: Playing lefts: List<List<Float>> rights: List<List<Float>> }
type Stereo { name: String left: List<Float> right: List<Float> }
type Keyed { name: String samples: List<Float> }
type Buses { left: List<Float> right: List<Float> groups: List<Stereo> keys: List<Keyed> }
type BandState { low: Float band: Float level: Float }
type Mixdown { left: List<Float> right: List<Float> peak: Float }

# osprey namespace synth
# osprey import ui::Stamp ui::Primitive ui::Accessible ui::PathStep
type PluginClass { classId: String className: String bundle: String }
type PluginWindow { windowNode: Int windowClass: String windowTrack: Int windowProject: String windowSerial: Int windowCycle: Int windowFocus: Bool windowClosing: Bool windowHandle: String windowSent: List<Float> }
type PluginBinding { pluginNode: Int pluginModule: String pluginClass: String pluginName: String }
type DeviceReceipt { acceptedFrames: Int returnedFrames: Int queuedFrames: Int deviceChannels: Int emptyCallbacks: Int deviceRunning: Bool }
type CablePoint { x: Float y: Float }
type CableCurve { sx: Float sy: Float dx: Float dy: Float }
type GraphSnapshot<G> { savedGraph: G savedSong: Arrangement savedTrack: Int savedPatchTrack: Int savedTimelineNote: Int savedNextIdentity: Int savedPositions: List<NodePosition> savedFlow: List<NodePosition> savedBindings: List<PluginBinding> savedSelected: Int savedNote: Int }
# [DAW-PAGES] The daw's three views of one session: a track's patch, the timeline and the mix.
# Skin marks classified once per role.
type CardMarks { cardRole: String cardMarks: List<Stamp> }
# Drawn module artwork and controls, reusable while its appearance key matches.
type CardArt { artNode: Int artKey: String artShapes: List<Primitive> artControls: List<Accessible> }
union Page { PatchPage {} TimelinePage {} MixPage {} }
type SendPort { portKey: String portCaption: String portNode: Int portOutlet: Int portChannels: Int connections: List<String> }
type SendDraft { routeOffset: Int editingId: Int sourceKey: String targetBus: String levelText: String sendActive: Bool options: List<SendPort> }
union OutputEdit { SongTiming { bpm: String meter: String } PluginSettings { folder: String } NodeMenu { nodeId: Int left: Float top: Float } BrowsingSends { offset: Int } EditingBus { busName: String } EditingSend { draft: SendDraft } OutputClosed {} ChoosingOutput { focus: Int } NamingGroup { proposed: String } }
type RollMotion { anchorX: Float anchorY: Float rowSize: Float stem: Bool original: Note preview: Note }
union RollDrag { RollIdle {} RollHeld { motion: RollMotion } }
type Shortcut { command: String keys: String }
type Keymap { name: String shortcuts: List<Shortcut> }
# [DAW-CONTROLS-DIAL] The dial being turned: where the press began and the value it began with.
union MixFader { FaderIdle {} TrackFader { index: Int } GroupFader { name: String } }
union DialTurn { DialIdle {} DialButtonDown {} DialHeld { identity: Int key: String startY: Float startValue: Float moved: Bool } }
union PatchPan { PanIdle {} PanHeld { lastX: Float lastY: Float } }
type Editor<G,M> { skin: M graph: G past: List<GraphSnapshot<G>> future: List<GraphSnapshot<G>> gesture: List<GraphSnapshot<G>> assetFolder: String paletteShown: Bool detailsShown: Bool paletteDrag: String note: Int armedNote: Int positions: List<NodePosition> flowPositions: List<NodePosition> panX: Float panY: Float panning: PatchPan dragging: Int grabX: Float grabY: Float layoutRevision: Int selected: Int nextIdentity: Int source: Int outlet: Int wireDragging: Bool wireMonitor: Bool wireX: Float wireY: Float selectedCable: Int revision: Int message: String workerPath: String audioPath: String pluginPath: String scanQueue: List<String> scanWarning: String catalog: List<PluginClass> pluginBindings: List<PluginBinding> sampleSerial: Int sampleTicket: String pluginSerial: Int patchWindowSerial: Int editorWindows: List<PluginWindow> pluginTicket: String pluginClassId: String cards: List<CardMarks> art: List<CardArt> page: Page outputEdit: OutputEdit laneTop: Int laneZoom: Float laneStart: Float laneTraces: List<List<PathStep>> overview: List<PathStep> song: Arrangement songPath: String renderer: String track: Int patchTrack: Int selectedNote: Int rollDrag: RollDrag rollBar: Int playEnd: Int playBeat: Float playClock: Float looping: Bool songTicket: String keymap: Keymap keysPath: String mixFader: MixFader dial: DialTurn dialOffset: Int dialEntry: String fineHeld: Bool songWidth: Float songHeight: Float }
union EditorAction { PatchSurface {} ClosePluginWindow { identity: Int } OpenPluginWindow { identity: Int } Undo {} Redo {} Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } RemoveCable {} Remove {} EditParameter { identity: Int key: String } Modulate { identity: Int key: String } Surface {} ShowModules {} ShowDetails {} PluginPath {} LoadSample {} BuiltinSample {} EditTiming {} TempoText {} PickMeter {} ApplyTiming {} ShowSettings {} SaveSettings {} ShowPage { page: Page } PickTrack { index: Int bar: Int } SelectNote { index: Int } SetVelocity { value: Float } AdjustMix { index: Int gain: Float pan: Float } AdjustBusGain { name: String gain: Float } ScrollLanes { top: Int } ZoomLanes { factor: Float } ChooseOutput { index: Int offset: Int } NewGroup {} BrowseBuses {} BrowseSends { offset: Int } PickBus {} RouteBus {} BeginSend { identity: Int } PagePortRoutes { offset: Int } SendSource {} SendDestination {} SendLevel {} FlipSend {} CommitSend {} DeleteSend {} GroupName {} CommitGroup {} CloseOutput {} AssignBus { index: Int bus: String } OpenTrack { index: Int } PlaySong {} RenderSong {} StopSong {} ToggleLoop {} MoveEnd { bar: Int } MoveStart { bar: Int } SaveProject {} UseKeymap { name: String } LoadKeymap {} Controls {} }
