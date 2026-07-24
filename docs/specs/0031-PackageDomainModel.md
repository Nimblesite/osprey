# Package Domain Model

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
  dependencies: List<DependencyEdge>
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

## Model invariants `[PACKAGE-TYPE-INVARIANTS]`

- Every signed payload is proof-free RFC 8785 JSON under its exact media type.
  Its envelope contains detached proofs sorted by principal; principals are
  unique and each proof is exactly one publisher, forge-admin, or registry-role
  mechanism allowed for that envelope by `[PACKAGE-PUBLISH]`.
  `authorizationSetDigest` is the digest of the complete ordered
  `SignedPublicationAuthorization`, never a single proof.
- A release cannot enter `Published` without a non-expired, one-use signed final
  authorization over the signed challenge. That challenge binds the exact
  `ReleaseId`, frozen CAS, source, manifest, resolution context, dependency lock,
  build plan, release digest, compatibility claim, both nonces, and signed intent.
  The intent source equals its active logged signed binding; `@osprey` requires
  two of three distinct enrolled principals.
- A repository binding requires one authorized scope-publisher proof and the
  forge-admin attestation for the same stable repository; both bind its one-use
  nonce and enrollment expiry. A challenge has only the pinned registry-role
  proof. Intent and final-authorization envelopes have publisher proofs only.
- `Utf8Text`, `Json`, and `Png` have exactly the extensions, media types, parsers,
  and limits in `[PACKAGE-PUBLISH]`; byte lengths are non-negative and match the
  digest input. Unknown asset roles/kinds are schema errors. Discovery metadata
  satisfies the byte, grammar, count, and uniqueness limits in spec 0032.
- For canonical value `x`, `H(d,x) = "sha256:" || lowercaseHex(SHA256(ASCII(d) || 0x00 || u64be(length(RFC8785(x))) || RFC8785(x)))`.
  Manifest,
  build-plan, and release digests are respectively `H("osprey.manifest.v1", manifest)`,
  `H("osprey.build-plan.v1", plan)`, and
  `H("osprey.release.v1", ReleaseIdentityEnvelope)`. `ReleaseId.digest` equals
  that release digest; no ordinal, proof, signature, or mutable state enters it.
- `ResolutionContext.verificationBundleDigest` equals
  `H("osprey.verification-bundle.v1", bundle)`. Its catalog snapshot/`asOf` and
  solver policy equal the bundle's like-named fields; its root version/digest and
  snapshot, timestamp, and release-status digests equal the bundle's unique
  `root`, `snapshot`, `timestamp`, and `release-status` role records. Bundle root
  version equals its root-role version; `timestampExpiresAt` equals the timestamp
  role expiry; `logCheckpointDigest` equals
  `H("osprey.log-checkpoint-record.v1", bundle.checkpoint)`. The proof policy
  equals the context solver policy. Contexts hash with `osprey.resolution-context.v1`.
- A release build plan binds `H("osprey.dependency-selection.v1", selection)`;
  only afterward does its completed witness lock hash as
  `H("osprey.release-lock.v1", lock)`, the challenge's `dependencyLockDigest`.
  A development plan instead binds
  `H("osprey.development-lock.v1", lock)`. Thus no lock/plan digest is cyclic.
- Build-plan component preimages are the typed projections above. Their `H`
  domains are, in field order, `osprey.module-graph.v1`,
  `osprey.compiler-invocation.v1`, `osprey.test-plan.v1`, and
  `osprey.output-layout.v1`; public interfaces and source bytes use
  `osprey.public-interface.v1` and `osprey.source-file.v1`. Module/import/test/
  artifact lists sort by their displayed path/namespace/symbol tuple; compiler
  flags retain semantic order. Unknown formats, fields, or kinds are rejected.
- `PackageKey` is globally unique in each lock and development overlay. Release
  packages derive that key from `ReleaseId`; development packages derive it from
  `LockedDevelopmentPackage`. Every activated dependency edge refers to exactly
  one indexed node; root witnesses activate Runtime/Build/Test edges, while
  non-root nodes activate Runtime/Build edges and never their Test edges.
- A release lock has one selected, digest-pinned root. Every non-root package is
  a `LockedReleasePackage`, so a local dependency is unrepresentable. A
  repository with several roots emits one lock per root; roots cannot depend on
  unpublished sibling roots. Each package's source/authorization-set digests equal
  the immutable registry release addressed by its `ReleaseId`.
- A release lock pins compiler, runtime, full toolchain/SDK/sysroot, target and
  CPU features, flags, sandbox image/policy, kernel ABI, locale, timezone,
  filesystem ordering, environment/time/randomness policy, complete system-
  provider closures/provenance, TUF/log state, catalog, root, and every package
  input. Exactly one signed provider-catalog entry and one `LockedSystemInput`
  exist per canonical capability; one native provider identity has one digest
  across the graph. The builder exposes no host input absent from these closures.
- A development lock records graph selection while local source remains live.
  It pins topology and manifests, not mutable local source or derived artifacts.
  Every development build snapshots local trees to CAS, rechecks contracts,
  effects and policy, and emits a `SignedDevelopmentBuildReceipt`. Its detached
  DSSE/in-toto attestation names the builder and has
  `H("osprey.development-build-receipt.v1", payload)` as its only subject; the
  proof-free payload therefore cannot hash its own `attestationDigest`.
- `overlayDigest` equals the canonical active overlay digest. Overlay entries and
  local locked nodes are a bijection with identical package, canonical path,
  epoch, and baseline. A baseline has the same key/epoch and identifies the exact
  captured published node; no published node may coexist for that key.
- A `WorkspaceTransaction` CAS-stages and fsyncs the manifest, optional release
  lock and development generation before creating its pending journal as commit.
  The generation binds those exact manifest/release-lock digests. Every command
  replays the journal idempotently before reading, so only old or new is seen.
- `overlayDigest`, `proofDigest`, `generationDigest`, and `journalDigest` use `H`
  domains `osprey.development-overlay.v1`, `osprey.resolution-proof.v1`,
  `osprey.development-generation.v1`, and `osprey.workspace-transaction.v1`,
  omitting their own field. Operator/witness signatures cover `H` domains
  `osprey.log-checkpoint.v1`/`osprey.witness-receipt.v1` with signature omitted.
- Every `DateTime` is valid Gregorian UTC in exactly
  `YYYY-MM-DDTHH:mm:ss.SSSZ`, years 0001-9999, with no leap second. Integers are
  canonical decimal; digests use lowercase algorithm-prefixed hex. String order
  is unsigned lexicographic order of the field-valid UTF-8 bytes, without locale.
- Package keys order by `(scope,name)` and releases by
  `(package,epoch,ordinal,digest)`. Dependencies order by variant
  Runtime/Build/Test then `(package,epoch,minimumOrdinal)`; assets by
  `(path,digest)`; system requirements by `(capability,minimumRevision)`;
  provider entries by `(capability,targetSpecDigest,provider,revision,digest)`;
  and locked system inputs by `(capability,provider,revision,digest)`.
- Overrides and package nodes order by package key; build inputs use absent key
  before present key, then package key; decisions order by package key. Proofs
  order by `(principal,proof-variant,credential/attestation/signature digest)`. TUF roles
  order by `(role,version)`, statuses by release, inclusion proofs by
  `(logId,leafIndex)`, and witness receipts by `witnessId`.
- Evidence orders by `(kind,source,digest,observedAt)`. Keyword, capability,
  reason-code, UUID, attestation, antecedent, and closure-digest lists use their
  corresponding scalar/package order. Every such set-like list is duplicate-free.
  Merkle proof `hashes` alone retain protocol position and may repeat. The
  structured proof uses stable reason codes; `proofDigest` hashes the proof with
  that field omitted, and localized prose never enters lock data.
- The canonical manifest preserves `[project]` name/source roots, optional
  default namespace/entry/flavor (`"default"` or `"ml"`), and `[modules]`
  wildcard policy. `[package]` is optional for application roots; when present
  it requires name, license, summary, and keywords and becomes `publication`.
  Published releases require it and require `BuildPlan.package` to equal its key.
  Absent `test_roots` means `[]`; source/test roots are unique.
  `[dependencies]`, `[build-dependencies]`, and `[test-dependencies]` map to
  Runtime, Build, and Test. There is at most one edge per `(kind,package)` and one
  system requirement per capability; all edges for a key name one epoch and the
  effective floor is their maximum. Each `[[assets]]` table has exactly `path`,
  `role`, `kind`, `byte_length`, `digest`, and only for runtime data
  `required_capability`. Role strings are `documentation`/`runtime-data`; kind
  strings are `text`/`json`/`png`, with a text format derived from its extension.
- `RelativePath` is root-confined, normalized, and rejects absolute paths,
  traversal and symlinks. `LocalPath` is development-only: `use` resolves it
  once against the selected workspace root to a canonical physical path,
  rejects cycles, and verifies the package key on every build.
- `ReleaseState.Published` means all publication gates passed at its recorded
  time. It does not mean safe, vulnerability-free, or permanently eligible.
- A status sequence starts at one: `previousStatus` and `reasonDigest` are absent
  only for initial `Eligible`; later `previousStatus` equals
  `H("osprey.release-status.v1", precedingRecord)` and carries the exact reason/
  advisory digest. Only the transitions in spec 0030 are valid.
- A bundle has one typed, fresh release-status record and inclusion proof for
  every selected release, exact unique TUF role paths/versions/expiries, and a
  consistency proof from the client's stored checkpoint. Witness IDs are
  distinct and bind the identical checkpoint. `Current` requires three of four
  receipts, a cross-log receipt, unexpired metadata, current policy, and online
  verification. Its local `verifiedAt` is excluded from the bundle/lock digest;
  offline success is `ReproducibleAsOf`, and revocation knowledge never rolls back.
- Scores are derived views over `Evidence`; a score cannot replace or mutate an
  evidence record, release state, lock, or trust attestation. Scores/confidences
  are integer basis points in 0-10,000; only `debtRatio` may be negative, in
  -10,000..10,000. All arithmetic and display follow spec 0032 exactly.
