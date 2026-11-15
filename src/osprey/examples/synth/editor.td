typeDiagram
type PluginClass { classId: String className: String }
type PluginBinding { pluginNode: Int pluginModule: String pluginClass: String pluginName: String }
type NodePosition { layoutNode: Int layoutX: Float layoutY: Float }
type Editor<G,C> { graph: G positions: List<NodePosition> dragging: Int grabX: Float grabY: Float layoutRevision: Int selected: Int nextIdentity: Int source: Int outlet: Int revision: Int message: String workerPath: String pluginPath: String catalog: List<PluginClass> pluginBindings: List<PluginBinding> pluginSerial: Int pluginTicket: String pluginClassId: String renderGraph: G renderClips: List<C> renderBindings: List<PluginBinding> renderSerial: Int renderTicket: String renderNode: Int }
union EditorAction { Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } Remove {} EditParameter { identity: Int key: String } Modulate { identity: Int key: String } Audition {} Surface {} PluginPath {} ScanVst {} CreateVst { identifier: String } }
