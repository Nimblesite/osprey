# Plan 0020 — Package Registry and Manager

Contract: [0029 Package Management](../specs/ecosystem/0029-PackageManagement.md), [0030 Trust and Discovery](../specs/ecosystem/0030-PackageRegistryTrustAndDiscovery.md), [0031 Domain Model](../specs/ecosystem/0031-PackageDomainModel.md), [0031 Domain Invariants](../specs/ecosystem/0031-PackageDomainInvariants.md), [0032 Evidence and Fair Discovery](../specs/ecosystem/0032-PackageEvidenceAndDiscovery.md), [0043 Python-Flavor Ports](../specs/ecosystem/0043-PythonFlavorPorts.md), [0044 Deployment and Ownership](../specs/ecosystem/0044-PackageRegistryDeploymentAndOwnership.md), [0045 Organization Governance](../specs/ecosystem/0045-OrganizationPackageGovernance.md)

Depends on: hosted Stages B–G on parent Milestone B (Phases 5–9) and Milestone C (Phases 7–10) completion; Stage D on Phases 5 and 7–10.

## Prerequisites in place

- Folder registry, verified fetch/install, deterministic CDCL/MaxSAT resolver, folder lock, local overlays, `publish --folder` and the uv Python plane run today (`src/osprey/packages/manager/{api,cli,core,host,manifest,resolver}/`), exercised by `make -C src/osprey/packages registry-test` and CI (`make test`, `.github/workflows/ci.yml` `test-registry-gc`/`test-registry-arc`, both required by the coverage gate).
- `src/toolchain/schemas/package-registry.td` is the canonical domain model; `src/osprey/packages/manager/supabase/migrations/schema.yaml` defines the eleven ownership tables (bootstrap FORCE-RLS, no allow policies/runtime grants).
- Blocked host capabilities are recorded in `src/osprey/packages/manager/core/src/capabilities.osp` (argv, exit status, environment, binary-safe reads, directory/stat/rename/fsync, UTC clock); `make -C src/osprey/packages registry-blockers` probes them.
- Blocker: DataProviderMigrate 0.9.11-beta applies the `registry` schema but its verifier hardcodes `public` (`.config/dotnet-tools.json`; `PostgresPolicyCatalogNormalizer.Inspect`); hosted migration convergence stays blocked until fixed.
- Blocker: the `schema-check: long whitespace and identifiers` stress case SIGBUSes; the exact case is retained as a failing regression.
- `login`, `audit`, `yank` are answered not-implemented by `cli/lib/dispatch.osp`; authenticated `publish` is the trust plane's.
- `registry::Arithmetic` (`core/src/arithmetic.osp`) holds the canonical integer primitives; the scoring methodology itself is not started.

## Milestone 0 — Stand it up anywhere

- [ ] Bind the server to a non-loopback address and serve a second machine on the same network; record the measured result. `0.0.0.0` binding from an automated session is refused by the permission policy, so this stays a manual step. `[PACKAGE-FOLDER-REGISTRY]`

## Milestone A — Local package engine

### Phase 1 — Canonical build plan and release artifacts

- [ ] Generate the canonical deterministic `BuildPlan` (project graph, toolchain, target, dependencies, inferred type/effect/capability surface) and `ReleaseDigest`/manifest/build-plan digest generation with `H(d,x)` domains `osprey.build-plan.v1`/`osprey.release.v1`; the derived compiler manifest in `cli/lib/build.osp` does not close this. `[PACKAGE-SOURCE-CANONICAL]` `[PACKAGE-TYPE-INVARIANTS]`
- [ ] Add cross-platform golden vectors proving identical `SourceDigest` and lock bytes on Linux, macOS, and Windows. `[PACKAGE-SOURCE-CANONICAL]` `[PACKAGE-CONFORMANCE]`

### Phase 2 — Release lock, provider catalog, CAS and atomic activation

- [ ] Implement RFC 8785 `osprey.lock` read/write as the Milestone B `ReleaseLockfile`/`DevelopmentLockfile`: bind manifest, selected-root source, complete typed TUF/status/log verification bundle, catalog snapshot/`asOf`, policy, complete src/toolchain/compiler/runtime/toolchain/SDK/sysroot, target/CPU/flags, sandbox, normalized environment/time/randomness, exact system artifacts/native closure/provenance, and the complete transitive graph. `[PACKAGE-TYPES]`
- [ ] Canonically sort and deduplicate every set-like lock array; encode exact UTC millisecond times, non-negative integers and digests once; retain Merkle proof order; store deterministic structured reason-code proofs instead of localized explanation strings. `[PACKAGE-TYPE-INVARIANTS]` Done so far: the list order and duplicate rules (`manager/core/src/set_order.osp`, 104 fixture rows in `manager/tests/src/order_tests.osp`); the time and digest encodings and the structured reason-code proofs are still unverified.
- [ ] Resolve each system capability through the threshold-signed provider catalog: one catalog entry/input per capability and one digest per provider identity across the graph; publishers never choose host packages. `[PACKAGE-MANIFEST]`
- [ ] CAS-stage the manifest, optional release lock and immutable development state; commit one fsynced recovery journal and replay it before every read so all package mutations expose exactly old or new state; ignore nested overlays. `[PACKAGE-TYPE-INVARIANTS]`
- [ ] Permit live local source edits without resolution; rehash them into each development build's CAS snapshot; recheck contracts/effects/policy; emit a signed `SignedDevelopmentBuildReceipt` with actual derived digests; invalidate only the local topology lock when a local manifest changes. `[PACKAGE-LOCAL-OVERRIDE]`
- [ ] Release mode: nearest-ancestor root selection and multi-root repositories (one lock per root, no release-dependency on unpublished siblings). `build --release` already rejects overrides, verifies pinned digests and rejects duplicate keys. `[PACKAGE-LOCAL-OVERRIDE]`
- [ ] Add a per-user content-addressed cache partitioned by hash algorithm and digest; verify on every ingress and before materialization. `[PACKAGE-THREATS]`
- [ ] Materialize package graphs read-only through staging plus atomic rename; never run package content during fetch, install, build-plan creation or activation. `[PACKAGE-THREATS]`
- [ ] Make bare `build`, `run`, `test`, and `docs` select the live-root development lock without network or resolution; only `build --release` selects the release lock. `build` is done; `run`/`test`/`docs` are not package commands yet. `[PACKAGE-LOCAL-OVERRIDE]`
- [ ] Require unexpired TUF state for resolve/fetch/activate; allow a cached verified graph to rebuild after expiry only with an explicit stale-freshness/non-activation report; confine known revoked bytes to capability-denied `--audit-revoked` reproduction. `[PACKAGE-TRUST]`
- [ ] Implement cache locks, concurrent fetch deduplication, interrupted-write recovery, garbage collection from lock roots, and corruption self-healing. `[PACKAGE-THREATS]`
- [ ] Close the type-level confinement gap: refuse an ambient builtin in a dependency source root unless the root grants its capability (compiler request recorded in `[PACKAGE-CAPABILITY-CONFINEMENT]`); until then the scan is the gate and the lock is the evidence. `[PACKAGE-CAPABILITY-CONFINEMENT]`

### Phase 3 — Resolver completion

- [ ] Evaluate every time-dependent objective at the signed catalog `asOf`, not the host clock, and include it in deterministic inputs. `Objective::overdueDays` already takes milliseconds as input; the `asOf` arrives with the Milestone B signed catalog. `[PACKAGE-RESOLUTION]`
- [ ] Benchmark a representative 100,000-release catalog after metadata load: ordinary add/update p95 below 2 s and adversarial unsat below 10 s on the CI reference machine; performance work must not weaken completeness or determinism. `[PACKAGE-RESOLUTION]`

### Phase 4 — Command surface completion

- [ ] Implement `login`, `audit`, `yank`, and the authenticated `publish`; `Dispatch` currently returns the not-implemented status for `login`/`audit`/`yank`. `[PACKAGE-API]`
- [ ] Make `add`, `remove`, `lock`, and `update` show capability and vulnerability deltas before one atomic manifest/lock update (dependency and epoch deltas are done); crash-safe atomicity remains blocked on filesystem rename/fsync. `[PACKAGE-API]`
- [ ] Make `add`, `remove`, and `update` rebase matching per-key baselines, validate the overlay graph, and write release/`publishedBaseLock` only for a complete registry solution (registry-only recompute and complete-solution-only write are done). `[PACKAGE-LOCAL-OVERRIDE]`
- [ ] Snapshot-test the remaining diagnostics: auth, quarantine, revocation, missing-system-capability (terse success, offline, conflict, capability and recovery lines are already asserted). `[PACKAGE-API]`

## Milestone B — Verifiable public registry

### Phase 5 — Client trust plane

- [ ] Embed and out-of-band fingerprint the initial 3-of-5 TUF root; implement sequential old-and-new-threshold root updates, 2-of-3 release-bearing roles, consistent snapshots, expiry, and persisted version floors. `[PACKAGE-TRUST]`
- [ ] Implement the 2-of-3 `release-status` role and the monotonic typed `Eligible`/`Yanked`/`Revoked` record chain; require one fresh exact record, inclusion proof, and stored-checkpoint consistency proof per locked release. `[PACKAGE-TRUST]`
- [ ] Verify distinct OIDC ephemeral-key and WebAuthn publisher proofs, DSSE/in-toto attestations, SLSA provenance, Merkle inclusion/consistency, 3-of-4 witness quorum, gossip, and external cross-log receipts locally. `[PACKAGE-TRUST]`
- [ ] Implement `Eligible`, `Yanked`, and `Revoked` client behavior exactly; cached locks survive yanks and known revocations block activation. `[PACKAGE-TRUST]`
- [ ] Fetch source from interchangeable mirrors, distrust transport/storage, and accept bytes only after metadata, proof, closure, and digest checks. `[PACKAGE-TRUST]`
- [ ] Run compromise simulations for every TUF role, publisher, forge, mirror, API, database, builder, scanner, log, witness, and local cache. `[PACKAGE-REGISTRY-CONFORMANCE]`

### Phase 6 — Hosted Supabase data plane behind the same Osprey API

- [ ] Create append-only identifiers plus mutable projections for principals, scopes, packages, release intents/releases, dependency edges, provider catalog/status records, assessments/evidence, advisories, appeals, ownership, and exposure ledgers. `[PACKAGE-OWNERSHIP-DATABASE]`
- [ ] Store CAS replicas at digest-derived keys with create-only service policy; keep TUF/log authority and all signing material outside Supabase. `[PACKAGE-HOSTED-PUBLISH]`
- [ ] Configure Supabase Auth for passkeys/OIDC, short sessions, MFA for privileged actions, RLS defense in depth, and no browser service role. `[PACKAGE-HOSTED-AUTH]`
- [ ] Build the Osprey `/v1` REST API with cursor pagination, digest ETags, idempotent writes, `application/problem+json`, rate limits, and audit IDs. `[PACKAGE-API]`
- [ ] Ensure the browser uses only the API and every authorization decision is repeated server-side against stable principal/scope IDs. `[PACKAGE-HOSTED-BOUNDARY]`
- [ ] Prove the catalog and immutable objects can be reconstructed from CAS, TUF, and the transparency log after deleting the Supabase project. `[PACKAGE-HOSTED-RELEASE]`

### Phase 7 — Source collection and publication orchestration

- [ ] Bind package/subdirectory to a stable forge repository and release branch with scope plus forge-admin proof; log rebinding and quarantine it 24 hours. `[PACKAGE-PUBLISH]`
- [ ] Implement proof-free RFC 8785 payloads plus detached ordered-proof envelopes for binding, intent, one-use challenge and final authorization, using exact DSSE/WebAuthn bytes and threshold principal sets. `[PACKAGE-TYPE-INVARIANTS]`
- [ ] Bind the intent to the publisher-computed canonical SourceDigest and forge hash algorithm; recompute commit identity, reachability, manifest, and source digests before admitting the snapshot to CAS. `[PACKAGE-PUBLISH]`
- [ ] Require the CLI to recompute and compare every challenge field (source/manifest/context/dependency-lock/build-plan/release digests, compatibility, identity, ordinal, nonces) before final signing. `[PACKAGE-PUBLISH]`
- [ ] Implement the GitHub App adapter using stable repository IDs, exact commits, configured release-branch reachability, and least privilege; implement the same adapter contract for GitLab and Codeberg fixtures. `[PACKAGE-PUBLISH]`
- [ ] Fetch source server-side, validate/canonicalize it, archive it in CAS, and reject every upload, archive substitution, native payload, hook, generator, or undeclared asset before work enters the build queue. `[PACKAGE-PUBLISH]`
- [ ] Allocate ordinal/epoch under a serializable package lock, enforce compatibility claims, and map each release digest permanently to one epoch/ordinal; expired challenges retry that reserved identity. `[PACKAGE-IDENTITY]`
- [ ] Implement quarantine, independent appeal, publication, yank, revocation, transfer, recovery, and tombstone workflows as logged state machines. `[PACKAGE-TRUST]`
- [ ] Produce and publish TUF metadata only after evidence, reproducibility, and policy gates complete. `[PACKAGE-PUBLICATION-GATES]`
- [ ] Compute and sign each release's capability surface at publication and serve it in catalog metadata, so a client never fetches an excluded candidate. `[PACKAGE-CAPABILITY-CONFINEMENT]`

### Phase 8 — Hermetic build and deterministic assessment

- [ ] Provision two separately administered SLSA Build L3 platforms with ephemeral workers, independent images/control planes, declared CAS inputs, resource ceilings, isolated caches, and protected attestation keys. `[PACKAGE-TRUST]`
- [ ] Compile/type/effect check, run registry tests, infer capabilities, compare public contract/ABI/behavior claims, analyze affected clients, and require bit-identical reference outputs from both pools. `[PACKAGE-PUBLICATION-GATES]`
- [ ] Generate canonical Osprey-profile SPDX 3.0.1 SBOMs and in-toto/DSSE attestations with separate subjects for source assembly and each reference output; test byte equality independently of SLSA conformance. `[PACKAGE-PUBLISH]`
- [ ] Integrate OSV-style advisories, reachability, secrets, licenses, SAST, taint, dependency intelligence, and sandboxed dynamic behavior analysis. `[PACKAGE-ASSESSMENT]`
- [ ] Add Osprey branch coverage, mutation operators, property/fuzz runner, and client-impact testing with disclosed denominators, exclusions, and limits. `[PACKAGE-EVIDENCE-TESTING]`
- [ ] Fail publication on confirmed malware, secrets, provenance failure, non-reproducibility, forbidden capabilities, or reachable critical/high vulnerabilities; send uncertain findings to quarantine and appeal. `[PACKAGE-PUBLICATION-GATES]`

### Phase 9 — AI review under measured authority

- [ ] Build complete signed temporal receipt frames and a finite generated adversarial frame with no repository/author/family/near-duplicate leakage; draw each one-use sample through the pre-window independent evaluator VRF. `[PACKAGE-ASSESSMENT]`
- [ ] Add whole-source and semantic-diff review after deterministic analysis; treat package text as prompt injection and expose no tools, network, credentials, mutable memory, or production actions to the model. `[PACKAGE-ASSESSMENT]`
- [ ] Deterministically chunk every raw token and AST node, add cross-file graph context, and sign a complete byte/token/AST coverage ledger; any omission, truncation, timeout, or model error remains quarantined. `[PACKAGE-ASSESSMENT]`
- [ ] Permit automatic authority only for content-addressed weights, runtime, deterministic kernels/hardware profile, tokenizer, decoding/RNG/retry rules, prompt, tools, chunker, aggregation, threshold and policy; mutable APIs stay advisory, and every behavior change starts unqualified. `[PACKAGE-ASSESSMENT]`
- [ ] Enforce the simultaneous exact package-level recall, false-positive, prevalence-derived precision and adversarial-recall finite-population hypergeometric bounds with logged alpha spending; report exact worst-interval ECE as diagnostic only. `[PACKAGE-ASSESSMENT]`
- [ ] Shadow each candidate model and report PR-AUC, recall at FPR, false positives per million, Brier score, abstention, latency, and cost at the observed class prior. `[PACKAGE-ASSESSMENT]`
- [ ] Ensure no AI-only pass, rejection, publication, yank, or revocation path exists and appeal reviewers do not receive the original model verdict. `[PACKAGE-ASSESSMENT]`

## Milestone C — Quality and fair discovery

### Phase 10 — Maintenance evidence and scoring

- [ ] Implement the fixed five-component, 24-month methodology and neutral confidence prior from spec 0032 in integer-only `src/osprey/packages/manager/core`; version and sign every calculation (`registry::Arithmetic` primitives are in place). `[PACKAGE-MAINTENANCE]` `[PACKAGE-EVIDENCE-ARITHMETIC]`
- [ ] Measure median/p90 vulnerability response and propagation, reachable overdue risk, recurrence, continuity/handover, release reliability, tests/mutation/fuzz/client impact, normalized static findings, and response quality from immutable evidence. `[PACKAGE-MAINTENANCE]`
- [ ] Treat missing data as `Unknown`; exclude raw vulnerabilities, commits, issues, stars, downloads, dependents, fame, age, and activity volume. `[PACKAGE-MAINTENANCE]`
- [ ] Render score, confidence, component vector, raw evidence, timestamps, sample sizes, tools/rules, exclusions, and method changes on every page. `[PACKAGE-MAINTENANCE]`
- [ ] Add counterfactual tests proving disclosure, a new package, a small community, or absence of popularity cannot mechanically lower rank. `[PACKAGE-DISCOVERY]`
- [ ] Differentially replay every normalizer, percentile, confidence and score against a second independent Osprey implementation and the signed basis-point golden vectors. `[PACKAGE-DISCOVERY-AUDIT]`

### Phase 11 — Fair search and Osprey WebAssembly UI

- [ ] Implement eligibility, the fixed merit formula, query-intent cohorts, 28-day exposure debt, eight-merit/two-exploration allocation, 80% intent floor, and two-results-per-publisher cap in shared Osprey core. `[PACKAGE-DISCOVERY]`
- [ ] Enforce the signed 512-byte summary/12-keyword grammar and exact bounded key/summary/keyword/compiler-API lexical and semantic serialization; never index arbitrary package documentation as ranking text. `[PACKAGE-DISCOVERY-INTENT]`
- [ ] Implement publisher-normalized entitlement, exact apportionment/ties, and auditable client-nonce VRF exploration with deterministic/degraded modes. `[PACKAGE-DISCOVERY-EXPOSURE]`
- [ ] Exclude all popularity/commercial features from default ranking and model training; expose `Most used` only as a separately selected sort. `[PACKAGE-DISCOVERY]`
- [ ] Collect randomized propensity-logged exploration events without raw IPs or retained raw queries and expire aggregate cohort ledgers after 28 days. `[PACKAGE-DISCOVERY-AUDIT]`
- [ ] Gate ranker releases on NDCG, coverage@10, new-package exposure, Gini, publisher share, exposure/merit, exploration conversion, and utility loss. `[PACKAGE-DISCOVERY-AUDIT]`
- [ ] Build the Osprey-to-WASM web UI for search, source, dependencies, effects/capabilities, evidence, proofs, advisories, release history, publishing, transfers, quarantine, and appeals. `[PACKAGE-IMPLEMENTATION-LANGUAGE]`
- [ ] Meet keyboard, screen-reader, responsive, no-JavaScript evidence-download, and clear `No blocking finding` language requirements. `[PACKAGE-DISCOVERY]`

## Milestone D — Operational launch

### Phase 12 — Hardening and general availability

- [ ] Verify `[PACKAGE-IMPLEMENTATION-LANGUAGE]` across the complete delivered source and build graph: all package-system executable implementation, tests, adapters, and operational tooling are Osprey; no package-specific Rust/C/JavaScript/Python component or launcher is present. `[PACKAGE-CONFORMANCE]`
- [ ] Operate independent CAS replicas, log monitors, four external witnesses with 3-of-4 quorum, cross-logging, 3-of-5 offline root custody, 2-of-3 release-role HSMs, documented ceremonies, and compromise runbooks. `[PACKAGE-TRUST]`
- [ ] Complete SLSA Build L3 for all packages and Source L4 for privileged packages; publish evidence only after independent verification. `[PACKAGE-TRUST]`
- [ ] Run disaster recovery without Supabase, mirror/CDN, either independent build platform, one witness, and each online signing role in turn. `[PACKAGE-REGISTRY-CONFORMANCE]`
- [ ] Fuzz every parser/protocol boundary and pass all conformance suites from specs 0029–0032 on Linux, macOS, and Windows. `[PACKAGE-CONFORMANCE]`
- [ ] Commission an independent security/cryptography/privacy/fairness audit and close every critical/high finding; publish the report and remediations. `[PACKAGE-REGISTRY-CONFORMANCE]`
- [ ] Launch with scoped source packages only; do not add binary publication, lifecycle hooks, SemVer selectors, direct database clients, or pay-to-rank as post-audit shortcuts. `[PACKAGE-DECISIONS]`
- [ ] Run `make ci` and the registry deployment rehearsal from an empty account to published package, fresh-client add, offline rebuild, update, yank, revoke, restore, and appeal. `[PACKAGE-HOSTED-CONFORMANCE]`

## Hosted rollout — Stage A (capabilities and contracts)

- [ ] Map spec 0044 IDs to the existing packages and add named failing Osprey regressions before each implementation change. `[PACKAGE-HOSTED-CONFORMANCE]`
- [x] Run the existing `make -C src/osprey/packages registry-test` baseline; keep its folder-registry behavior while extending the same API. `[PACKAGE-FOLDER-REGISTRY]` Done: `src/osprey/packages/Makefile` `registry-test`, run by CI as `test-registry-gc` and `test-registry-arc`.
- [ ] Inventory/probe JWT/JWKS verification, WebAuthn/DSSE, entropy, secure time, constant-time comparisons, TLS verification, credential storage, PostgreSQL transactions/pooling, headers/cookies, secret delivery and browser/WASM host interfaces; record absent capabilities in `src/osprey/packages/manager/blockers/` with failing assertions; resolve through Osprey, never a native/JavaScript shim. `[PACKAGE-HOSTED-AUTH]`
- [ ] Verify the native API image's Cloud Run bind/port behavior and secret access; the current JSON bind/port config and missing environment support need an explicit tested solution; no shell secret interpolation or credentials baked into JSON/images. `[PACKAGE-HOSTED-TOPOLOGY]`
- [ ] Extend the canonical domain model for stable principal/org/package IDs, accepted org memberships, the single administrator FK, explicit permission grants, epochs, sessions, invitations and handovers; reconcile spec 0031 and `src/toolchain/schemas/package-registry.td` before generating implementation types; no duplicate handwritten model. `[PACKAGE-OWNERSHIP-MODEL]`

## Hosted rollout — Stage B (Supabase ownership and RLS)

Governed by spec 0045 and `src/osprey/packages/manager/supabase/migrations/schema.yaml`; provision the private `registry` schema as a platform resource before table migration; keep the bootstrap FORCE-RLS/default-deny with no runtime grants and reject `ownership.not-ready` until policies/handlers/tests ship together.

- [ ] Create isolated local/staging projects and apply the pinned DataProvider migration YAML: same-org composite FKs, one non-null active-member administrator, organization/name uniqueness, immutable identities and non-cascading history; no handwritten SQL/ORM schema. `[PACKAGE-OWNERSHIP-DATABASE]`
- [ ] Provision restricted runtime logins with no owner/BYPASSRLS/elevated role membership; separate migration/collector/eligibility/audit permissions; remove default grants and unused Data API/GraphQL/Realtime exposure. `[PACKAGE-OWNERSHIP-RLS]`
- [ ] Implement one transaction/context boundary and the exact RLS predicates/column permissions for organization memberships and package permissions; prove pooled request isolation across success, cancellation and rollback. `[PACKAGE-OWNERSHIP-RLS]`
- [ ] Implement spec 0045: organization-only ownership, organization custodians/members, one package administrator, explicit per-package permissions, targeted invitations, admin handover, maintenance responsibility and 30-day exceptional recovery; organization custodians cannot grant package permissions unless they hold that package's sole administrator reference; all mutations produce atomic audit/outbox evidence. `[PACKAGE-OWNERSHIP-ADMIN]`
- [ ] Implement durable rollout-mode/version checks from `[PACKAGE-HOSTED-CONTROL]`, including deny-on-store-failure and epoch invalidation of queued writes; package permissions cannot change deployment modes. `[PACKAGE-HOSTED-CONTROL]`
- [ ] Test two orgs, multiple packages and an outsider against every CRUD/join/view/storage path using actual production-shaped roles, including direct Supabase requests; include positive controls, row/column escalation attempts, stale sessions, same-connection reuse and concurrent removal/finalization. `[PACKAGE-OWNERSHIP-CONFORMANCE]`
- [ ] Prove the anti-name-sitting requirement end to end: `(organization_id, name)` uniqueness and no global leaf-name claim or reservation. `[PACKAGE-NAMES-NO-SITTING]`
- [ ] Audit actual `pg_roles`, memberships, schema/column/default grants, policies, function execution, exposed schemas and storage policies; a textual policy-name check alone is insufficient. `[PACKAGE-OWNERSHIP-RLS]`

## Hosted rollout — Stage C (GitHub login and publication authority)

- [ ] Configure separate staging/production Supabase GitHub OAuth apps and exact provider/API callback URLs; implement API-managed PKCE/state and opaque browser sessions. `[PACKAGE-HOSTED-AUTH]`
- [ ] Implement bounded sessions, logout/revocation, CSRF, exact-origin CORS, fresh WebAuthn proof and the CLI browser handoff/credential-store path. `[PACKAGE-HOSTED-AUTH]`
- [ ] Prove identity linking/recovery cannot transfer package authority through email, username reuse or Supabase automatic linking; restore and rotate credentials through logged principal proofs. `[PACKAGE-HOSTED-AUTH]`
- [ ] Prove revoked/unapproved linked identities lose private reads as well as mutations; positive anonymous reads use the separate published-projection reader; every human CLI publication uses browser WebAuthn; OS-store access tokens do not sign releases. `[PACKAGE-HOSTED-AUTH]`
- [ ] Configure the separate least-privilege selected-repository GitHub App; implement dual-authority binding, stable provider IDs, signed webhook validation, replay rejection and synchronous installation/branch/owner revalidation. `[PACKAGE-PUBLISH]`
- [ ] Exercise all of `[PACKAGE-HOSTED-AUTH]`, including uninstallation/repository transfer with delayed webhooks and a removed maintainer retaining a valid JWT. `[PACKAGE-HOSTED-AUTH]`

## Hosted rollout — Stage D (source publication, workers and independent trust)

- [ ] Extend the existing API with signed intent/challenge/finalization endpoints sharing core policy; reject tarballs, multipart bytes, public storage writes and arbitrary repository URLs. `[PACKAGE-HOSTED-PUBLISH]`
- [ ] Collect only a bound exact commit, freeze canonical source in private CAS, allocate immutable ordinal/digest reservations and enforce one-use expiring challenges and authority epochs. `[PACKAGE-PUBLISH]`
- [ ] Implement serializable ownership/publication transitions with consistent locks, idempotency and an outbox; revalidate authority before collection, signing and eligibility; prove races with grant removal, transfers, duplicate requests and process crashes. `[PACKAGE-OWNERSHIP-RLS]`
- [ ] Complete parent-plan Phases 5 and 7–10 prerequisites: independent reproducible builders, scanning/evidence, signing custody, TUF/status, witnessed log and fresh-client verification. `[PACKAGE-TRUST]`
- [ ] Complete the external operator/custody commitments from the inventory, embed the reviewed initial TUF root in the CLI and publish its version/five fingerprints through the governance repository and DNSSEC; verify the fresh-client bootstrap and sequential root rotation independently. `[PACKAGE-TRUST]`
- [ ] Prove storage overwrite/upsert/list/signed-upload refusal and quarantine isolation; run malicious source/prose, archive/path, SSRF and build-sandbox tests. `[PACKAGE-HOSTED-PUBLISH]`

## Hosted rollout — Stage E (Cloudflare and read-only staging)

- [ ] Provision Cloudflare proxy/TLS/WAF, separate static website deployment, cookieless content origin, GCP load balancer/Cloud Armor/NEG, restricted Cloud Run and DNS-validated origin certificates. `[PACKAGE-HOSTED-TOPOLOGY]`
- [ ] Install the hostname-scoped header-overwrite transform and constant-time API secret guard; test secret rotation with a short dual-version overlap, then prove the retired secret fails; test missing-secret startup failure. `[PACKAGE-HOSTED-EDGE]`
- [ ] Verify registrar lock, zone DNSSEC/DS, individual administrator MFA/recovery and least-privilege ruleset edits; validate Cloud Run's isolated probe port without a public health bypass (serverless NEGs need no load-balancer health check). `[PACKAGE-HOSTED-EDGE]`
- [ ] Block direct run.app/revision/LB-IP traffic and traffic via a different Cloudflare tenant; test public success through the intended hostname, including OPTIONS and CLI requests. `[PACKAGE-HOSTED-EDGE]`
- [ ] Implement the Osprey WASM website's login, organization/membership/package-admin/permission controls, repository binding, publication progress and rejection reasons; escape/sanitize hostile content and never expose credentials or quarantined source. `[PACKAGE-HOSTED-ROUTES]`
- [ ] Wire and test every flow in `[PACKAGE-HOSTED-ROUTES]`, including org-scoped name availability, existing warnings, targeted invitation acceptance, exact single-admin handover, operational grants, maintenance and pending/rejected publication states; incomplete screens stay hidden. `[PACKAGE-HOSTED-ROUTES]`
- [ ] Prove private/cookie/Authorization responses never enter CDN caches; immutable source caches verify digests and mutable signed status remains fresh; load-test numeric API/worker/instance limits and set cost alerts. `[PACKAGE-HOSTED-EDGE]`
- [ ] Cut over GitHub OAuth/App webhooks only through the tested protected ingress; test invitation/owner/recovery notifications, SPF/DKIM/DMARC, failure alerts and the authenticated inbox; complete private-data retention/deletion handling before public enrollment. `[PACKAGE-HOSTED-AUTH]`

## Hosted rollout — Stage F (deploy/recover rehearsal and release gates)

- [ ] End the deferred CI arrangement before hosting: add registry/security/database/browser/edge suites to required checks, preserving every existing gate and coverage floor (folder-registry suites already run in `make test` and CI; the hosted suites under `src/osprey/packages/manager/{auth-tests,database-tests,governance-tests,hosted-tests}` are not yet wired); update required-check configuration and its verification together. `[PACKAGE-HOSTED-CONFORMANCE]`
- [ ] Add dedicated registry build/deploy workflows; every staging/production/frontend/manual deployment depends on a fresh unconditional exact-SHA `audit` job; run `make audit`, `make ci`, `make -C src/osprey/packages registry-test` and all hosted conformance checks; never reuse a passing audit from another SHA. `[PACKAGE-HOSTED-RELEASE]`
- [ ] Build the native image/static artifacts once, record immutable digests, apply reviewed additive schema changes separately, verify RLS drift/tests, deploy a no-traffic revision and run release smoke checks through an operator-restricted candidate route retaining the ingress/edge controls; prove it reaches the candidate revision without public run.app/tag access; never migrate at build/startup. `[PACKAGE-HOSTED-RELEASE]`
- [ ] Rehearse traffic/artifact rollback to the exact prior release, checking schema compatibility and preserving consumed nonces, authority epochs, revocations and log history. `[PACKAGE-HOSTED-RELEASE]`
- [ ] Provision and rehearse `[PACKAGE-HOSTED-CONTROL]`: named mode-promotion authority, narrow independent incident job/credentials, freeze under edge/API outage, and jointly approved unfreeze; prove neither operator mode changes nor the independent incident path can grant package permissions or eligibility. `[PACKAGE-HOSTED-CONTROL]`
- [ ] Restore off-provider database/CAS/ownership/trust backups into an isolated replacement project; measure RPO ≤ 5 minutes and RTO ≤ 4 hours, reconcile all acknowledged security state with the signed log, and keep writes frozen until reconciliation passes. `[PACKAGE-HOSTED-RELEASE]`
- [ ] Rehearse from an empty account: GitHub login → create org → invite member → explicit publishing-permission grant → selected-repo binding → publish → fresh-client fetch/build → remove maintainer → reject queued/replayed publish → yank/revoke → transfer/recovery → restore; retain positive results and every expected denial. `[PACKAGE-HOSTED-CONFORMANCE]`
- [ ] Obtain an independent application/RLS/crypto/infrastructure audit and close all critical/high findings, including review of provider identity linking and pooler behavior. `[PACKAGE-HOSTED-CONFORMANCE]`

## Hosted rollout — Stage G (controlled production rollout)

- [ ] Promote the same artifacts from staging with server-side modes `read-only`, `allowlisted-publishers`, `public-publishers`, `write-frozen`; keep separate switches for reads, ownership administration and publication so an incident can freeze sensitive mutations while retaining eligible public reads; keep suspensions/revocations available through the independent incident path. `[PACKAGE-HOSTED-CONTROL]`
- [ ] Roll out in waves (read-only → internal publishers ≥ 24 h → invited organizations ≥ 48 h → public enrollment), promoting only on each wave's recorded gate evidence; run the new-revision traffic progression 1% → 10% → 50% → 100% with ≥ 30-minute holds and ≥ 100 synthetic/API checks plus publication/revocation probes; separate the static website artifact promotion and keep prior-digest rollback. `[PACKAGE-HOSTED-CONTROL]`
- [ ] Hold the initial operational gates: API 5xx < 1% over 15 minutes, metadata p95 < 1 s excluding cold-start probes, no outbox item stalled beyond 5 minutes, zero unexplained proof/RLS/origin failures, all signed metadata within freshness; any suspected cross-org access, unauthorized acceptance, leaked credential or corrupt artifact immediately enters `write-frozen`. `[PACKAGE-HOSTED-CONTROL]`
- [ ] Alert on denied cross-scope attempts, privilege/policy drift, identity/owner changes, secret/JWKS failures, GitHub suspension, queue/outbox age, signature/witness freshness, digest mismatches and unexpected storage writes; log request/trace/principal/scope/package/operation IDs and policy versions (never cookies, tokens, private source or secrets); retain access-controlled audit evidence and an operator incident/rotation runbook; test alert delivery before opening enrollment. `[PACKAGE-HOSTED-CONTROL]`
