typeDiagram
type PluginClass { classId: String className: String }
type PluginBinding { pluginNode: Int pluginModule: String pluginClass: String pluginName: String }
type RenderSession { sessionNode: Int sessionKey: String }
type DevicePending<G,S> { deviceGraph: G deviceState: S deviceChannels: Int }
type DeviceReceipt { acceptedFrames: Int returnedFrames: Int queuedFrames: Int deviceChannels: Int emptyCallbacks: Int deviceRunning: Bool }
type CablePoint { x: Float y: Float }
type CableCurve { sx: Float sy: Float dx: Float dy: Float }
type NodePosition { layoutNode: Int layoutX: Float layoutY: Float }
type GraphSnapshot<G> { savedGraph: G savedPositions: List<NodePosition> savedBindings: List<PluginBinding> savedSelected: Int savedNote: Int }
type Editor<G,P,S,M> { skin: M graph: G past: List<GraphSnapshot<G>> future: List<GraphSnapshot<G>> gesture: List<GraphSnapshot<G>> assetFolder: String paletteShown: Bool detailsShown: Bool keyboardShown: Bool keyboardWidth: Float paletteDrag: String note: Int armedNote: Int scopeSamples: List<Float> positions: List<NodePosition> dragging: Int grabX: Float grabY: Float layoutRevision: Int selected: Int nextIdentity: Int source: Int outlet: Int wireDragging: Bool wireMonitor: Bool wireX: Float wireY: Float selectedCable: Int revision: Int message: String workerPath: String audioPath: String pluginPath: String catalog: List<PluginClass> pluginBindings: List<PluginBinding> sampleSerial: Int sampleTicket: String pluginSerial: Int pluginTicket: String pluginClassId: String renderPending: List<P> renderBindings: List<PluginBinding> renderSerial: Int renderTicket: String renderSessions: List<RenderSession> renderOpening: Bool renderInput: String renderDevicePending: List<DevicePending<G,S>> renderDeviceKey: String renderEmptyCallbacks: Int renderContinuous: Bool renderStopping: Bool }
union EditorAction { Undo {} Redo {} Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } RemoveCable {} Remove {} EditParameter { identity: Int key: String } Modulate { identity: Int key: String } Audition {} PlayGraph {} StopGraph {} Surface {} Keyboard {} ShowModules {} ShowDetails {} ShowKeyboard {} Strike { note: Int } PluginPath {} LoadSample {} BuiltinSample {} ScanVst {} CreateVst { identifier: String } }
