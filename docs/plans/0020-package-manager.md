# Plan 0020 — Package Registry and Manager

**Status:** in progress. The folder registry, verified fetching, deterministic resolution, folder locks, local overrides, folder publication and uv integration are implemented under [`packages/manager/`](../../packages/manager/README.md). `make registry-test` exercises fixtures, manifests, the client, core/API suites and a loopback consumer. Real compiler checks on 25 October 2026 passed all eight build forms across Default and Python flavors: bare build, run, relative project path and absolute project path from another directory. Derived manifests now live in the selected project's `.osprey/build/` with manifest-relative source paths. The second-machine probe remains open.

Crash-safe activation, canonical build-plan/release artifacts, authenticated public publishing, the hosted package website and the trust plane remain delivery work. Current filesystem handlers use POSIX shell commands; cross-platform acceptance remains open. Checked-off items below record specific delivered subsets, not completion of every requirement in their phase.

**Contracts:** [package management](../specs/ecosystem/0029-PackageManagement.md), [trust and discovery](../specs/ecosystem/0030-PackageRegistryTrustAndDiscovery.md), [domain types](../specs/ecosystem/0031-PackageDomainModel.md) and [evidence](../specs/ecosystem/0032-PackageEvidenceAndDiscovery.md). [Research references](../package-manager-research.md) support the fixed design; phases validate it rather than reopening product decisions.

**Ownership:** [organization-only packages, one package admin and explicit permissions](../specs/ecosystem/0045-OrganizationPackageGovernance.md); tables are defined by DataProvider migration YAML and the canonical typeDiagram. No personal scopes or global package leaf names.

**Hosted rollout:** [deployment and ownership spec 0044](../specs/ecosystem/0044-PackageRegistryDeploymentAndOwnership.md) and the [staged rollout plan](0020-package-manager-rollout.md) govern Cloudflare ingress, website/API deployment, Supabase GitHub authentication, organization/package RLS, maintainer grants, source-derived publication, recovery and launch gates. They extend phases 6–12; hosted writes stay disabled until their conformance and independent review pass. The deferred local-only registry CI arrangement below ends before hosted deployment.

## Delivery target

First, ship a working, self-hostable registry: an Osprey API that serves
working packages from a folder, and an Osprey client that fetches and verifies
them (`[PACKAGE-FOLDER-REGISTRY]`). Then grow it into a public, source-derived Osprey package registry with a small local command
surface, complete deterministic dependency resolution, end-to-end provenance,
non-executable installation, evidence-based maintenance scoring, and fair
discovery. Local development can replace any canonical package key globally with
a checkout and switch it back in one command. Release builds reject those
overlays, pin every input, and contain exactly one source for each package key.
The complete package system MUST be implemented in Osprey, including the CLI, manifest parser, domain model, resolver, locks, cache, cryptography, filesystem transactions, process isolation, HSM/forge adapters, registry API, publication pipeline, scoring, and WebAssembly frontend. Package tests, independent reference implementations, and operational tooling are also Osprey. There is no Rust host, CLI bridge, or other implementation-language exception. The binding contract is `[PACKAGE-IMPLEMENTATION-LANGUAGE]` in spec 0029.

The launch package format is Osprey source plus closed-format declared data. SQLite and
other native components are explicit system capabilities installed separately
through operating-system package managers. Osprey package installation never downloads native provider binaries, runs lifecycle hooks or invokes operating-system installers. Declared CPython interop dependencies use the separate uv environment described in spec 0029.

## Current repository seams

- `crates/osprey-project` assembles mixed-flavor source graphs. Its compiler manifest reader handles `[project]` and `[modules]`; package metadata and resolution belong to the Osprey package client.
- `crates/osprey-cli` supplies the existing compiler, project build, tests, documentation, formatter and target drivers. `packages/manager/cli` is the separate Osprey package frontend, including its folder-lock model, verified install and derived compiler project.
- `packages/manager/core`, `manifest` and `resolver` implement package identities, canonical manifests/source digests, capability surfaces and deterministic resolution. A derived compiler manifest is not the complete canonical build plan with toolchain/provenance inputs required below.
- `packages/manager/api` serves the database-free folder registry. The hosted browser UI, Supabase data plane, TUF client, transparency log, provenance builders and discovery index remain planned.
- The folder registry proves source integrity, not publisher authority. Authenticated publication and trust proofs belong to Milestone B; source verification must remain usable without those services.

These compiler crates are build tools for the Osprey package application, not implementation locations for package semantics. The binding language requirement is `[PACKAGE-IMPLEMENTATION-LANGUAGE]` in spec 0029. Missing host capabilities remain explicit blockers with regression probes.

## One-source architecture

The implementation is placed deliberately:

```text
schemas/package-registry.td       canonical typeDiagram model
packages/manager/core/                    Osprey manifests, domain, policy, resolver, scoring
packages/manager/cli/                     Osprey command parsing, presentation, compiler invocation
packages/manager/host/                    Osprey effect handlers, crypto, filesystem and process adapters
packages/manager/api/                     Osprey REST server: folder registry first, hosted data plane later
packages/manager/sample-registry/         a folder of real packages the server serves and the tests fetch
packages/manager/web/                     Osprey browser application -> WebAssembly
packages/manager/tests/                   Osprey conformance tests, independent oracles, fuzz drivers
packages/manager/blockers/                Osprey probes for host capabilities Osprey lacks (red by design)
packages/manager/ops/                     Osprey publication, migration and operational tooling
packages/manager/supabase/migrations/     DataProvider migration YAML for PostgreSQL schema/RLS (Phase 6)
```

`schemas/package-registry.td` is the source of generated Osprey domain types as
soon as typeDiagram's Osprey target lands. Handwritten copies are forbidden.
Until generation is available, implementation uses serialized conformance
fixtures directly and does not duplicate the model in service code.

`packages/manager/core` owns every rule shared by CLI, API, and web: identities,
manifest semantics, catalog projection, solver clauses/objectives, score
methodology, and ranking policy. `packages/manager/host` implements effect handlers in Osprey and cannot choose policy. API preview resolution and local lock resolution run the same compiled core and policy digest. Schema, manifest, fixture, and deployment data are declarative inputs; they cannot hide executable package logic in another language.

The Osprey command frontend owns the planned `osprey package`, `add`, `use`, `lock`, and other package commands. It invokes the existing compiler for compilation through Osprey process effects. Bootstrap and command routing must not require edits to the existing Rust CLI or a non-Osprey launcher.

## Milestone 0 — Working registry API serving working packages

**First and foremost.** Spec `[PACKAGE-FOLDER-REGISTRY]` is the contract. Exit
criterion: one person starts the Osprey server over a folder of packages on
one machine, and another fetches a package from it over the network, has its
digest verified, and builds a project that uses it. All of it is Osprey, and
every item lands with tests.

### Phase F1 — Folder catalog and source bundles

- [x] Parse a release directory path `@scope/name/<epoch>.<ordinal>` into key,
      epoch, and ordinal, rejecting every malformed segment with a stable code.
- [x] Walk the registry root through a `Host` directory effect. Until Osprey has
      a directory builtin (`files.directory` blocker), the native handler lists
      files through the `Process` effect, and the blocker records it.
- [x] Validate each release with the shared source rules, compute its
      `SourceDigest`, and report every invalid release with its reason.
      `ReleaseFiles::assemble` adds the LFS-pointer and submission-bound rules;
      `tests/src/loader_tests.osp` drives the loader over
      `fixtures/folder/refusals.json`: two releases served, nine refused, each
      with its stable code.
- [x] Encode and decode the canonical JSON source bundle, with round-trip,
      ordering, escaping, and tamper tests.
- [x] Build the catalog keyed by digest and by key, ordered by key, epoch,
      ordinal.

### Phase F2 — Osprey registry server

- [x] Serve `GET /v1/health`, `/v1/packages`, `/v1/packages/{scope}/{name}`,
      `/v1/releases/{digest}`, and `/v1/releases/{digest}/source` from
      `packages/manager/api/`, with `ETag`s and `application/problem+json` errors.
- [x] Route through one pure function from request to response, tested for
      every endpoint and every error without opening a socket.
- [x] Read root, bind address, and port from a JSON config file, because
      Osprey has no argv or environment builtin yet.
- [x] Ship `packages/manager/sample-registry/` with at least three real packages, one
      depending on another, each compiling with the existing compiler.
- [x] End-to-end test: start the server on a loopback port, request every
      endpoint with the Osprey HTTP client, and assert the bodies byte for byte.
      `packages/manager/e2e` builds and starts the server on `127.0.0.1:18797` and
      compares status, ETag and body of every endpoint with the pure `Router`.
- [x] `make registry-serve-sample` starts the server over the sample registry.
- [x] Local registry `[PACKAGE-LOCAL-REGISTRY]`: `make registry-serve` publishes the
      repository's own `packages/*` into `target/local-registry` with
      `publish --folder` and serves it on `127.0.0.1:8787`. All 19 packages
      stage. The VS Code extension bundles the server and the staged
      tree, starts and stops it from commands, the `osprey.toml` explorer menu
      and the status bar, and offers to start it when the package manager opens
      against a silent loopback registry (`test/suite/local-registry.test.ts`).
- [x] Every package in `packages/` is served. `@osprey/jinja2`,
      `@osprey/numpy` and `@osprey/safetensors` bound native panics, which
      capability confinement refuses and [FAILURE-EXPLICIT] forbids: jinja2's
      unused `quarantine` is gone (its `halt` already reports render errors),
      safetensors' unused binding is gone, and numpy's GPU product now answers
      a failed `mat_mul` as numpy's `ValueError`
      (`errors.product_lengths` in its suite, under both storage handlers).
      `crates/osprey-cli/tests/no_osprey_panics.rs` refuses any package or
      example that binds a native panic, abort or exit.
- [x] Harden the server to `[PACKAGE-HARDENING]` S1–S3: `405` carries
      `Allow: GET`; every response carries `nosniff`, `default-src 'none'` and
      `no-referrer`; digest-addressed responses are `immutable`, the rest
      `no-cache`; traversal, percent-encoded and method probes are answered from
      the catalog alone. `Router::hardeningHeaders`, asserted by
      `router_tests` ("hardening headers", "hostile probes"); the runtime's
      request bounds (1 MiB, 32 KiB headers, 5 s) are recorded in S4.
- [x] Publish `SECURITY.md` (`[PACKAGE-HARDENING]` D1: private reporting,
      scope, response target).

### Phase F3 — Download client

- [x] `fetch @scope/name[@epoch]` and `fetch <digest>` download a bundle from
      the configured registry URL, verify its digest, and write
      `.osprey/packages/@scope/name/<digest>/`.
- [x] Reject a tampered bundle, a wrong key, a missing release, and an
      unreachable registry, each with a stable code and a test.
- [x] A consumer project builds against a fetched package: the end-to-end
      demo of the exit criterion, run on loopback by `make registry-test`.
      `packages/manager/e2e` fetches every release by name, epoch and digest, checks the
      installed bytes, then builds and runs `e2e/consumer/` against the fetched
      `@demo/report`, `@demo/maths` and `@demo/greeting` sources.

### Phase F4 — Stand it up anywhere

- [x] Document running a registry on any host, port, and folder, and pointing
      a client at it, in `packages/manager/README.md`.
- [ ] Bind to a non-loopback address and serve a second machine on the same
      network; record the measured result. Needs a second machine; binding
      `0.0.0.0` from an automated session was refused by its permission policy,
      so this stays a manual step.

Registry suites run through `make registry-test` only until the package system
stabilises. Wiring them into `make test` and CI is deferred to the last phase
of this plan, "Deferred — CI integration".

## Milestone A — Local package engine

### Phase 0 — Schemas and adversarial fixtures

- [x] Establish `packages/manager/core`, `packages/manager/cli`, `packages/manager/host`, and `packages/manager/tests` as ordinary Osprey projects built by the existing compiler; prove an Osprey command reaches the shared core and returns a tested result. Projects share source through `source_roots`; `osprey-pkg doctor` reaches `registry::Capabilities` and `packages/manager/tests` covers it with fixture hosts.
- [x] Inventory required argument, byte-buffer, filesystem, atomicity, process, cryptographic, and network capabilities. Exercise each through Osprey tests and record unavailable capabilities before depending on them. No substitute implementation in another language is permitted. Recorded in `packages/manager/README.md` and `packages/manager/core/src/capabilities.osp`; argv, exit status, environment, binary-safe reads, directory operations (including atomic rename and fsync) and a UTC clock remain blocked. Bitwise builtins now exist and are covered by an inventory regression; the registry SHA-256 still uses lookup tables. The network is exercised by `packages/manager/e2e` (server and client on loopback); SHA-256 runs in Osprey.
- [x] Add an Osprey conformance gate that rejects non-Osprey executable source in the package system, including tests, host adapters, operational tools, and handwritten browser glue; validate declarative inputs separately. `registry::Conformance` plus the checked-in-tree suite.
- [x] Extract the typeDiagram block from spec 0031 into
      `schemas/package-registry.td` and make documentation import/render it. The spec keeps the rendered block and a suite fails when the two differ.
- [x] Add canonical JSON fixtures for manifests; proof-free publication payloads
      and detached ordered-proof envelopes; release IDs/statuses; typed TUF,
      inclusion/consistency and witness bundles; both locks/proofs; build receipts;
      evidence, attestations, advisories, and state transitions.
      `fixtures/canonical/` holds 44 values with RFC 8785 bytes and `H(d, x)`,
      bound by 46 digest/equality relations; `fixtures/transitions/` holds
      release-status chains; `fixtures/schema/` holds accept/reject cases.
- [x] Add malicious fixtures for path traversal, case collisions, symlinks,
      submodules, LFS pointers, devices, executable modes, Unicode confusables,
      oversized trees, decompression bombs, and digest substitution.
      `fixtures/malicious/confusables.json` and `png-bombs.json` (asserted by
      `registry::Png` header and chunk checks) join `paths.json` and
      `source/minimal-tree.json`. The IDAT inflate-overrun case is recorded for
      the PNG decoder, which does not exist yet.
- [x] Add bounded dependency graphs covering satisfiable, unsatisfiable,
      downgrade, epoch, target, capability, revocation, technical-lag, and
      cycle-rejection cases, each with the one expected lock/explanation.
      `fixtures/graphs/` (14 cases) agree with an exhaustive bounded oracle in
      `tests/src/graph_oracle.osp`; the CDCL resolver of Phase 3 must match them.
- [x] Record TUF, transparency, DSSE/in-toto, RFC 8785, SHA-256 agility, and
      compromised-mirror vectors from their primary specifications.
      `fixtures/vectors/`: FIPS 180-4 SHA-256, digest agility, RFC 8785
      sorting/escaping, the DSSE PAE example, RFC 9162 Merkle roots, inclusion and
      consistency proofs, TUF 1.0 client-workflow and compromised-mirror cases,
      asserted through `registry::Merkle`, `Dsse` and `TufCheck`.
- [x] Add golden vectors for exact UTC millisecond timestamps, every list sort/
      duplicate rule, detached PAE/WebAuthn bytes, every digest preimage/omission,
      and acyclic dependency-selection/build-plan/full-lock staging.
      `fixtures/time/datetimes.json`, `fixtures/order/lists.json` (104 cases over
      `registry::SetOrder`), `vectors/dsse.json`, and
      `fixtures/canonical/relations.json` for omissions and staging.
- [x] Run schema/fixture validation as a fail-fast stage of `make registry-test`, covering every suite and fixture. Joining `make test` is deferred to "Deferred — CI integration". `packages/manager/fixture-check` runs first: data-only fixtures, strict JSON, defined spec citations, complete indexes, and every value named in `fixtures/schema-map.json` valid against `schemas/package-registry.td` and in canonical list order; it also rejects undefined spec citations in registry source.

Exit gate: the package application and test harness compile from Osprey source without a package-specific host or launcher in another language, and every serialized form and security state transition has a reviewed, byte-exact fixture before storage or API code exists.

### Phase 1 — Manifest and canonical identity

The TOML parser and manifest validators live in `packages/manager/manifest/` with
their own test project, `packages/manager/manifest/tests` (`make registry-test` runs
it). They stay out of the shared core because of type-check cost, measured
with `--check` on the assembled `packages/manager/tests` program: 38 s wall (307 s CPU)
without the parser, 115 s (662 s CPU) with it. Only the parser-free
`ManifestModel` types are in `core/`; projects that parse manifests add
`../manifest/src` themselves. The `doctor` manifest verdict waits on that cost.

- [x] Implement one typed TOML parser in `packages/manager/core`, written in Osprey, that preserves current `[project]`/`[modules]` behavior and adds
      `[package]`, bounded discovery metadata, all three dependency tables,
      `[[assets]]`, and `[system.*]` from `[PACKAGE-MANIFEST]`.
      `registry::Toml` (strict TOML subset: tables, arrays of tables, dotted
      and quoted keys, inline tables, integers, strings, booleans; floats,
      datetimes and non-ASCII are rejected with located codes) and
      `registry::Manifest` with `ManifestSections`/`ManifestDeps` project
      `osprey.toml` onto the canonical `PackageManifest`; the spec example
      round-trips to `fixtures/manifests/ledger/canonical.json` and its digest.
      Lives in `packages/manager/manifest/` for now (see above).
- [x] Have the Osprey frontend validate manifests and prepare deterministic compiler inputs through the existing compiler interface. Keep package parsing and resolution out of the compiler's Rust project loader. `build` parses `osprey.toml` with `Manifest::parse`, verifies the lock, writes the derived project `.osprey/build/osprey.toml` (absolute roots, TOML-escaped strings; exact bytes asserted in `cli/tests` build suite) and invokes the compiler on it; the compiler's loader reads only that derived file and knows nothing of packages.
- [x] Accept `[project].python_sources` in published manifests as part of the
      canonical manifest (`pythonSources`), keep every listed path inside a
      source root when `[package]` is present (`manifest.python-source-unrooted`),
      and refuse any `.py` file in a release tree that does not begin with
      `# osprey: flavor=python` (`source.python-flavor-header`), so Python-flavor
      ports such as `@osprey/numpy` publish as Osprey source
      (`[PACKAGE-MANIFEST]`, `[PACKAGE-SOURCE-CANONICAL]`, spec 0043).
      `ManifestSections`/`Manifest`/`ManifestJson`, `ReleaseFiles::treeCode`,
      `schemas/package-registry.td`; `manifest/tests` 15/15, `bundle_tests`,
      the ledger canonical fixture and digest re-derived.
- [x] Reject SemVer/version fields, upper ranges, peer/optional dependencies,
      forge dependencies, published path dependencies, and unknown security-
      relevant keys with actionable source spans. `manifest_tests.osp`
      "rejects dependency selectors" and "rejects structure": every forbidden
      selector key and every unknown table or key fails with its code at the
      offending `line:column`, before any hashing.
- [x] Validate scoped ASCII names, SPDX expressions, source roots, dependency
      epochs/minimum ordinals, and system-capability declarations.
      `manifest_tests.osp` "rejects package metadata" and "rejects systems and
      assets": names via `Names`, licenses via `Spdx`, roots via `Paths`
      (duplicates and case-fold collisions), epochs and floors via the bounded
      integer rules, capability grammar and once-per-manifest, asset kinds,
      roles, extensions and digests.
- [ ] Implement the exact ASCII path grammar, entry encoding, root selection and
      domain-separated SourceDigest in `[PACKAGE-SOURCE-CANONICAL]`, plus
      manifest, build-plan, and ReleaseDigest generation. Path grammar, case-fold
      collisions, exclusions, forbidden entry kinds, LFS pointers, submission
      bounds, envelope bytes, `SourceDigest` and `H(d, x)` are in `registry::Paths`,
      `registry::Source`, `registry::Digests` and a pure-Osprey `registry::Sha256`,
      golden-tested against independently computed fixtures in
      `packages/manager/tests/fixtures/source`. Canonical manifest parsing/digests and source-root selection now exist. The complete canonical build-plan and release-artifact contract remains open; the derived compiler manifest does not close it.
- [ ] Generate a deterministic build plan from the project graph, toolchain,
      target, dependencies, and inferred type/effect/capability surface.
- [ ] Add cross-platform golden vectors proving identical digests on Linux,
      macOS, and Windows.
- [x] Compute each release's ambient capability surface
      (`[PACKAGE-CAPABILITY-CONFINEMENT]`) in `registry::Conformance` from a
      builtin-to-capability fixture, reject shadowed builtin names, accept
      `capabilities = [...]` grants on `[dependencies]` entries, and golden-test
      the surfaces of every sample-registry release. The scan lives in
      `registry::Surface` (not `Conformance`, which stays the
      implementation-language gate): its table equals
      `fixtures/capabilities/builtins.json`; a builtin's name declared with
      `fn`, `let` or `mut` is `source.shadowed-builtin` at publication
      (`ReleaseFiles::treeCode`); grants parse per the item above; and
      `fixtures/capabilities/sample-surfaces.json` records every sample
      release's surface, asserted by `surface_tests` over the real files.
- [x] Accept `[system.python]` with `capability = "python/interop"` and a
      CPython floor, and golden-test that a package's `pyproject.toml` and
      `uv.lock` are ordinary `SourceDigest` entries (`[PACKAGE-PYTHON-UV]`).

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
      Done for the folder lock: packages by `ReleaseId`, overrides by key,
      edges in canonical edge order, surfaces as canonical sets, every digest
      once, strict decoding (`lock.order`, `lock.shape`). Times, Merkle proofs
      and structured resolution proofs belong to the Milestone B lock.
- [x] Implement `osprey.local.toml` as a package-key overlay and
      `osprey.local.lock` as its graph selection. Apply an override globally to
      every active edge; never retain a published candidate for the same key.
      `packages/manager/cli/lib/overlay.osp` reads and renders the `[overrides]` table
      (`path`, `epoch`, optional `baseline`); `lock --local` removes every
      published candidate of an overridden key, adds the checkout as the one
      candidate (its source digest, overlay epoch, baseline ordinal, surface and
      dependencies) so every edge resolves to it, and writes
      `osprey.local.lock` with the `overrides` member; `osprey.lock` never sees
      an override (`overlay_tests`).
- [x] Resolve root Runtime/Build/Test plus non-root Runtime/Build edges; exclude
      every non-root Test edge and apply one-key uniqueness to their union.
      `Clauses::rootClauses` fires every root edge, `edgeClauses` only the
      Runtime/Build edges of candidates (`ManifestModel::activeEdges`), and
      `uniquenessClauses` admits one release per key across epochs; the lock
      stores each package's Runtime/Build edges (`graphs/test-edge-not-activated.json`,
      `lockfile_tests`).
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
      Done: `build --release` refuses any override, verifies every pinned
      digest and surface, and a duplicate key is `lock.duplicate-key`; the
      release lock holds registry `ReleaseId`s only. Open: nearest-ancestor
      root selection and multi-root repositories (`--project` names the root).
- [ ] Add a per-user content-addressed cache partitioned by hash algorithm and
      digest; verify on every ingress and before materialization.
- [ ] Materialize package graphs read-only through staging plus atomic rename;
      never run package content during fetch, install, build-plan creation, or
      activation.
- [ ] Make bare `build`, `run`, `test`, and `docs` always select the live-root
      development lock without network or resolution; only `build --release`
      selects the release lock after an empty-overlay check.
      Done for `build`: while the overlay is active a bare `build` selects
      `osprey.local.lock` (`build.no-local-lock` with the `lock --local`
      remediation when it is missing) and compiles overridden keys from their
      checkouts, and `build --release` refuses any override with
      `build.overrides-active`; without an overlay `build` still reads
      `osprey.lock`, which a development lock with no overrides equals byte for
      byte. `run`, `test` and `docs` are not package commands yet.
- [ ] Require unexpired TUF state for resolve/fetch/activate; allow a cached
      verified graph to rebuild after expiry only with an explicit stale-
      freshness/non-activation report, and confine known revoked bytes to
      capability-denied `--audit-revoked` reproduction.
- [ ] Implement cache locks, concurrent fetch deduplication, interrupted-write
      recovery, garbage collection from lock roots, and corruption self-healing.
- [x] Add `fetch`, `verify --offline`, `tree`, `why`, and `doctor` in Osprey with Osprey host-effect handlers
      with stable machine-readable output beneath concise terminal rendering.
      `packages/manager/cli/lib/{fetch,install,verify,tree,why,doctor}.osp` over
      `registry_host::Host`; `tree` prints the locked graph indented with
      `(build)` and ` *` marks, `why @scope/name` every edge chain from the
      root, both from `osprey.toml` and `osprey.lock` alone
      (`tree.no-lock`, `why.not-locked`, `why.no-path`). The machine-readable
      layer is the stable code-first lines every command prints; a JSON
      rendering is the Phase 4 "stable JSON" item. `packages/manager/cli/tests` 56/56.
- [x] Pin every locked release's capability surface in `osprey.lock` and fail
      `fetch` and `build` with `capability.surface` when an installed tree's
      recomputed surface differs (`[PACKAGE-CAPABILITY-CONFINEMENT]`).
      `packages/manager/core/src/surface.osp` scans the verified bundle's source for
      the ambient builtins (outside comments and string literals, whole
      identifiers, `http*`/`websocket*` by prefix; the table equals
      `fixtures/capabilities/builtins.json`) and adds the `[system.*]`
      capabilities; `lock` writes the set as `surface` on each lock package
      (`Lockfile`, canonical set or `lock.shape`); target-less `fetch`
      (`Install`), `verify --offline` and therefore `build` recompute it from
      the installed files and report `capability.surface @scope/name`.
- [x] Harden the client to `[PACKAGE-HARDENING]` C1–C8: the lock decoder
      applies the name and digest grammars (`lock.shape`); `lock` refuses
      `lock.release-replaced` and `lock.release-vanished` (the latter overridable
      with `--accept-removals`, which the refusal names) and a current lock
      that does not decode; every printed line passes the control-byte
      sanitizer; derived-project strings are TOML-escaped and compiler
      invocations shell-quoted; each probe of the spec's table is a named test.
      The 128 MiB body bound (`Bundle::sizeCode`, `bundle_tests`); the lock
      grammar (`Lockfile::keyOf`, `digestMember`; five grammar cases in
      `fixtures/lock/rejections.json`); the sanitizer (`Reports::printable`,
      wired in `cli/src/main.osp`, `reports_tests`); TOML escaping
      (`Build::quote`) and shell quoting (`Host::shellWord`, `build_tests`);
      the lock refusals (`packages/manager/cli/lib/lock.osp`, `lock_tests`).
- [x] The `lock` command of the folder registry (`[PACKAGE-FOLDER-REGISTRY]`
      "Folder lock", `[PACKAGE-RESOLUTION]`): one `GET /v1/packages`, every
      listed release fetched and verified through the same path as `fetch`,
      each manifest parsed (a rejected one excluded with
      `lock.invalid-manifest`), `Resolve::solve` run, `osprey.lock` written
      through `Lockfile::encode` with `+`/`=`/`~`/`-` lines; an Explain
      outcome prints its code and keys and writes nothing. `build` compiles
      each locked package through its own `source_roots` and `python_sources`
      (`build.manifest` when an installed manifest is rejected). Every command
      parses `--registry`/`--project`/switches through one `Flags` module and
      the suites share one `Fixture` module (the first draft duplicated the
      argument loop in four commands and the fixture handlers in five suites).
      `packages/manager/cli/tests` 56/56; `packages/manager/e2e` runs the real flow
      (`lock`, `fetch`, `verify --offline`, `build --run` on `e2e/consumer/`,
      lock bytes pinned in `fixtures/lock/consumer.json`).
- [x] Consume the folder lock: target-less `fetch` installs every locked
      release through the verified download and skips a tree whose digest
      matches; `verify --offline` reports `verify.no-lock`,
      `verify.manifest-digest`, `verify.dependencies`, `verify.missing`,
      `verify.digest` or `verified N packages`; `build [--run]` verifies, writes
      the derived project under `.osprey/build/` and runs the compiler.
      `packages/manager/cli/lib/{lockfile,install,verify,build,project}.osp`;
      `packages/manager/cli/tests` 31/31, `packages/manager/e2e` 30/30, `fixtures/lock/`
      (`[PACKAGE-FOLDER-REGISTRY]` "Folder lock").
- [x] Drive uv for the Python interop plane (`[PACKAGE-PYTHON-UV]`): `lock`
      composes `.osprey/python/pyproject.toml` from the root and every locked
      `python/interop` release by path source, runs `uv lock` and pins both
      digests under `root.python`; `fetch` runs `uv sync --frozen`;
      `verify --offline`, `build` and `run` regenerate, compare and run
      `uv lock --check --offline`, never syncing. Unit tests drive the uv
      invocations through a fixture `Process` handler; the end-to-end case
      uses uv when available; without it, the end-to-end probe asserts
      `python.uv-lock-failed status 127` and that no Osprey lock is written.
      `doctor` reports uv's path or installation guidance.

Exit gate: a fetched release graph rebuilds offline byte-for-byte with one node
per key; local source can switch both directions without manifest edits; process
termination at every write boundary leaves either the old graph or the new one.

### Phase 3 — Complete Osprey resolver

- [x] Implement candidate normalization and hard-clause generation for
      closure, epoch/minimum ordinal, target, capabilities, trust state and
      release uniqueness: `packages/manager/resolver/src/candidates.osp` (eligible,
      yanked-but-current, target, granted capabilities, with exclusion reasons)
      and `clauses.osp` (root edges, candidate Runtime/Build edges, per-key
      at-most-one, key-level acyclicity, and support clauses so no package
      outside the closure is selected). The clauses carry no origin tags yet
      (see the explanation item below). Lives beside the engine in
      `packages/manager/resolver/` rather than `core/` for the type-check-cost reason
      recorded in the README. Key and identity equality, ordering and the
      blank fallbacks are the single `Names`/`Identity` definitions; the
      first draft duplicated them in four modules.
- [x] Feed ambient capability surfaces and the root's per-dependency grants
      into the capability clauses, so an ungranted `io/network` release is not
      a candidate and `resolution.capability` names the key and capability;
      add the fixture graphs (`[PACKAGE-CAPABILITY-CONFINEMENT]`). `lock`
      gives each candidate its recomputed surface (`Surface::ofRelease`);
      the manifest grammar accepts `capabilities = [...]` on a dependency
      entry (`ManifestDeps::grants`, `manifest.dependency.capability`,
      canonical `grants` in the manifest JSON and spec 0031); the graph
      carries `grants` and `Candidates::capsOk` admits a capability granted to
      every key or to that release's key, never transitively. Fixtures:
      `graphs/grant.json` (granted, locked) beside `graphs/capability.json`
      (ungranted, explained); `cli/tests` lock suite drives both through a
      release that names `readFile`.
- [x] Implement deterministic CDCL with two watched literals, first-UIP
      learned clauses, backjumping, assumption cores and proof tracing:
      `packages/manager/resolver/src/sat*.osp`, `packages/manager/resolver/tests` 8/8 in 18 s
      and 1.0 GB (trivial, oracle, pigeonhole php(3,2)/php(4,3) with core and
      implied-learned-clause checks, learning, assumptions, determinism,
      rejects, 200-variable random 3-SAT). Restarts and clause minimization are
      deliberately absent (recorded in the module comment); the engine learned
      asserting literals one level too deep and relearned the same clause
      without bound until fixed.
- [x] Implement lexicographic partial-MaxSAT optimization in the exact order in
      `[PACKAGE-RESOLUTION]`, including its integer cost tuple and canonical final
      graph vector; freeze every earlier component before optimizing the next.
      `packages/manager/resolver/src/optimize.osp`: sequential-counter "at most k" with
      linear search over the components the folder registry has (changed
      identities, package count, lag; `V`, `D`, `U`, `E` are zero without a
      trust plane), then the package-tuple tie-break through `Sat::assume`.
      `Resolve::solve` matches all 14 graph fixtures byte for byte and agrees
      with the exhaustive oracle on each (`packages/manager/tests` 335/335). The graph
      suites were vacuous before this work (`test` ignores its thunk's result
      and the fixture decoder read the wrong JSON level); both fixed.
- [x] Derive minimal conflict cores and human explanations from learned
      incompatibilities; never synthesize explanations after the fact. Every
      hard clause records the package keys it is about
      (`Clauses::Problem.mentions`); on a refutation `Resolve` takes the
      solver's UNSAT core, minimises it by deletion (a clause whose removal
      keeps the formula refuted is dropped, one solve each) and hands
      `Explain::fromCore` the keys the minimal core mentions, so the packages
      an explanation names come from the proof; the stable code is the first
      of the spec's causes that holds for one of those keys. The oracle keeps
      the reachability path (`Explain::of`); both agree on all 14 fixtures
      (`packages/manager/tests` 335/335). Open: a `why` for a refutation that prints
      the core's clauses themselves rather than the keys.
- [ ] Evaluate every time-dependent objective at the signed catalog `asOf`, not
      the host clock, and include it in deterministic inputs. The folder
      registry carries no time-dependent component (`V`, `D`, `U`, `E` are
      zero, `Objective::overdueDays` takes its milliseconds as an input) and
      the resolver never reads a clock; the `asOf` input arrives with the
      signed catalog of Milestone B.
- [x] Property-test soundness, bounded completeness, determinism, safe
      downgrade, global one-key/one-source uniqueness, overlay replacement, and
      objective ordering; differentially test against an independent exhaustive
      Osprey oracle on small graphs. `packages/manager/tests/src/graph_gen.osp` seeds 60
      small graphs deterministically (one to three keys, one or two releases
      each, random Runtime edges including cycles, random floors);
      `graph_tests` asserts the resolver's lock equals the exhaustive oracle's
      byte for byte and that both refute the same graphs (soundness and
      bounded completeness, since the oracle enumerates every assignment; a
      refutation's code and packages differ by design when a graph has several
      causes: the resolver reports the cause its minimal core used and that
      core's keys, the oracle the first cause in spec order over every key),
      and that reversing the catalog
      and the root edges changes no outcome (determinism; `Resolve::solve`
      canonicalises its input first, which the generated graphs showed was
      needed for core-derived explanations). Safe downgrade, uniqueness,
      overlay replacement and objective ordering are the named fixtures
      `downgrade`, `revocation`, `technical-lag`, `tie-break` and the
      overlay suite's locks.
- [ ] Benchmark a representative 100,000-release catalog after metadata load:
      ordinary add/update p95 below two seconds and adversarial unsat below ten
      seconds on the CI reference machine. Performance work cannot weaken
      completeness or determinism.

Exit gate: no valid bounded graph is missed, invalid graph is emitted, or input
permutation changes a lock or explanation.

### Phase 4 — Simple transactional CLI

- [ ] Add `login`, `package init`, `add`, `remove`, `lock`, `update`, `use`,
      `fetch`, `tree`, `why`, `audit`, `verify`, `doctor`, `publish`, and `yank`
      as Osprey modules in `packages/manager/cli`, sharing `packages/manager/core` and `packages/manager/host`.
      Done as Osprey modules under `packages/manager/cli/lib`: `package init`, `add`,
      `remove`, `lock`, `update`, `use`, `fetch`, `tree`, `why`, `verify`,
      `build`, `doctor`, and `publish --folder <registry-root> [--breaking]`,
      the folder-registry publish: the release tree (manifest, source roots,
      python sources, declared assets) is checked and digested exactly as the
      server will and placed at the next ordinal of the newest epoch, or the
      next epoch, or `1.1` (`publish_tests`). Open: `login`, `audit`, `yank`
      and the authenticated `publish` of the trust plane (Milestone B);
      `Dispatch` answers them with the not-implemented status.
- [ ] Make `add`, `remove`, `lock`, and `update` show dependency, capability,
      vulnerability, and epoch deltas before one atomic manifest/lock update.
      Dependency deltas (`+`, `=`, `~`, `-` per package) and epoch deltas
      (`epoch @scope/name eN -> eM` from `update --breaking`) are printed;
      capability and vulnerability deltas wait for the surfaces and the
      advisory feed. `Transaction::commit` writes the
      manifest, relocks, and writes the previous text back when the lock fails.
      This is rollback on a reported failure; crash-safe atomicity remains
      blocked on filesystem rename and fsync support.
- [ ] Make `add`, `remove`, and `update` atomically recompute the registry-only
      base, rebase matching per-key baselines, validate the overlay graph, and
      write release/`publishedBaseLock` only for a complete registry solution.
      Retain an override while its key remains transitively reachable.
      The registry-only recompute and the complete-solution-only write are
      done (`packages/manager/cli/lib/{transaction,add,remove,update}.osp`,
      `transaction_tests`); baselines and overlays do not exist yet.
- [x] Make `add @scope/name` choose the current eligible epoch and
      `--epoch N` choose an older contract lineage; write exact digest pins only
      to the lock. `Add` takes the key's newest listed epoch, or `--epoch N`,
      and that epoch's newest ordinal as the manifest floor; the digest goes
      only to the lock (`add.exists`, `listing.unknown-package`,
      `listing.unknown-epoch`).
- [x] Make `add <path>` infer the key, write the canonical dependency with
      a published baseline ordinal and exact per-override `ReleaseId` even when no
      full base exists, or floor zero/epoch 1 when unpublished (`--epoch N` selects
      another lineage); after publication, `lock` establishes its first baseline.
      `Add` takes the key from the checkout's `[package]`, writes
      `{ epoch, min_release }` with the published baseline's epoch and ordinal
      (or epoch 1 / `--epoch N` and floor 0 when nothing is published,
      printing `development-only`), relocks the release lock only when a
      baseline exists, then maps the checkout through `Use::mapLocal` and
      relocks the development lock; a failed development relock writes the
      manifest and the release lock back (`overlay_tests` "add <path>").
- [x] Make ordinary `update` remain inside epochs and `update --breaking`
      evaluate newer epochs one by one with a generated migration report.
      `Update` relocks with the current lock's choices non-sticky
      (`Lock::relock(fresh)`), so a key moves to its epoch's newest release and
      prints `~`; `--breaking` retargets each root dependency with a newer
      listed epoch to that epoch at floor 1 and prints one
      `epoch @scope/name eN -> eM` line per move before the lock's delta. It
      moves to the newest epoch directly rather than one epoch at a time; the
      per-epoch walk belongs with the migration report of the trust plane.
- [x] While overrides are active, make `update` re-resolve their published base,
      atomically replace matching baselines, then revalidate locals; only
      `update --breaking` may change an override epoch, and failure changes none.
      `Update::overlayUpdate`: the release lock is relocked first, each
      mapping's baseline becomes the base's node for its key (printed as
      `baseline @scope/name <old> -> <new>`), only `--breaking` moves a
      mapping's epoch, and the development lock is relocked through the
      overlay transaction; a failed development relock restores both the
      overlay and the previous release lock (`overlay_tests`
      "update rebases an active overlay").
- [x] Make `use <path>` infer the package key/current epoch/baseline, apply it
      globally, and resolve a local lock; make `use <key> --published` remove it
      and `use --list` expose replacements; `use --clear` restores all or none.
      `packages/manager/cli/lib/use.osp`: the key comes from the checkout's
      `[package]` (`use.no-package`) and must already be in the graph
      (`use.not-a-dependency`; a new key goes through `add`); the baseline is
      the development lock's published node, else the release lock's, else the
      newest listed release of the epoch in force; the overlay and the
      development lock change in one `Transaction` (a failed relock restores
      the overlay). `--published`, `--list` (with each checkout's current
      source digest) and `--clear` as specified.
- [x] Make `lock` release-only and `lock --local` development-only. Capture the
      exact pre-override lock/ReleaseIds; switching back restores that baseline,
      never an opportunistic update, and preserves the overlay on revocation.
      `lock` ignores the overlay and writes `osprey.lock`; `lock --local`
      writes only `osprey.local.lock`; each mapping records its baseline
      `ReleaseId`, and the development lock's resolver keeps the current
      development lock's choices sticky, so switching back does not move other
      keys. Revocation handling waits for the trust plane.
- [x] Make `build --release` ignore no error: any override, duplicate key,
      unpinned source/system/toolchain input, mismatch, or fallback is fatal.
      Any override is `build.overrides-active` before anything is read from a
      checkout; a duplicate key is `lock.duplicate-key` at decode; every
      digest, surface and manifest pin is verified first and any mismatch
      stops the build; nothing resolves or falls back. Toolchain and system
      inputs are not pinned by the folder-registry lock (Milestone B).
- [ ] Snapshot-test terse success, offline, auth, quarantine, revocation,
      conflict-core, missing-system-capability, and recovery diagnostics.
      Exact-line assertions exist for terse success (`+`/`=`/`~`/`-` lines,
      `verified N packages`, `added`/`removed`/`published`), offline
      (`verify.*`), conflict (`resolution.*` with the keys the minimal core
      names), capability (`resolution.capability`, `capability.surface`) and
      recovery (`transaction.restore`, `lock --local` remediation). Auth,
      quarantine and revocation diagnostics wait for the trust plane.
- [x] Expose stable JSON for editor/automation integration without creating a
      second semantic API. `osprey-pkg --json <command> …` prints the command's
      report as one RFC 8785 line, `{"lines":[…],"status":N}`
      (`Reports::json`, routed by `Dispatch`): the same value the terminal
      shows, sanitized by the same path, with the same status
      (`dispatch_tests` "--json rendering").

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

### Phase 6 — Hosted Supabase data plane behind the same Osprey API

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
- [ ] Compute and sign each release's capability surface at publication and
      serve it in catalog metadata, so a client never fetches an excluded
      candidate (`[PACKAGE-CAPABILITY-CONFINEMENT]`).

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
      confidence prior from spec 0032 in integer-only `packages/manager/core`; version
      and sign every calculation. `registry::Arithmetic` holds the canonical
      `roundDiv`/`higher`/`lower`/`ratio`/`weighted`/percentile primitives with
      fault-on-domain-violation; the methodology itself is not started.
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
      against a second independent Osprey implementation and the signed basis-point golden vectors.

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

- [ ] Verify `[PACKAGE-IMPLEMENTATION-LANGUAGE]` across the complete delivered source and build graph: all package-system executable implementation, tests, adapters, and operational tooling are Osprey; no package-specific Rust/C/JavaScript/Python component or launcher is present.
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

## Deferred — CI integration

Lowest priority. Start only once the package system has stabilised. Until then
every registry suite runs through `make registry-test` and stays out of
`make test` and CI.

- [ ] Run the end-to-end folder-registry demo, a consumer project building
      against a fetched package, in CI on loopback.
- [ ] Add the registry suites to `make test` once they type-check quickly.
- [ ] Make schema/fixture validation a fail-fast `make test` stage. Today the
      type checker's effect-row solver runs for minutes on the TOML parser.

## Fixed risk controls

| Risk | Required control |
| --- | --- |
| Package manager bootstraps itself | The entire package application, including CLI and host effects, is checked-in Osprey built by the existing compiler; no non-Osprey package bootstrap is introduced. Its released source later enters the same pipeline. |
| Required capability is unavailable in Osprey | Record the blocker with an Osprey regression case and resolve it through Osprey source or an existing platform interface. Never substitute package implementation in another language or weaken a security gate. |
| Solver complexity or slowness | Complete CDCL/MaxSAT remains mandatory; indexing, incremental reuse, and objective encodings optimize it. There is no greedy mode. |
| Supabase compromise or lock-in | Clients trust TUF/log/digests; all authoritative objects are reconstructable and storage/API are replaceable. |
| AI false positives or evasion | Real-prior gates, abstention, drift demotion, quarantine, independent appeal, and no AI-only authority. |
| New-package cold start | Neutral maintenance prior plus security-gated exploration and exposure debt, never fabricated quality history. |
| Vulnerability disclosure bias | Raw vulnerability count has zero weight; unresolved reachable risk and response/propagation speed are scored. |
| Native ecosystem pressure | System capabilities and explicit provider guidance; no native code or installer execution enters the registry. |
| Malicious code inside a dependency | Capability confinement: a published package reaches no file, process, network or terminal except through effects the consumer handles or capabilities the root granted; the resolver excludes ungranted releases, and fetch and build re-scan every installed tree. A compiler check that refuses ambient builtins in dependency roots is requested as a compiler issue, never implemented in the registry. |
| Python plane drift | uv owns Python resolution and installation; `osprey.lock` pins the generated uv project and `uv.lock` by digest, `build`/`run` verify offline, and the registry never carries a Python package. |
| Local override leaks into release | Separate development overlay/lock, prominent diagnostics, release-mode rejection, exact workspace digests, and one-key/one-source validation. |
| Forge/tag mutation | Dual-authority repository binding, publisher-computed digest, frozen CAS challenge, final signed authorization, and permanent log. |
| Key compromise | 3-of-5 offline root, 2-of-3 release roles, independent custody/control planes, witnessed rotation, and client version floors. |

## Definition of done

Every executable component delivered by this plan is implemented in Osprey, including the CLI, host effects, cryptography, adapters, tests, and operational tools. A mixed-language package implementation does not satisfy this plan.

The work is complete only when an unknown new publisher can publish a small
Osprey library from a reviewed forge commit with one command; a fresh client can
resolve it deterministically, verify every trust edge, install without executing
package code, switch a transitive dependency to a local checkout and back without
editing its manifest, reproduce the exact one-version-per-key release build
offline, understand every conflict and score, and find it through security-gated
fair exploration. The same exercise must survive a malicious mirror, compromised
database, revoked release, expired online key, failed builder, and full Supabase
loss without accepting different bytes or silently weakening policy.
