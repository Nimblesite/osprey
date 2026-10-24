# Osprey package system

The registry and package client are written in Osprey and built by the existing compiler. [Plan 0020](../docs/plans/0020-package-manager.md) tracks delivery against [specs 0029–0032](../docs/specs/ecosystem/README.md). The folder registry, source verification, deterministic resolution, locks, local overrides, folder publication and uv integration run today. Authenticated public publishing, the dedicated package website and trust services remain planned.

The loopback consumer uses Default-flavor source. Package-client builds with declared `python_sources` still have a manifest-path integration gap; frontend compilation and publication tests do not prove that path. See the [package guide](../website/src/docs/package-manager.md) for the exact current limitation.

## Layout

| Directory | Responsibility |
| --- | --- |
| `core/` | Names, identities, canonical JSON/source rules, SHA-256, capability surfaces and the implementation-language gate |
| `host/` | File, tree, HTTP, process and console effects with native handlers |
| `api/` | Startup catalog, pure HTTP router and folder-registry listener |
| `cli/` | Manifest/lock commands, verified installation, local overrides, folder publication, build preparation and reports |
| `manifest/` | TOML parser and manifest validation |
| `resolver/` | CDCL solver and deterministic package graph/objective selection |
| `tests/`, `cli/tests/` | Core/API and client suites, including fixture-driven source, graph, lock, capability and Python-plane checks |
| `fixture-check/` | Schema, canonical-order and specification-reference checks for declarative fixtures |
| `sample-registry/` | Example release trees |
| `e2e/` | Loopback server/client demo and Default-flavor consumer |
| `blockers/` | Explicit probes for incomplete host capabilities |
| `../schemas/package-registry.td` | Canonical domain model shared with spec 0031 |

These projects compose sibling modules through explicit `source_roots`. Registry suites are ordinary modules called by their project's `src/main.osp`; use the commands below rather than assuming test-file discovery selects them.

## Run a registry

From the repository root:

```sh
make build
./target/release/osprey build registry/cli -o target/osprey-pkg
make registry-serve
```

The server reads `sample-registry/` and listens on `http://127.0.0.1:8787`. Leave it running. In another terminal:

```sh
curl http://127.0.0.1:8787/v1/packages
echo "lock --project $PWD/registry/e2e/consumer --registry http://127.0.0.1:8787" | ./target/osprey-pkg
echo "fetch --project $PWD/registry/e2e/consumer --registry http://127.0.0.1:8787" | ./target/osprey-pkg
echo "verify --offline --project $PWD/registry/e2e/consumer" | ./target/osprey-pkg
echo "build --run --project $PWD/registry/e2e/consumer" | ./target/osprey-pkg
```

Every client command prints `status: N`; require `0`. The frontend reads one command from stdin because argument-vector and process-exit-status builtins are unavailable. Prefix a command with `--json` for a structured report. The [command reference](../website/src/docs/package-manager.md#commands) covers local overrides, publication and update behavior.

A release directory is `@scope/name/<epoch>.<ordinal>/`. To serve another root, change [api/registry-server.json](api/registry-server.json), for example:

```json
{ "root": "/srv/osprey-packages", "address": "0.0.0.0", "port": 8787 }
```

The catalog is loaded at startup; restart the server after folder publication. Current filesystem handlers use POSIX `find`, `mkdir` and shell redirection, so an Osprey source implementation alone does not establish Windows host support. The second-machine acceptance probe is still open.

## Verification

```sh
make registry-test
make registry-blockers
```

`registry-test` runs fixture validation, manifest, client, core/API and loopback suites. `registry-blockers` deliberately asserts capabilities that are not delivered; it is expected to fail. `blockers/src/main.osp` includes binary reads and a historical million-element fallback/pipeline reproducer. A probe's current result must be measured before calling it a present compiler crash.

Source-digest fixtures contain independently computed goldens. The resolver is checked against bounded graph fixtures and an exhaustive search oracle; candidate and explanation helpers are shared, so that oracle independently checks search/optimal selection rather than every supporting rule. This is not the planned 100,000-release performance acceptance.

## Host capabilities and current limits

[core/src/capabilities.osp](core/src/capabilities.osp) supplies `doctor`'s inventory. File text I/O, HTTP, process execution, stdin, lists/maps and byte-level string access are available. Remaining host gaps include argv, exit status, environment access, binary-safe file reads, directory/stat/atomic-rename/fsync builtins and a UTC clock. File-tree handlers currently shell out; rollback after a reported failure is not crash-safe activation.

Bitwise builtins (`bitAnd`, `bitXor`, shifts and related operations) exist. The inventory regression checks that these are absent from the blocked list. [core/src/sha256.osp](core/src/sha256.osp) still uses arithmetic/nibble lookup tables; that implementation choice no longer proves the language lacks bitwise operations.

Verified language boundaries relevant to these suites:

- `readFile` stops at the first NUL; a five-byte `a`, NUL, `b`, `0xff`, `c` probe currently has `byteLength` 1. Folder source bundles are text, not a binary asset format.
- Default syntax rejects field renaming such as `Success { value: answer }` and qualified bare variant patterns such as `Shapes::Dot =>`. Existing helpers avoid those forms.
- `checkAll`/`expectAll` require a list literal. A computed boolean list is rejected at code generation; assert its folded result explicitly instead.
- `==` on lists is rejected rather than comparing identity. `structuralEquals([1, 2], [1, 2])` performs the explicit deep comparison and returns `true`.
- Tail-call optimization is not implemented. Avoid treating an old stack-depth measurement as a stable recursion limit.
- A test callback's return value is not an assertion; use the test framework's checks. Keep expected output and assertion results separate.

## Security gates that run today

The table links the implemented folder-registry checks to their tests. Runtime request bounds are separate from registry tests. These checks do not establish publisher identity or complete the hosted trust contract; [SECURITY.md](../SECURITY.md) is the disclosure policy.

| Gate | Where | Test |
| --- | --- | --- |
| A response is a pure function of method, path and the startup catalog; no request touches the filesystem | `api/lib/router.osp` | `router_tests` "hostile probes": traversal, `%2e%2e`, `%40scope`, `//v1` all 404; `HEAD`/`PUT`/`DELETE` 405 |
| `Allow: GET` on 405; `nosniff`, `default-src 'none'`, `no-referrer` on every response; `immutable` caching only on digest-addressed bodies | `Router::hardeningHeaders`, `Server::headersOf` | `router_tests` "hardening headers" |
| Request bounds: 1 MiB body, 32 KiB headers, 5 s | the runtime (`compiler/runtime/http_server_internal.h`) | recorded as S4; not a registry test |
| 128 MiB bundle bound before parsing | `Bundle::sizeCode` | `bundle_tests` |
| Strict JSON: depth 64, duplicate keys, surrogates, control bytes, UTF-8 | `core/src/json.osp` | `json_tests` |
| Path grammar, kind gate, LFS pointers, 10 000 files / 50 MiB | `core/src/paths.osp`, `source.osp`, `release_files.osp` | `paths_tests`, `fixtures/folder/refusals.json`, `fixtures/malicious/` |
| Recomputed `SourceDigest` equals the claim and the request before any write | `Bundle::verify`, `Fetch` | `bundle_tests`, `cli/tests` fetch suite |
| Install directory derived from the verified identity only | `Fetch::installDir` | `cli/tests` fetch suite |
| Lock scopes, names and digests pass the name and digest grammars (`lock.shape`) | `Lockfile::keyOf`, `Lockfile::digestMember` | `tests/fixtures/lock/rejections.json` (traversal scope, bad name, traversal and short digests, non-sha256 manifest digest) |
| Every printed line has control bytes replaced by `?` | `Reports::printable`, `cli/src/main.osp` | `cli/tests` reports suite |
| Derived `osprey.toml` strings TOML-escaped; compiler invocation shell-quoted | `Build::quote`, `Host::shellWord` | `cli/tests` build suite |
| Locked builds never resolve; relocking checks registry continuity: a re-served digest is `lock.release-replaced`, a dropped locked release is `lock.release-vanished` unless `--accept-removals` (which the refusal names), and a current lock that does not decode is its `lock.*` code; each refusal writes nothing | `Lock` | `cli/tests` lock suite: exact lock bytes, `+`/`=`/`~`/`-` lines, every refusal by name |
| A candidate whose manifest is rejected is excluded with `lock.invalid-manifest`; a locked package whose installed manifest is rejected stops `build` with `build.manifest` before the compiler runs | `Lock`, `Build` | `cli/tests` lock and build suites |
| A published source that declares a builtin's name with `fn`, `let` or `mut` is refused with `source.shadowed-builtin`, so a declaration cannot hide a call from the surface scan | `Surface::declaredBuiltins`, `ReleaseFiles::treeCode` | `surface_tests` "shadowed builtin names" |
| Capability confinement: each release's surface (the ambient builtins its source names, outside comments and literal string text, including interpolation expressions, plus its `[system.*]` capabilities) is recomputed from the verified bundle; a release whose surface the root manifest does not grant on that key (`capabilities = [...]` on the dependency entry) is not a candidate (`resolution.capability`); the surface is pinned per package in `osprey.lock` and target-less `fetch`, `verify --offline` and `build` fail with `capability.surface` when an installed tree's recomputed surface differs | `core/src/surface.osp`, `Lock`, `Install`, `Verify`, `Candidates::capsOk` | `surface_tests` (table equals `fixtures/capabilities/builtins.json`), `graphs/grant.json`, `cli/tests` lock and verify suites, `fixtures/lock/rejections.json` surface cases |
| Python plane (P1–P3): the generated `.osprey/python/pyproject.toml` names no index and pins `index-strategy = "first-index"`; a member's own `pyproject.toml` may name no `[[tool.uv.index]]` (`python.index-forbidden`) and no `git`, `url` or outside-tree `path` source (`python.source-forbidden`); every sdist and wheel in `uv.lock` carries a hash or `lock` refuses it (`python.uv-lock-hashes`); each refusal runs no uv and writes no lock; sync is `uv sync --frozen`, verification `uv lock --check --offline` | `PythonPlane` | `cli/tests` python suite |

Named tests above cover the folder-registry subset. Python publisher-attestation identity checks and the public trust plane remain unfinished.
