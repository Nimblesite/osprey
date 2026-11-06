typeDiagram
union AccessRole { Pressable {} Slidable {} Popup {} MenuItem {} Unannounced {} }
type Accessible { accessRole: AccessRole accessLabel: String accessLeft: Float accessTop: Float accessWidth: Float accessHeight: Float accessValue: Float accessMin: Float accessMax: Float accessMovable: Bool accessSelected: Bool }
