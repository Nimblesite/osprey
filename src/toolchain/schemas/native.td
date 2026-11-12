typeDiagram
# Implements [NATIVE-SHM-OWNERSHIP], [NATIVE-SHM-PUBLISH], [NATIVE-SHM-DELIVERY].
# Logical lifecycle contract; generated ABI layouts are a separate representation.
# Int counters/sizes are checked nonnegative signed 64-bit values; overflow fails.
# Generated JS views MUST use bigint, never number (precision loss above 2^53).
# Instance UUIDs are checked against live instances; lease serials never wrap.
# Instance IDs are fresh per startup; owner tokens are unique and consumed once.
alias ApplicationInstance = Uuid
type OwnerToken { appInstance: ApplicationInstance serial: Int }
alias AllocationId = Int
alias ReuseGeneration = Int
alias SnapshotRevision = Int
alias AcceptedSequence = Int
alias CommandTicket = Int
alias ByteCount = Int
alias RecordCount = Int
alias SchemaFingerprint = String

type AllocationReference {
  appInstance: ApplicationInstance
  allocation: AllocationId
  generation: ReuseGeneration
  offset: ByteCount
}
type OwnedLease { token: OwnerToken reference: AllocationReference }
type BorrowedRange { owner: OwnerToken offset: ByteCount length: ByteCount }
type SnapshotLease { appInstance: ApplicationInstance revision: SnapshotRevision root: OwnedLease }

# These shared-process descriptors are never serialized into persistent/wire frames.
type BridgeLimits {
  chunkBytes: ByteCount
  payloadBytes: ByteCount
  ordinaryRecordsPerDirection: RecordCount
  outstandingTickets: RecordCount
  externalFrameBytes: ByteCount
  traversalDepth: RecordCount
  traversalNodes: RecordCount
  canonicalExpansionBytes: ByteCount
}
type BridgeStartup { sharedAbiFingerprint: SchemaFingerprint limits: BridgeLimits }
union BridgeDirection { ToApplication {} ToHost {} }
type RecordIdentity { appInstance: ApplicationInstance direction: BridgeDirection sequence: AcceptedSequence }
type TicketIdentity { appInstance: ApplicationInstance ticket: CommandTicket }
union BridgePhase { Open {} Closing {} Closed {} Faulted {} }
union ResourceKind {
  ChunkStorage {} PayloadStorage {} OrdinaryQueue {} TerminalReservations {}
  TicketTable {} ExternalFrame {} TraversalDepth {} TraversalNodes {} CanonicalExpansion {}
}
union BridgeError {
  InvalidLimit { resource: ResourceKind }
  LimitExceeded { resource: ResourceKind requested: Int limit: Int }
  WouldBlock { resource: ResourceKind }
  WrongInstance { expected: ApplicationInstance actual: ApplicationInstance }
  StaleAllocation { reference: AllocationReference }
  InvalidOwner { owner: OwnerToken }
  InvalidRange { offset: ByteCount length: ByteCount available: ByteCount }
  InvalidLayout { expected: SchemaFingerprint actual: SchemaFingerprint }
  CounterExhausted {}
  BridgeClosing {}
  BridgeClosed {}
  AllocationFailed { requested: ByteCount }
  SynchronizationFailed {}
}
union AcquireSnapshotResult { Acquired { snapshot: SnapshotLease } AcquireFailed { error: BridgeError } }
union RetainResult { Retained { lease: OwnedLease } RetainFailed { error: BridgeError } }
union ReleaseResult { Released {} ReleaseFailed { error: BridgeError } }
# Acceptance transfers the payload owner; refusal leaves the caller's owner intact.
union AdmissionResult { Accepted { identity: RecordIdentity } Refused { error: BridgeError } }
type OutstandingOwners { hostLeases: RecordCount gpuLeases: RecordCount tickets: RecordCount }
union CloseResult { Drained {} Busy { outstanding: OutstandingOwners } CloseFailed { error: BridgeError } }

# [NATIVE-SHM-ACCESSIBILITY] Logical contract only; host adapters remain unimplemented.
# Node serials never reuse within an instance; actions also validate source revision.
type SemanticId { appInstance: ApplicationInstance serial: Int }
type SemanticBounds { left: Float top: Float width: Float height: Float }
union SemanticRole {
  Group {} Text {} Heading {} Button {} Link {} TextField {} CheckBox {} Radio {}
  Switch {} Slider {} ComboBox {} ListRole {} ListItem {} Table {} Row {} Cell {}
  Dialog {} Menu {} MenuItem {} Tab {} TabList {} Image {} Progress {} Status {}
}
union SemanticCheck { Unchecked {} Checked {} Mixed {} }
# Secure exposes no text, text ranges, or value; never populate names with secrets.
union SemanticText { NoText {} PublicText { text: String selection: Option<TextSelection> } Secure {} }
# Text offsets count Unicode scalar values, not UTF-8 bytes or UTF-16 code units.
# Hosts convert platform offsets and reject non-boundaries or out-of-range offsets.
type TextSelection { anchor: Int focus: Int }
type SemanticRange { current: Float minimum: Float maximum: Float step: Option<Float> }
type SemanticState {
  enabled: Bool selected: Bool expanded: Option<Bool> checkState: Option<SemanticCheck>
  busy: Bool required: Bool invalid: Bool readOnly: Bool
}
union SemanticRelationKind { LabelledBy {} DescribedBy {} Controls {} Details {} ErrorMessage {} }
type SemanticRelation { relation: SemanticRelationKind targets: List<SemanticId> }
union SemanticAction {
  Activate {} Dismiss {} Increment {} Decrement {} SetText {} SetRange {}
  SetSelection {} ScrollForward {} ScrollBackward {} Materialize {}
}
# Binding serial resolves on the app executor; it is not an application action payload.
type SemanticBinding { action: SemanticAction serial: Int }
type VirtualPosition { index: Int total: Int }
type SemanticNode {
  id: SemanticId role: SemanticRole name: String description: String
  bounds: SemanticBounds semanticState: SemanticState text: SemanticText range: Option<SemanticRange>
  children: List<SemanticId> relations: List<SemanticRelation> actions: List<SemanticBinding>
  virtualPosition: Option<VirtualPosition> headingLevel: Option<Int>
}
# Children define reading order. Decorative nodes are omitted; roots/IDs are unique.
# Modal background exclusion, acyclicity and target existence require validation.
type SemanticSnapshot {
  appInstance: ApplicationInstance revision: SnapshotRevision roots: List<SemanticId>
  nodes: List<SemanticNode> keyboardFocus: Option<SemanticId>
  accessibilityFocus: Option<SemanticId> modalRoot: Option<SemanticId>
}
union FocusChannel { KeyboardFocus {} AccessibilityFocus {} }
union AnnouncementUrgency { Polite {} Assertive {} }
# Reliable queue payloads own their content independently of replaceable snapshots.
union SemanticOperation {
  RequestFocus { target: SemanticId channel: FocusChannel }
  Announce { text: String urgency: AnnouncementUrgency }
}
type SemanticDelivery {
  identity: RecordIdentity sourceRevision: SnapshotRevision operation: SemanticOperation
}
union SemanticActionValue {
  NoActionValue {} TextValue { text: String } RangeValue { value: Float }
  SelectionValue { selection: TextSelection } MaterializeValue { index: Int }
}
type SemanticInvocation {
  target: SemanticId sourceRevision: SnapshotRevision binding: Int value: SemanticActionValue
}

# [NATIVE-SHM-DELIVERY] Typed control payloads replace Incoming.eventValue.
# Accepted sequence identifies delivery, NOT the binding: a button can be used repeatedly.
# The executor validates instance/revision/region/ordinal and the issued binding kind.
# No application action, previous serialized state or generic value map crosses this boundary.
type ControlEvent {
  identity: RecordIdentity sourceRevision: SnapshotRevision region: String ordinal: Int
  now: Int value: ControlValue
}
union ControlValue {
  Tapped {}
  TextEntered { text: String }
  FlagSet { flag: Bool }
  ChoiceChosen { choice: String }
  SplitterResized { points: Int }
  CursorPlaced { offset: Int }
  StrokeFinished { stroke: BoardStroke }
  LassoClosed { enclosed: List<Int> }
  SceneControlled { input: SceneInput }
}
type BoardPoint { x: Int y: Int }
type BoardStroke { points: List<BoardPoint> width: Int endedAt: Int }
union SceneInput {
  Keyed { keyName: String down: Bool }
  Pointed { pointerId: Int x: Float y: Float down: Bool shift: Bool }
  Viewed { width: Float height: Float }
}
# Float inputs must be finite; stroke points are nonnegative, width >= 1 and points nonempty.
# Enclosed indices, ordinals and pointer IDs are nonnegative; ranges are checked by the executor.
# Signed resize/cursor inputs retain the existing bind-time clamp; time is signed Unix seconds.
# Shift is explicit (legacy missing shift means false); no inferred/default payload fields.

# [NATIVE-SHM-SCENE] One streamed rect, disc or sprite, shared as fixed 72-byte rows in a single
# sealed leaf chunk. `code` is the scene encoding's (0 rect, 1 disc, 3 sprite); `index` is a
# sprite's atlas entry; a disc's `width` is its radius. Hosts refuse non-finite floats and codes
# they cannot draw. The rows are process-local and never written to a wire or to disk.
type SceneShape {
  code: Int index: Int
  x: Float y: Float width: Float height: Float angle: Float
  colour: Int depth: Int
}

# [NATIVE-SHM-VIEW] One native control of a published view, written by compiled
# Osprey and read in place by the host. Rows are in drawing order; `parent` is an
# earlier row or -1; `action` is the binding a tap or edit delivers, or -1. Every
# style value is resolved in Osprey: the host only applies it. `textOffset` and
# `textLength` name UTF-8 bytes in the frame's text run.
type ViewRow {
  kind: Int parent: Int action: Int
  textOffset: Int textLength: Int
  colour: Int background: Int
  size: Float padding: Float spacing: Float radius: Float
  weight: Int
  width: Float height: Float grow: Float
  flags: Int border: Int
  hintOffset: Int hintLength: Int
}
