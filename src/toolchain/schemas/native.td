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

# [NATIVE-SHM-SCENE] One row of a canvas's scene, shared as fixed 88-byte rows in a single sealed
# leaf chunk. `code` 20 starts a scene: `index` is the canvas's view row, `x`, `y` and `width` its
# width, height and zoom; the rows after it, until the next 20, belong to that canvas. Codes: 0 rect,
# 1 disc (`width` is the radius), 2 strip (`index` pairs follow as 7 rows, `width` the stroke),
# 3 sprite (`index` the atlas entry), 4 glyphs and 6 lettering (text in `textOffset`/`textLength`,
# `width` the size; lettering's `index` is its anchor and `height` its weight), 5 path (`index` steps
# of code 8 follow, then one 9 outline and one 10 paint; `x`, `y`, `width` move it and scale it),
# 7 a point or a continuation's values, 8 a path step (`index` 0 move, 1 line, 2 quadratic,
# 3 cubic and one 7, 4 arc with `colour` 1 large and 2 sweep, 5 polygon with `depth` 7 rows and
# `colour` 1 closed, 6 close), 9 outline (`index` 0 fill or 1 line of `x` width, `colour` 1 bevel,
# `depth` dash 7 rows), 10 paint (`index` 0 solid or 1 linear, then its tints as 11 rows: `index`
# 0 RGB or 1 HSL in `x`..`height`; linear's `x` angle, `y` and `width` stops, `colour` 1 oklab),
# 21 an atlas entry's name in its text, 22 an accessible element (`index` role, `x`..`height` its
# box, `angle` its value, `colour` 1 selected and 2 movable, the label in its text) and one 7 with
# its minimum and maximum. Hosts refuse non-finite floats, codes they cannot draw, counts that run
# past the rows and text ranges outside the run. The rows are process-local and never written to a
# wire or to disk.
type SceneShape {
  code: Int index: Int
  x: Float y: Float width: Float height: Float angle: Float
  colour: Int depth: Int
  textOffset: Int textLength: Int
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

# [NATIVE-SHM-ASK] One field of a capability request or a slice write in the published frame,
# replacing the JSON request list. A row whose `field` is 0 starts an ask: its `number` is the
# capability's code and its text the ticket (a slice write's text is its key). The rows after it,
# until the next field-0 row, are that ask's arguments: `field` is the argument's position in the
# capability's declaration, counted from 1; a list repeats its field once per item, a pair (a header
# or an environment variable) is two rows in order; whole numbers and flags are in `number`, text in
# `textOffset` and `textLength` (UTF-8 bytes of the frame's text run). Hosts refuse codes they do not
# know, fields out of range and text ranges outside the run.
type AskRow { field: Int number: Int textOffset: Int textLength: Int }
