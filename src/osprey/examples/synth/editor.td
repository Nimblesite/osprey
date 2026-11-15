typeDiagram
type Editor<G> { graph: G selected: Int nextIdentity: Int source: Int outlet: Int revision: Int message: String }
union EditorAction { Add { kind: String } Select { identity: Int } Source { identity: Int outlet: Int } Connect { identity: Int inlet: Int } Disconnect { identity: Int inlet: Int } Remove {} EditParameter { identity: Int key: String } Audition {} Surface {} }
