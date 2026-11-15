typeDiagram
type PluginClass { classId: String className: String }
type PluginBinding { pluginNode: Int pluginModule: String pluginClass: String pluginName: String }
type RenderSession { sessionNode: Int sessionKey: String }
type DevicePending<G,S> { deviceGraph: G deviceState: S deviceChannels: Int }
type DeviceReceipt { acceptedFrames: Int returnedFrames: Int queuedFrames: Int deviceChannels: Int emptyCallbacks: Int deviceRunning: Bool }
type NodePosition { layoutNode: Int layoutX: Float layoutY: Float }
type Editor<G,P,S> { graph: G positions: List<NodePosition> dragging: Int grabX: Float grabY: Float layoutRevision: Int selected: Int nextIdentity: Int source: Int outlet: Int revision: Int message: String workerPath: String pluginPath: String catalog: List<PluginClass> pluginBindings: List<PluginBinding> pluginSerial: Int pluginTicket: String pluginClassId: String renderPending: List<P> renderBindings: List<PluginBinding> renderSerial: Int renderTicket: String renderSessions: List<RenderSession> renderOpening: Bool renderInput: String renderDevicePending: List<DevicePending<G,S>> renderDeviceKey: String renderEmptyCallbacks: Int renderContinuous: Bool renderStopping: Bool }
union EditorAction { Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } Remove {} EditParameter { identity: Int key: String } Modulate { identity: Int key: String } Audition {} PlayGraph {} StopGraph {} Surface {} PluginPath {} ScanVst {} CreateVst { identifier: String } }
