typeDiagram
# Implements [NATIVE-SHM-OWNERSHIP], [NATIVE-SHM-PUBLISH], [NATIVE-SHM-DELIVERY].
# Logical lifecycle contract; generated ABI layouts are a separate representation.
# Int counters/sizes are checked nonnegative signed 64-bit values; overflow fails.
# Generated JS views MUST use bigint, never number (precision loss above 2^53).
# Instance UUIDs are checked against live instances; lease serials never wrap.
# Instance IDs are fresh per startup; owner tokens are unique and consumed once.
alias ApplicationInstance = Uuid
type OwnerToken { instance: ApplicationInstance serial: Int }
alias AllocationId = Int
alias ReuseGeneration = Int
alias SnapshotRevision = Int
alias AcceptedSequence = Int
alias CommandTicket = Int
alias ByteCount = Int
alias RecordCount = Int
alias SchemaFingerprint = String

type AllocationReference {
  instance: ApplicationInstance
  allocation: AllocationId
  generation: ReuseGeneration
  offset: ByteCount
}
type OwnedLease { token: OwnerToken reference: AllocationReference }
type BorrowedRange { owner: OwnerToken offset: ByteCount length: ByteCount }
type SnapshotLease { instance: ApplicationInstance revision: SnapshotRevision root: OwnedLease }

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
type RecordIdentity { instance: ApplicationInstance direction: BridgeDirection sequence: AcceptedSequence }
type TicketIdentity { instance: ApplicationInstance ticket: CommandTicket }
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
  Closing {}
  Closed {}
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
