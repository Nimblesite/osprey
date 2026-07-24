# Plan 0020 — Package Registry and Manager

**Status:** specified, not implemented. The normative contracts are
[spec 0029](../specs/0029-PackageManagement.md) and
[spec 0030](../specs/0030-PackageRegistryTrustAndDiscovery.md), with canonical
types in [spec 0031](../specs/0031-PackageDomainModel.md) and score/ranking math
in [spec 0032](../specs/0032-PackageEvidenceAndDiscovery.md); the cited evidence
is [collected separately](../package-manager-research.md). This plan contains no
product decision gates: phases validate the fixed design rather than reopening it.

## Outcome

Ship a public, source-derived Osprey package registry with a small local command
surface, complete deterministic dependency resolution, end-to-end provenance,
non-executable installation, evidence-based maintenance scoring, and fair
discovery. Local development can replace any canonical package key globally with
a checkout and switch it back in one command. Release builds reject those
overlays, pin every input, and contain exactly one source for each package key.
The API, policy/domain layer, resolver, and WebAssembly frontend are written in
Osprey. Rust remains only the existing compiler/CLI bootstrap and a small native
host for cryptography, filesystem atomicity, process isolation, and HSM/forge
protocols.

The launch package format is Osprey source plus closed-format declared data. SQLite and
other native components are explicit system capabilities installed separately
through operating-system package managers. Osprey never downloads native
binaries, runs lifecycle hooks, or invokes those installers.

## Current repository seams

- `crates/osprey-project` already builds a deterministic mixed-flavor
  `ProjectGraph`. Its dependency-free manifest reader understands only
  `[project]` and `[modules]` and intentionally ignores `[package]`.
- `crates/osprey-cli` already dispatches project `build`, test, docs, formatter,
  native, and WebAssembly flows. It has no package commands, lock model, cache,
  repository client, or authentication.
- Spec 0025 supplies logical modules and imports; spec 0027 supplies native TAP
  tests and coverage. Mutation, property/fuzz orchestration, affected-client
  tests, and deterministic package build plans do not exist.
- Osprey has HTTP/WebSocket and WebAssembly targets, but there is no registry
  service, browser package UI, Supabase schema, CAS, TUF client, transparency
  log, provenance pipeline, sandbox fleet, or search index.
- The package system therefore starts with local identity/resolution tests and
  does not put a network facade in front of unfinished trust semantics.

## One-source architecture

The implementation is placed deliberately:

```text
schemas/package-registry.td       canonical typeDiagram model
registry/core/                    Osprey domain, policy, resolver, scoring
registry/api/                     Osprey REST application
registry/web/                     Osprey browser application -> WebAssembly
registry/supabase/migrations/     PostgreSQL schema and RLS policy
crates/osprey-package-host/       audited Rust host effects and CLI bridge
crates/osprey-project/            manifest/project integration
crates/osprey-cli/src/package/    thin package command presentation
```

`schemas/package-registry.td` is the source of generated Osprey domain types as
soon as typeDiagram's Osprey target lands. Handwritten copies are forbidden.
Until generation is available, implementation uses serialized conformance
fixtures directly and does not duplicate the model in service code.

`registry/core` owns every rule shared by CLI, API, and web: identities,
manifest semantics, catalog projection, solver clauses/objectives, score
methodology, and ranking policy. Host code implements effects but cannot choose
policy. API preview resolution and local lock resolution run the same compiled
core and policy digest.

## Milestone A — Local package engine

### Phase 0 — Schemas and adversarial fixtures

- [ ] Extract the typeDiagram block from spec 0031 into
      `schemas/package-registry.td` and make documentation import/render it.
- [ ] Add canonical JSON fixtures for manifests; proof-free publication payloads
      and detached ordered-proof envelopes; release IDs/statuses; typed TUF,
      inclusion/consistency and witness bundles; both locks/proofs; build receipts;
      evidence, attestations, advisories, and state transitions.
- [ ] Add malicious fixtures for path traversal, case collisions, symlinks,
      submodules, LFS pointers, devices, executable modes, Unicode confusables,
      oversized trees, decompression bombs, and digest substitution.
- [ ] Add bounded dependency graphs covering satisfiable, unsatisfiable,
      downgrade, epoch, target, capability, revocation, technical-lag, and
      cycle-rejection cases, each with the one expected lock/explanation.
- [ ] Record TUF, transparency, DSSE/in-toto, RFC 8785, SHA-256 agility, and
      compromised-mirror vectors from their primary specifications.
- [ ] Add golden vectors for exact UTC millisecond timestamps, every list sort/
      duplicate rule, detached PAE/WebAuthn bytes, every digest preimage/omission,
      and acyclic dependency-selection/build-plan/full-lock staging.
- [ ] Make schema/fixture validation a fail-fast `make test` stage.

Exit gate: every serialized form and security state transition has a reviewed,
byte-exact fixture before storage or API code exists.

### Phase 1 — Manifest and canonical identity

- [ ] Replace the narrow line parser in `osprey-project` with one typed TOML
      parser that preserves current `[project]`/`[modules]` behavior and adds
      `[package]`, bounded discovery metadata, all three dependency tables,
      `[[assets]]`, and `[system.*]` from `[PACKAGE-MANIFEST]`.
- [ ] Reject SemVer/version fields, upper ranges, peer/optional dependencies,
      forge dependencies, published path dependencies, and unknown security-
      relevant keys with actionable source spans.
- [ ] Validate scoped ASCII names, SPDX expressions, source roots, dependency
      epochs/minimum ordinals, and system-capability declarations.
- [ ] Implement the exact ASCII path grammar, entry encoding, root selection and
      domain-separated SourceDigest in `[PACKAGE-SOURCE-CANONICAL]`, plus
      manifest, build-plan, and ReleaseDigest generation.
- [ ] Generate a deterministic build plan from the project graph, toolchain,
      target, dependencies, and inferred type/effect/capability surface.
- [ ] Add cross-platform golden vectors proving identical digests on Linux,
      macOS, and Windows.

Exit gate: the same source project produces the same canonical identity on all
supported hosts, and every forbidden package input fails before hashing.

### Phase 2 — Release locks, local overlays, CAS, and atomic activation

- [ ] Implement RFC 8785 `osprey.lock` read/write and bind it to manifest,
      selected-root source, the complete typed TUF/status/log verification bundle,
      catalog snapshot/`asOf`, policy, complete
      compiler/runtime/toolchain/SDK/sysroot, target/CPU/flags, sandbox,
      normalized environment/time/randomness, exact system artifacts/native
      closure/provenance, and complete transitive graph.
- [ ] Canonically sort and deduplicate every set-like lock array, encode
      exact UTC millisecond times, non-negative integers and digests once, retain
      Merkle proof order, and store deterministic structured reason-code proofs
      instead of localized explanation strings.
- [ ] Implement `osprey.local.toml` as a package-key overlay and
      `osprey.local.lock` as its graph selection. Apply an override globally to
      every active edge; never retain a published candidate for the same key.
- [ ] Resolve root Runtime/Build/Test plus non-root Runtime/Build edges; exclude
      every non-root Test edge and apply one-key uniqueness to their union.
- [ ] Resolve each system capability through the threshold-signed provider
      catalog, selecting one catalog entry/input per capability and one digest per
      provider identity across the graph; publishers never choose host packages.
- [ ] CAS-stage manifest, optional release lock and immutable development state;
      commit one fsynced recovery journal and replay it before every read so all
      package mutations expose exactly old or new state. Ignore nested overlays.
- [ ] Permit live local source edits without resolution, rehash them into each
      development build's CAS snapshot, recheck contracts/effects/policy, emit a
      signed build receipt with actual derived digests, and invalidate only the
      local topology lock when a local manifest changes.
- [ ] Make release mode reject active overlays and external paths, verify every
      pinned digest, allow one pinned selected root plus registry ReleaseIds only,
      and reject duplicate package keys without an exception. Multi-root repos
      select the nearest ancestor manifest, keep state adjacent, emit one lock per
      root, and cannot release-depend on unpublished siblings.
- [ ] Add a per-user content-addressed cache partitioned by hash algorithm and
      digest; verify on every ingress and before materialization.
- [ ] Materialize package graphs read-only through staging plus atomic rename;
      never run package content during fetch, install, build-plan creation, or
      activation.
- [ ] Make bare `build`, `run`, `test`, and `docs` always select the live-root
      development lock without network or resolution; only `build --release`
      selects the release lock after an empty-overlay check.
- [ ] Require unexpired TUF state for resolve/fetch/activate; allow a cached
      verified graph to rebuild after expiry only with an explicit stale-
      freshness/non-activation report, and confine known revoked bytes to
      capability-denied `--audit-revoked` reproduction.
- [ ] Implement cache locks, concurrent fetch deduplication, interrupted-write
      recovery, garbage collection from lock roots, and corruption self-healing.
- [ ] Add `fetch`, `verify --offline`, `tree`, `why`, and `doctor` host effects
      with stable machine-readable output beneath concise terminal rendering.

Exit gate: a fetched release graph rebuilds offline byte-for-byte with one node
per key; local source can switch both directions without manifest edits; process
termination at every write boundary leaves either the old graph or the new one.

### Phase 3 — Complete Osprey resolver

- [ ] Implement candidate normalization and hard-clause generation in
      `registry/core` for closure, epoch/minimum ordinal, target/toolchain,
      capabilities, trust state, and release uniqueness.
- [ ] Implement deterministic incremental CDCL with watched literals, learned
      incompatibilities, restarts, clause minimization, and proof tracing.
- [ ] Implement lexicographic partial-MaxSAT optimization in the exact order in
      `[PACKAGE-RESOLUTION]`, including its integer cost tuple and canonical final
      graph vector; freeze every earlier component before optimizing the next.
- [ ] Derive minimal conflict cores and human explanations from learned
      incompatibilities; never synthesize explanations after the fact.
- [ ] Evaluate every time-dependent objective at the signed catalog `asOf`, not
      the host clock, and include it in deterministic inputs.
- [ ] Property-test soundness, bounded completeness, determinism, safe
      downgrade, global one-key/one-source uniqueness, overlay replacement, and
      objective ordering; differentially test against an independent exhaustive
      oracle on small graphs.
- [ ] Benchmark a representative 100,000-release catalog after metadata load:
      ordinary add/update p95 below two seconds and adversarial unsat below ten
      seconds on the CI reference machine. Performance work cannot weaken
      completeness or determinism.

Exit gate: no valid bounded graph is missed, invalid graph is emitted, or input
permutation changes a lock or explanation.

### Phase 4 — Simple transactional CLI

- [ ] Add `login`, `package init`, `add`, `remove`, `lock`, `update`, `use`,
      `fetch`, `tree`, `why`, `audit`, `verify`, `doctor`, `publish`, and `yank`
      as thin modules rather than enlarging `main.rs`.
- [ ] Make `add`, `remove`, `lock`, and `update` show dependency, capability,
      vulnerability, and epoch deltas before one atomic manifest/lock update.
- [ ] Make `add`, `remove`, and `update` atomically recompute the registry-only
      base, rebase matching per-key baselines, validate the overlay graph, and
      write release/`publishedBaseLock` only for a complete registry solution.
      Retain an override while its key remains transitively reachable.
- [ ] Make `add @scope/name` choose the current eligible epoch and
      `--epoch N` choose an older contract lineage; write exact digest pins only
      to the lock.
- [ ] Make `add <path>` infer the key, write the canonical dependency with
      a published baseline ordinal and exact per-override `ReleaseId` even when no
      full base exists, or floor zero/epoch 1 when unpublished (`--epoch N` selects
      another lineage); after publication, `lock` establishes its first baseline.
- [ ] Make ordinary `update` remain inside epochs and `update --breaking`
      evaluate newer epochs one by one with a generated migration report.
- [ ] While overrides are active, make `update` re-resolve their published base,
      atomically replace matching baselines, then revalidate locals; only
      `update --breaking` may change an override epoch, and failure changes none.
- [ ] Make `use <path>` infer the package key/current epoch/baseline, apply it
      globally, and resolve a local lock; make `use <key> --published` remove it
      and `use --list` expose replacements; `use --clear` restores all or none.
- [ ] Make `lock` release-only and `lock --local` development-only. Capture the
      exact pre-override lock/ReleaseIds; switching back restores that baseline,
      never an opportunistic update, and preserves the overlay on revocation.
- [ ] Make `build --release` ignore no error: any override, duplicate key,
      unpinned source/system/toolchain input, mismatch, or fallback is fatal.
- [ ] Snapshot-test terse success, offline, auth, quarantine, revocation,
      conflict-core, missing-system-capability, and recovery diagnostics.
- [ ] Expose stable JSON for editor/automation integration without creating a
      second semantic API.

Exit gate: init, add, switch local/published, build exact release offline,
explain, audit, update, and recover fit the command surface and never silently
mutate compatibility epochs or admit two sources for a package key.

## Milestone B — Verifiable public registry

### Phase 5 — Client trust plane

- [ ] Embed and out-of-band fingerprint the initial 3-of-5 TUF root; implement
      sequential old-and-new-threshold root updates, 2-of-3 release-bearing
      roles, consistent snapshots, expiry, and persisted version floors.
- [ ] Implement the 2-of-3 `release-status` role and monotonic typed
      `Eligible`/`Yanked`/`Revoked` record chain; require one fresh exact record,
      inclusion proof, and stored-checkpoint consistency proof per locked release.
- [ ] Verify distinct OIDC ephemeral-key and WebAuthn publisher proofs,
      DSSE/in-toto attestations, SLSA provenance, Merkle inclusion/consistency,
      3-of-4 witness quorum, gossip, and external cross-log receipts locally.
- [ ] Implement `Eligible`, `Yanked`, and `Revoked` client behavior exactly;
      cached locks survive yanks and known revocations block activation.
- [ ] Fetch source from interchangeable mirrors, distrust transport/storage,
      and accept bytes only after metadata, proof, closure, and digest checks.
- [ ] Run compromise simulations for every TUF role, publisher, forge, mirror,
      API, database, builder, scanner, log, witness, and local cache.

Exit gate: no single compromised online component can cause acceptance of
unauthorized, rolled-back, expired, targeted, substituted, or revoked source;
acceptance of still-valid frozen state is bounded by its signed expiry.

### Phase 6 — Supabase data plane and Osprey API

- [ ] Create append-only identifiers plus mutable projections for principals,
      scopes, packages, release intents/releases, dependency edges,
      provider catalog/status records, assessments/evidence, advisories, appeals,
      ownership, and exposure ledgers.
- [ ] Store CAS replicas at digest-derived keys with create-only service policy;
      keep TUF/log authority and all signing material outside Supabase.
- [ ] Configure Supabase Auth for passkeys/OIDC, short sessions, MFA for
      privileged actions, RLS defense in depth, and no browser service role.
- [ ] Build the Osprey `/v1` REST API with cursor pagination, digest ETags,
      idempotent writes, `application/problem+json`, rate limits, and audit IDs.
- [ ] Ensure the browser uses only the API and every authorization decision is
      repeated server-side against stable principal/scope IDs.
- [ ] Prove the catalog and immutable objects can be reconstructed from CAS,
      TUF, and the transparency log after deleting the Supabase project.

Exit gate: the API passes contract/security tests and a total Supabase loss does
not invalidate identity, trust history, or recoverability.

### Phase 7 — Source collection and publication orchestration

- [ ] Bind package/subdirectory to a stable forge repository and release branch
      with scope plus forge-admin proof; log rebinding and quarantine it 24 hours.
- [ ] Implement proof-free RFC 8785 payloads plus detached ordered-proof envelopes
      for binding, intent, one-use challenge and final authorization, using exact
      DSSE/WebAuthn bytes and threshold principal sets from spec 0030.
- [ ] Bind the intent to the publisher-computed canonical SourceDigest and forge
      hash algorithm; recompute commit identity, reachability, manifest, and
      source digests before admitting the snapshot to CAS.
- [ ] Require the CLI to recompute and compare every challenge field, including
      source/manifest/context/dependency-lock/build-plan/release digests,
      compatibility, identity, ordinal and nonces, before final signing.
- [ ] Implement the GitHub App adapter using stable repository IDs, exact
      commits, configured release-branch reachability, and least privilege;
      implement the same adapter contract for GitLab and Codeberg fixtures.
- [ ] Fetch source server-side, validate/canonicalize it, archive it in CAS, and
      reject every upload, archive substitution, native payload, hook, generator,
      or undeclared asset before work enters the build queue.
- [ ] Allocate ordinal/epoch under a serializable package lock, enforce
      compatibility claims, and map each release digest permanently to one
      epoch/ordinal. Expired challenges retry that reserved identity; no digest or
      later submission can reuse it.
- [ ] Implement quarantine, independent appeal, publication, yank, revocation,
      transfer, recovery, and tombstone workflows as logged state machines.
- [ ] Produce and publish TUF metadata only after evidence, reproducibility, and
      policy gates complete.

Exit gate: a source commit can become exactly one immutable release, while no
publisher-supplied artifact can reach the catalog.

### Phase 8 — Hermetic build and deterministic assessment

- [ ] Provision two separately administered SLSA Build L3 platforms with
      ephemeral workers, independent images/control planes, declared CAS inputs,
      resource ceilings, isolated caches, and protected attestation keys.
- [ ] Compile/type/effect check, run registry tests, infer capabilities, compare
      public contract/ABI/behavior claims, analyze affected clients, and require
      bit-identical reference outputs from both pools.
- [ ] Generate canonical Osprey-profile SPDX 3.0.1 SBOMs and in-toto/DSSE
      attestations with separate subjects for source assembly and each reference
      output; test byte equality independently of SLSA conformance.
- [ ] Integrate OSV-style advisories, reachability, secrets, licenses, SAST,
      taint, dependency intelligence, and sandboxed dynamic behavior analysis.
- [ ] Add Osprey branch coverage, mutation operators, property/fuzz runner, and
      client-impact testing with disclosed denominators, exclusions, and limits.
- [ ] Fail publication on confirmed malware, secrets, provenance failure,
      non-reproducibility, forbidden capabilities, or reachable critical/high
      vulnerabilities; send uncertain findings to quarantine and appeal.

Exit gate: every published release has reproducible source/build evidence and a
complete signed assessment without claiming that a clean result means safe.

### Phase 9 — AI review under measured authority

- [ ] Build complete signed temporal receipt frames and a finite generated
      adversarial frame with no repository/author/family/near-duplicate leakage;
      draw each one-use sample through the pre-window independent evaluator VRF.
- [ ] Add whole-source and semantic-diff review after deterministic analysis;
      treat package text as prompt injection and expose no tools, network,
      credentials, mutable memory, or production actions to the model.
- [ ] Deterministically chunk every raw token and AST node, add cross-file graph
      context, and sign a complete byte/token/AST coverage ledger; any omission,
      truncation, timeout, or model error remains quarantined.
- [ ] Permit automatic authority only for content-addressed weights, runtime,
      deterministic kernels/hardware profile, tokenizer, decoding/RNG/retry rules,
      prompt, tools, chunker, aggregation, threshold and policy; mutable APIs stay
      advisory, and every behavior change starts unqualified.
- [ ] Enforce the simultaneous exact package-level recall, false-positive,
      prevalence-derived precision and adversarial-recall finite-population
      hypergeometric bounds with logged alpha spending in
      `[PACKAGE-ASSESSMENT]`; report exact worst-interval ECE as diagnostic only.
- [ ] Shadow each candidate model and report PR-AUC, recall at FPR, false
      positives per million, Brier score, abstention, latency, and cost at the
      observed class prior.
- [ ] Ensure no AI-only pass, rejection, publication, yank, or revocation path
      exists and appeal reviewers do not receive the original model verdict.

Exit gate: the AI path can improve triage but cannot weaken deterministic gates
or exercise authority unsupported by measured evidence.

## Milestone C — Quality and fair discovery

### Phase 10 — Maintenance evidence and scoring

- [ ] Implement the fixed five-component, 24-month methodology and neutral
      confidence prior from spec 0032 in integer-only `registry/core`; version
      and sign every calculation.
- [ ] Measure median/p90 vulnerability response and propagation, reachable
      overdue risk, recurrence, continuity/handover, release reliability,
      tests/mutation/fuzz/client impact, normalized static findings, and response
      quality from immutable evidence.
- [ ] Treat missing data as `Unknown`; exclude raw vulnerabilities, commits,
      issues, stars, downloads, dependents, fame, age, and activity volume.
- [ ] Render score, confidence, component vector, raw evidence, timestamps,
      sample sizes, tools/rules, exclusions, and method changes on every page.
- [ ] Add counterfactual tests proving disclosure, a new package, a small
      community, or absence of popularity cannot mechanically lower rank.
- [ ] Differentially replay every normalizer, percentile, confidence and score
      against a second implementation and the signed basis-point golden vectors.

Exit gate: every score is independently reproducible and no aggregate conceals
which evidence changed it.

### Phase 11 — Fair search and Osprey WebAssembly UI

- [ ] Implement eligibility, the fixed merit formula, query-intent cohorts,
      28-day exposure debt, eight-merit/two-exploration allocation, 80% intent
      floor, and two-results-per-publisher cap in shared Osprey core.
- [ ] Enforce the signed 512-byte summary/12-keyword grammar and exact bounded
      key/summary/keyword/compiler-API lexical and semantic serialization; never
      index arbitrary package documentation as ranking text.
- [ ] Implement publisher-normalized entitlement, exact apportionment/ties, and
      auditable client-nonce VRF exploration with deterministic/degraded modes.
- [ ] Exclude all popularity/commercial features from default ranking and model
      training; expose `Most used` only as a separately selected sort.
- [ ] Collect randomized propensity-logged exploration events without raw IPs
      or retained raw queries and expire aggregate cohort ledgers after 28 days.
- [ ] Gate ranker releases on NDCG, coverage@10, new-package exposure, Gini,
      publisher share, exposure/merit, exploration conversion, and utility loss.
- [ ] Build the Osprey-to-WASM web UI for search, source, dependencies,
      effects/capabilities, evidence, proofs, advisories, release history,
      publishing, transfers, quarantine, and appeals.
- [ ] Meet keyboard, screen-reader, responsive, no-JavaScript evidence-download,
      and clear `No blocking finding` language requirements.

Exit gate: a relevant vetted newcomer receives controlled exposure without
lowering security eligibility or allowing the ranker to learn its own bias.

## Milestone D — Operational launch

### Phase 12 — Hardening and general availability

- [ ] Operate independent CAS replicas, log monitors, four external witnesses
      with 3-of-4 quorum, cross-logging, 3-of-5 offline root custody, 2-of-3
      release-role HSMs, documented ceremonies, and compromise runbooks.
- [ ] Complete SLSA Build L3 for all packages and Source L4 for privileged
      packages; publish evidence only after independent verification.
- [ ] Run disaster recovery without Supabase, mirror/CDN, either independent
      build platform, one witness, and each online signing role in turn.
- [ ] Fuzz every parser/protocol boundary and pass all conformance suites from
      specs 0029-0032 on Linux, macOS, and Windows.
- [ ] Commission an independent security/cryptography/privacy/fairness audit and
      close every critical/high finding; publish the report and remediations.
- [ ] Launch with scoped source packages only. Do not add binary publication,
      lifecycle hooks, SemVer selectors, direct database clients, or pay-to-rank
      as post-audit shortcuts.
- [ ] Run `make ci` and the registry deployment rehearsal from an empty account
      to published package, fresh-client add, offline rebuild, update, yank,
      revoke, restore, and appeal.

Exit gate: every `[PACKAGE-CONFORMANCE]` and
`[PACKAGE-REGISTRY-CONFORMANCE]` requirement passes with retained evidence.

## Fixed risk controls

| Risk | Required control |
| --- | --- |
| Package manager bootstraps itself | Registry core is an ordinary checked-in Osprey project built by the existing compiler; its released source later enters the same pipeline. |
| Solver complexity or slowness | Complete CDCL/MaxSAT remains mandatory; indexing, incremental reuse, and objective encodings optimize it. There is no greedy mode. |
| Supabase compromise or lock-in | Clients trust TUF/log/digests; all authoritative objects are reconstructable and storage/API are replaceable. |
| AI false positives or evasion | Real-prior gates, abstention, drift demotion, quarantine, independent appeal, and no AI-only authority. |
| New-package cold start | Neutral maintenance prior plus security-gated exploration and exposure debt, never fabricated quality history. |
| Vulnerability disclosure bias | Raw vulnerability count has zero weight; unresolved reachable risk and response/propagation speed are scored. |
| Native ecosystem pressure | System capabilities and explicit provider guidance; no native code or installer execution enters the registry. |
| Local override leaks into release | Separate development overlay/lock, prominent diagnostics, release-mode rejection, exact workspace digests, and one-key/one-source validation. |
| Forge/tag mutation | Dual-authority repository binding, publisher-computed digest, frozen CAS challenge, final signed authorization, and permanent log. |
| Key compromise | 3-of-5 offline root, 2-of-3 release roles, independent custody/control planes, witnessed rotation, and client version floors. |

## Definition of done

The work is complete only when an unknown new publisher can publish a small
Osprey library from a reviewed forge commit with one command; a fresh client can
resolve it deterministically, verify every trust edge, install without executing
package code, switch a transitive dependency to a local checkout and back without
editing its manifest, reproduce the exact one-version-per-key release build
offline, understand every conflict and score, and find it through security-gated
fair exploration. The same exercise must survive a malicious mirror, compromised
database, revoked release, expired online key, failed builder, and full Supabase
loss without accepting different bytes or silently weakening policy.
