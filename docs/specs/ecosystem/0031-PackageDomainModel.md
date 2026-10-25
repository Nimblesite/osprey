# Package Domain Model

> **ALL OSPREY, and A WORKING API THAT SERVES WORKING PACKAGES COMES FIRST.**
> Every executable part of the package system is Osprey, with no exceptions
> (`[PACKAGE-IMPLEMENTATION-LANGUAGE]`). The first deliverable is the
> database-free folder registry of `[PACKAGE-FOLDER-REGISTRY]` in
> [spec 0029](0029-PackageManagement.md); everything in this document layers
> onto that running service.

**Status:** normative target. This is the typed data contract for
[Package Management](0029-PackageManagement.md) and
[Registry Trust and Discovery](0030-PackageRegistryTrustAndDiscovery.md), with
score semantics fixed by [Evidence and Fair Discovery](0032-PackageEvidenceAndDiscovery.md).

The model uses current
[typeDiagram language syntax](https://typediagram.dev/docs/language-reference.html).
It is intentionally expressed with `type`, `union`, and `alias` so typeDiagram
can generate the Osprey model when that output target lands. Field removal,
meaning change, or union narrowing is a schema-breaking change; additive fields
require a lock/API format bump unless they have an explicit compatible default.

Executable model generation, validation, canonical serialization, and conformance tooling MUST be implemented in Osprey under [the implementation-language contract](0029-PackageManagement.md#implementation-language-package-implementation-language). Generated package-domain source MUST be Osprey; a handwritten or generated implementation in another language is not permitted. Until Osprey model generation is available, Osprey code consumes serialized conformance fixtures directly, without a duplicate handwritten domain model. The typeDiagram schema and serialized fixtures remain declarative data.

The canonical source is [`schemas/package-registry.td`](../../../schemas/package-registry.td). The block below is its byte-identical documentation projection, enforced by the existing schema parity test; do not maintain a separate model. [Model invariants](0031-PackageDomainInvariants.md) and [organization governance](0045-OrganizationPackageGovernance.md) constrain these shapes.

```typediagram
typeDiagram
alias ScopeName = String
alias PackageName = String
alias CommitId = String
alias Digest = String
alias RelativePath = String
alias LocalPath = String
alias CompatibilityEpoch = Int
alias ReleaseOrdinal = Int
alias PrincipalId = String
type PackageKey { scope: ScopeName name: PackageName }
alias OrganizationId = String
alias MembershipId = String
alias PackageId = String
alias GovernanceId = String
alias AuthorityEpoch = Int
union PrincipalKind { Human {} Workflow {} }
union HumanPrincipalKind { Human {} }
union PrincipalState { Active {} Suspended {} }
union OrganizationState { Active {} Frozen {} Tombstoned {} }
union OrganizationRole { Custodian {} Member {} }
union MembershipState { Active {} Revoked {} }
union ActiveMembershipState { Active {} }
union RegistryPackageState { Draft {} Active {} WriteFrozen {} Deprecated {} Tombstoned {} }
union PackagePermission { PublishRelease {} YankRelease {} EditMetadata {} ManageMaintenance {} ManageSecurity {} }
union PermissionState { Active {} Revoked {} }
union AdminTransferState { Pending {} Completed {} Cancelled {} Declined {} Expired {} }
union InvitationState { Pending {} Accepted {} Revoked {} Expired {} }
union MaintenanceReportState { Open {} Acknowledged {} Resolved {} Withdrawn {} }
type RegistryPrincipal { id: PrincipalId kind: PrincipalKind state: PrincipalState credentialEpoch: AuthorityEpoch createdAt: DateTime }
type RegistryAuthBinding { id: GovernanceId principalId: PrincipalId issuer: String provider: String providerSubject: String state: PermissionState verifiedAt: DateTime }
type Organization { id: OrganizationId handle: ScopeName displayName: String state: OrganizationState governanceEpoch: AuthorityEpoch createdAt: DateTime }
type OrganizationMembership {
  id: MembershipId
  organizationId: OrganizationId
  principalId: PrincipalId
  role: OrganizationRole
  principalKind: PrincipalKind
  state: MembershipState
  authorityEpoch: AuthorityEpoch
  acceptedAt: DateTime
}
type OrganizationRecoveryAuthority { id: GovernanceId organizationId: OrganizationId principalId: PrincipalId principalKind: HumanPrincipalKind state: PermissionState governanceEpoch: AuthorityEpoch enrollmentDigest: Digest enrolledAt: DateTime }
type RegistryPackage {
  id: PackageId
  organizationId: OrganizationId
  name: PackageName
  adminMembershipId: MembershipId
  adminMembershipState: ActiveMembershipState
  adminPrincipalKind: HumanPrincipalKind
  authorityEpoch: AuthorityEpoch
  state: RegistryPackageState
  createdAt: DateTime
}
type PackagePermissionGrant {
  id: GovernanceId
  organizationId: OrganizationId
  packageId: PackageId
  membershipId: MembershipId
  permission: PackagePermission
  state: PermissionState
  grantedByMembershipId: MembershipId
  authorityEpoch: AuthorityEpoch
  grantedAt: DateTime
  expiresAt: Option<DateTime>
}
type PackageAdminTransfer {
  id: GovernanceId
  organizationId: OrganizationId
  packageId: PackageId
  fromMembershipId: MembershipId
  toMembershipId: MembershipId
  toMembershipState: ActiveMembershipState
  toPrincipalKind: HumanPrincipalKind
  expectedAuthorityEpoch: AuthorityEpoch
  state: AdminTransferState
  proposalDigest: Digest
  acceptanceDigest: Option<Digest>
  createdAt: DateTime
  notBefore: DateTime
  expiresAt: DateTime
  decidedAt: Option<DateTime>
}
type OrganizationInvitation { id: GovernanceId organizationId: OrganizationId targetPrincipalId: PrincipalId issuerMembershipId: MembershipId role: OrganizationRole tokenDigest: Digest state: InvitationState expectedGovernanceEpoch: AuthorityEpoch createdAt: DateTime expiresAt: DateTime }
type PackageMaintenanceReport { id: GovernanceId organizationId: OrganizationId packageId: PackageId reporterMembershipId: MembershipId summary: String evidenceDigest: Digest state: MaintenanceReportState createdAt: DateTime }
union GovernanceEventKind { OrganizationCreated {} MembershipChanged {} PackageCreated {} PermissionChanged {} AdminProposed {} AdminReplaced {} MaintenanceChanged {} Recovery {} OrganizationTransfer {} }
type GovernanceEvent { id: GovernanceId organizationId: OrganizationId packageId: Option<PackageId> actorPrincipalId: PrincipalId kind: GovernanceEventKind authorityEpoch: AuthorityEpoch proofDigest: Digest previousEventDigest: Option<Digest> createdAt: DateTime }
type SourceRef { forge: String repositoryId: String subdirectory: RelativePath commit: CommitId }
type RepositoryBinding {
  package: PackageKey
  forge: String
  repositoryId: String
  subdirectory: RelativePath
  releaseBranch: String
  authorizationExpiresAt: DateTime
  nonce: String
}
union CredentialProof {
  SigstoreCi {
    issuer: String
    subject: String
    repository: String
    workflow: String
    gitRef: String
    certificateDigest: Digest
    transparencyBundleDigest: Digest
    signatureDigest: Digest
  }
  WebAuthnPasskey {
    credentialId: String
    relyingPartyId: String
    origin: String
    userVerified: Bool
    assertionDigest: Digest
  }
}
union EnvelopeProof {
  Publisher { credential: CredentialProof }
  ForgeAdmin { installationId: String repositoryId: String attestationDigest: Digest }
  RegistryRole { role: String keyId: String signatureDigest: Digest }
}
type AuthorizationProof { principal: PrincipalId proof: EnvelopeProof }
type SignedRepositoryBinding { payload: RepositoryBinding proofs: List<AuthorizationProof> }
type ReleaseId { package: PackageKey epoch: CompatibilityEpoch ordinal: ReleaseOrdinal digest: Digest }
union CompatibilityClaim {
  Compatible { previous: Digest }
  Breaking { previousEpoch: CompatibilityEpoch }
  Initial { package: PackageKey }
}
type ReleaseIntent {
  package: PackageKey
  source: SourceRef
  repositoryBindingDigest: Digest
  forgeHashAlgorithm: String
  expectedSourceDigest: Digest
  expectedManifestDigest: Digest
  compatibility: CompatibilityClaim
  expiresAt: DateTime
  nonce: String
}
type SignedReleaseIntent { payload: ReleaseIntent proofs: List<AuthorizationProof> }
type ReleaseChallenge {
  intentEnvelopeDigest: Digest
  release: ReleaseId
  casDigest: Digest
  sourceDigest: Digest
  manifestDigest: Digest
  resolutionContextDigest: Digest
  dependencyLockDigest: Digest
  buildPlanDigest: Digest
  releaseDigest: Digest
  compatibility: CompatibilityClaim
  expiresAt: DateTime
  intentNonce: String
  challengeNonce: String
}
type SignedReleaseChallenge { payload: ReleaseChallenge proofs: List<AuthorizationProof> }
type PublicationAuthorization { challengeEnvelopeDigest: Digest }
type SignedPublicationAuthorization { payload: PublicationAuthorization proofs: List<AuthorizationProof> }
union ReleaseState { Submitted { submittedAt: DateTime } Quarantined { reason: String } Published { publishedAt: DateTime } }
union ActivationState { Eligible {} Yanked {} Revoked {} }
type ReleaseStatus { release: ReleaseId sequence: Int state: ActivationState effectiveAt: DateTime reasonDigest: Option<Digest> previousStatus: Option<Digest> roleMetadataVersion: Int }
type DependencyRequirement { package: PackageKey epoch: CompatibilityEpoch minimumOrdinal: ReleaseOrdinal }
type DependencyGrant { package: PackageKey capabilities: List<String> }
type SystemRequirement { capability: String minimumRevision: Int }
union PackageAssetKind { Utf8Text {} Json {} Png {} }
union PackageAssetRole { Documentation { kind: PackageAssetKind } RuntimeData { kind: PackageAssetKind requiredCapability: String } }
type PackageAsset { path: RelativePath byteLength: Int digest: Digest role: PackageAssetRole }
union DependencyEdge { Runtime { requirement: DependencyRequirement } Build { requirement: DependencyRequirement } Test { requirement: DependencyRequirement } }
type PackageDiscoveryMetadata { summary: String keywords: List<String> }
union SourceFlavor { Default {} Ml {} }
type PublishedPackageMetadata { package: PackageKey licenseExpression: String discovery: PackageDiscoveryMetadata }
type PackageManifest {
  projectName: String
  sourceRoots: List<RelativePath>
  defaultNamespace: Option<String>
  entry: Option<RelativePath>
  flavor: Option<SourceFlavor>
  allowWildcardImports: Bool
  publication: Option<PublishedPackageMetadata>
  testRoots: List<RelativePath>
  pythonSources: List<RelativePath>
  dependencies: List<DependencyEdge>
  grants: List<DependencyGrant>
  systemRequirements: List<SystemRequirement>
  assets: List<PackageAsset>
}
union DependencySelectionRef { Release { selectionDigest: Digest } Development { developmentLockDigest: Digest } }
type ModulePlanEntry { path: RelativePath flavor: SourceFlavor sourceBytesDigest: Digest imports: List<String> publicInterfaceDigest: Digest }
type ModuleGraphProjection { modules: List<ModulePlanEntry> }
type CompilerInvocationProjection { compilerDigest: Digest runtimeDigest: Digest targetSpecDigest: Digest cpuFeatureSetDigest: Digest flags: List<String> normalizedEnvironmentDigest: Digest sandboxPolicyDigest: Digest }
type TestPlanEntry { path: RelativePath namespace: String symbol: String }
type TestPlanProjection { tests: List<TestPlanEntry> timeoutMilliseconds: Int randomSeedDigest: Digest policyDigest: Digest }
union OutputArtifactKind { Library {} Executable {} TestReport {} Sbom {} }
type OutputArtifact { path: RelativePath kind: OutputArtifactKind executable: Bool }
type OutputLayoutProjection { artifacts: List<OutputArtifact> }
type BuildPlan {
  format: Int
  package: Option<PackageKey>
  sourceDigest: Digest
  manifestDigest: Digest
  resolutionContextDigest: Digest
  dependencySelection: DependencySelectionRef
  moduleGraphDigest: Digest
  compilerInvocationDigest: Digest
  testPlanDigest: Digest
  outputLayoutDigest: Digest
  inferredCapabilities: List<String>
}
type ReleaseIdentityEnvelope { package: PackageKey epoch: CompatibilityEpoch sourceDigest: Digest manifestDigest: Digest buildPlanDigest: Digest }
type Release {
  id: ReleaseId
  source: SourceRef
  sourceDigest: Digest
  manifestDigest: Digest
  buildPlanDigest: Digest
  authorizationSetDigest: Digest
  provenanceDigest: Digest
  sbomDigest: Digest
  state: ReleaseState
  inferredCapabilities: List<String>
}
type LocalOverride { package: PackageKey path: LocalPath epoch: CompatibilityEpoch baseline: Option<ReleaseId> }
type DevelopmentOverlay { overrides: List<LocalOverride> }
type ProviderCatalogEntry { capability: String targetSpecDigest: Digest provider: String revision: Int artifactDigest: Digest closureDigests: List<Digest> provenanceDigest: Digest }
type LockedSystemInput {
  capability: String
  provider: String
  revision: Int
  artifactDigest: Digest
  closureDigests: List<Digest>
  provenanceDigest: Digest
}
type LockedBuildEnvironment {
  compilerDigest: Digest
  runtimeDigest: Digest
  toolchainClosure: List<Digest>
  sdkSysrootClosure: List<Digest>
  targetSpecDigest: Digest
  cpuFeatureSetDigest: Digest
  flagsDigest: Digest
  sandboxPolicyDigest: Digest
  sandboxImageDigest: Digest
  normalizedEnvironmentDigest: Digest
  clockPolicyDigest: Digest
  randomnessPolicyDigest: Digest
  randomSeedDigest: Digest
  localePolicyDigest: Digest
  timezonePolicyDigest: Digest
  filesystemOrderPolicyDigest: Digest
  kernelAbiDigest: Digest
}
type ResolutionContext {
  catalogSnapshot: Digest
  catalogAsOf: DateTime
  providerCatalogDigest: Digest
  tufRootVersion: Int
  tufRootDigest: Digest
  tufSnapshotDigest: Digest
  tufTimestampDigest: Digest
  releaseStatusMetadataDigest: Digest
  logCheckpointDigest: Digest
  verificationBundleDigest: Digest
  solverPolicy: Digest
  buildEnvironment: LockedBuildEnvironment
  providerEntries: List<ProviderCatalogEntry>
  systemInputs: List<LockedSystemInput>
}
type TufRoleState { role: String version: Int expiresAt: DateTime metadataDigest: Digest }
type LogCheckpoint { logId: String treeSize: Int rootHash: Digest issuedAt: DateTime operatorSignatureDigest: Digest }
type MerkleInclusionProof { logId: String leafDigest: Digest leafIndex: Int treeSize: Int hashes: List<Digest> }
type MerkleConsistencyProof { logId: String fromSize: Int toSize: Int hashes: List<Digest> }
type WitnessReceipt { witnessId: String checkpointDigest: Digest signatureDigest: Digest }
type VerificationBundle {
  tufRootVersion: Int
  roles: List<TufRoleState>
  releaseStatuses: List<ReleaseStatus>
  checkpoint: LogCheckpoint
  inclusionProofs: List<MerkleInclusionProof>
  consistencyProof: MerkleConsistencyProof
  witnesses: List<WitnessReceipt>
  crossLogReceiptDigest: Digest
  catalogSnapshot: Digest
  catalogAsOf: DateTime
  timestampExpiresAt: DateTime
  policyDigest: Digest
}
union VerificationStatus {
  Current { verifiedAt: DateTime bundle: VerificationBundle }
  ReproducibleAsOf { catalogAsOf: DateTime checkpointDigest: Digest bundleDigest: Digest }
  Revoked { advisory: String }
  Incomplete { reason: String }
  Invalid { reason: String }
}
type LockedRoot {
  package: Option<PackageKey>
  relativePath: RelativePath
  sourceDigest: Digest
  manifestDigest: Digest
  buildPlanDigest: Digest
  dependencies: List<DependencyEdge>
  inferredCapabilities: List<String>
}
type DevelopmentRoot { package: Option<PackageKey> relativePath: RelativePath manifestDigest: Digest dependencies: List<DependencyEdge> }
type LockedReleasePackage {
  release: ReleaseId
  sourceDigest: Digest
  manifestDigest: Digest
  buildPlanDigest: Digest
  dependencies: List<DependencyEdge>
  authorizationSetDigest: Digest
  provenanceDigest: Digest
  sbomDigest: Digest
  inferredCapabilities: List<String>
}
union LockedDevelopmentPackage {
  Published { release: LockedReleasePackage }
  Local {
    package: PackageKey
    epoch: CompatibilityEpoch
    baseline: Option<ReleaseId>
    path: LocalPath
    manifestDigest: Digest
    dependencies: List<DependencyEdge>
  }
}
type DevelopmentBuildInput {
  package: Option<PackageKey>
  sourceDigest: Digest
  manifestDigest: Digest
  buildPlanDigest: Digest
  sbomDigest: Digest
  inferredCapabilities: List<String>
}
type DevelopmentBuildReceipt {
  builder: PrincipalId
  developmentLockDigest: Digest
  buildEnvironmentDigest: Digest
  inputs: List<DevelopmentBuildInput>
  outputDigest: Digest
  observedAt: DateTime
}
type SignedDevelopmentBuildReceipt { payload: DevelopmentBuildReceipt attestationDigest: Digest }
type ResolutionDecision {
  package: PackageKey
  selectedDigest: Digest
  reasonCodes: List<String>
  antecedents: List<PackageKey>
}
type ResolutionProof { policyDigest: Digest decisions: List<ResolutionDecision> proofDigest: Digest }
type DependencySelectionRoot { package: Option<PackageKey> relativePath: RelativePath sourceDigest: Digest manifestDigest: Digest dependencies: List<DependencyEdge> inferredCapabilities: List<String> }
type ReleaseDependencySelection { format: Int root: DependencySelectionRoot context: ResolutionContext packages: List<LockedReleasePackage> proof: ResolutionProof }
type ReleaseLockfile { format: Int root: LockedRoot context: ResolutionContext packages: List<LockedReleasePackage> proof: ResolutionProof }
type DevelopmentLockfile {
  format: Int
  root: DevelopmentRoot
  overlayDigest: Digest
  publishedBaseLock: Option<Digest>
  context: ResolutionContext
  packages: List<LockedDevelopmentPackage>
  proof: ResolutionProof
}
type DevelopmentStateGeneration {
  generationDigest: Digest
  manifestDigest: Digest
  releaseLockDigest: Option<Digest>
  overlay: DevelopmentOverlay
  lock: DevelopmentLockfile
  previousGeneration: Option<Digest>
}
type WorkspaceTransaction {
  previousManifest: Digest
  nextManifest: Digest
  previousReleaseLock: Option<Digest>
  nextReleaseLock: Option<Digest>
  previousGeneration: Option<Digest>
  nextGeneration: Digest
  journalDigest: Digest
}
union ScanVerdict {
  NoBlockingFinding { observedAt: DateTime }
  NeedsReview { findingIds: List<Uuid> }
  Blocked { findingIds: List<Uuid> }
}
type Evidence { kind: String source: String digest: Digest observedAt: DateTime expiresAt: Option<DateTime> }
type MaintenanceAssessment {
  score: Int
  confidence: Int
  adjustedScore: Int
  vulnerabilityResponse: Int
  vulnerabilityResponseConfidence: Int
  continuity: Int
  continuityConfidence: Int
  testEffectiveness: Int
  testEffectivenessConfidence: Int
  reviewAndStaticHygiene: Int
  reviewAndStaticHygieneConfidence: Int
  communityResponsiveness: Int
  communityResponsivenessConfidence: Int
  methodology: Digest
  evidence: List<Evidence>
}
type DiscoveryAssessment {
  release: ReleaseId
  policy: Digest
  catalogSnapshot: Digest
  evidenceSnapshot: Digest
  ledgerSnapshot: Digest
  cohort: String
  publisher: PrincipalId
  intentMatch: Int
  securityEvidence: Int
  adjustedMaintenance: Int
  compatibility: Int
  merit: Int
  debtRatio: Int
  exposureAdjustedMerit: Int
}
type ReleaseAssessment { release: ReleaseId verdict: ScanVerdict maintenance: MaintenanceAssessment attestations: List<Digest> }
```
