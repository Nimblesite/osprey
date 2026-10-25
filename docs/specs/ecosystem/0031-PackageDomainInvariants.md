# Package Domain Invariants

Normative constraints for the [canonical domain model](0031-PackageDomainModel.md). The canonical typeDiagram describes shapes; these requirements constrain identity, authority, serialization and transitions. Organization-only ownership and the single administrator contract are defined in [spec 0045](0045-OrganizationPackageGovernance.md).

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
- JSON encoding of the model is fixed: a `type` is an object whose members are
  its fields; an alias is its underlying scalar; an `Option<T>` field is omitted
  when absent and is never `null`; a `union` value is an object with exactly one
  member, named by the variant, whose value is the variant's payload object
  (`{}` for a payload-free variant such as `{"Eligible":{}}`); `Bool` is a JSON
  boolean, `Int` a canonical decimal integer, and `DateTime`, `Digest`, `Uuid`
  and every string alias a JSON string. An object with zero or several variant
  members, an unknown variant, a `null`, or a non-integer number is rejected.
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
  `[project].python_sources` lists the Osprey Python-flavor files of the
  project as root-confined `RelativePath`s ending in `.py`; it is part of the
  canonical manifest (`pythonSources`). A manifest that declares `[package]`
  must keep every listed path inside one of its `sourceRoots`
  (`manifest.python-source-unrooted`), and every `.py` file in a release tree
  begins with the line `# osprey: flavor=python` or the release is refused
  (`source.python-flavor-header`): published packages contain Osprey source
  only, and a Python-flavor file is Osprey source that the Osprey compiler
  reads, never a CPython module ([PACKAGE-PUBLISH]).
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
