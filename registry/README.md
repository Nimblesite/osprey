# Osprey package system

The package manager and registry from [plan 0020](../docs/plans/0020-package-manager.md) and specs [0029](../docs/specs/ecosystem/0029-PackageManagement.md)–[0032](../docs/specs/ecosystem/0032-PackageEvidenceAndDiscovery.md), written entirely in Osprey and built by the existing compiler (`[PACKAGE-IMPLEMENTATION-LANGUAGE]`). **All Osprey, and a working API that serves working packages comes first.** The active work is plan 0020 Milestone 0: the database-free folder registry of spec 0029 `[PACKAGE-FOLDER-REGISTRY]`, served by `api/` and fetched by `cli/`. The sections below say exactly what runs today.

## Layout

| Directory | Namespace | Contents |
| --- | --- | --- |
| `core/` | `registry` | Pure rules shared by every frontend: package names, release identities, digest envelopes and `H(d, x)`, the canonical source-tree grammar and `SourceDigest`, a pure-Osprey SHA-256, byte encodings, canonical ordering, the arithmetic policy, the language-conformance gate and the blocked-capability inventory. No I/O. |
| `host/` | `registry_host` | The `Host` effects (`Files`, `Tree`, `Web`, `Process`, `Console`) and their native handlers built on Osprey builtins. Handlers carry no policy. |
| `api/` | `registry_api` | The folder-registry HTTP server: configuration, catalog loading, the pure `Router`, and the listener. |
| `cli/` | `registry_cli` | Command routing and the commands themselves: `help`, `doctor`, `fetch <target>`, target-less `fetch` (install the lock), `verify --offline`, `build [--run]`; `lock` is in progress. `src/main.osp` is the executable entry and sanitizes every printed line; `lib/` holds everything tests assemble. |
| `cli/tests/` | `registry_tests` | The frontend suites (dispatch, fetch, lockfile, install, verify, build, reports), assembled apart from `tests/` so the frontend can depend on the manifest parser without that program paying its type-check cost. |
| `sample-registry/` | — | A registry root with four real releases of three packages; `@demo/report` uses the other two. |
| `manifest/` | `registry` | The TOML parser and manifest validators, kept out of `core/` until the type checker handles the parser in seconds. |
| `resolver/` | `registry` | The CDCL engine (`Sat`), the package resolver over it (`Resolution` types, `Candidates`, `Clauses`, `Optimize`, `Resolve::solve`) and the engine's own test project; the resolver is proven against the 14 graph fixtures and the exhaustive oracle in `tests/`. |
| `tests/` | `registry_tests` | The core, host and API suites, assembled into one program. `fixtures/` holds declarative JSON fixtures whose digest goldens were computed with an independent implementation (Python `hashlib`/`json`) and are asserted byte for byte. |
| `fixture-check/` | `registry_fixture_check` | The fail-fast first stage of `make registry-test`: every fixture is data, parses strictly, cites a defined spec section and is listed by its folder's `index.json`; every typed fixture value named in `tests/fixtures/schema-map.json` is valid against `schemas/package-registry.td` and in canonical list order; every registry source cites only defined spec sections. |
| `e2e/` | `registry_e2e` | The Milestone 0 demo, last stage of `make registry-test`: builds and starts the server over `sample-registry/` on `127.0.0.1:18797`, compares every endpoint with the pure `Router` byte for byte, fetches every release by name, epoch and digest, checks the installed bytes, builds and runs `e2e/consumer/` against the fetched sources, then stops the server. |
| `blockers/` | `registry_blockers` | Probes for capabilities Osprey lacks; red by design until each lands. |
| `../schemas/package-registry.td` | — | The canonical typeDiagram model. Spec 0031 renders the same bytes and a test keeps them identical. |

Projects share source through `source_roots` (for example `cli` lists `../core/src`), because Osprey has no cross-project dependency model yet. `osprey test` compiles each `*.test.osp` in isolation and cannot import project modules, so suites are ordinary modules with an `export fn run()` called from `tests/src/main.osp`, run with `osprey registry/tests --run`.

## Run a registry

A registry is a folder. Each release is a directory `@scope/name/<epoch>.<ordinal>/` holding the package's source tree, as in `sample-registry/`. Nothing else is needed: no database, no account, no hosted service.

```sh
make registry-serve          # serves registry/sample-registry on http://127.0.0.1:8787
curl http://127.0.0.1:8787/v1/packages
curl http://127.0.0.1:8787/v1/packages/demo/maths
```

To serve your own folder on your network, edit `api/registry-server.json`:

```json
{ "root": "/srv/osprey-packages", "address": "0.0.0.0", "port": 8787 }
```

Then run `osprey . --run` in `registry/api`. At startup the server walks the root, computes each release's `SourceDigest`, prints every release it serves and every directory it refuses with the reason, and answers the endpoints of spec 0029 `[PACKAGE-FOLDER-REGISTRY]`.

Fetch from it with the package frontend, which verifies the digest before writing anything:

```sh
echo "fetch @demo/report --registry http://127.0.0.1:8787" | ./target/osprey-pkg
```

The release lands in `.osprey/packages/@demo/report/<digest>/`.

## Running

```sh
make registry-test                                   # fixture-check, manifest, CLI and core suites, then the e2e demo; TAP output
make registry-blockers                               # capability probes, expected red
./target/release/osprey build registry/cli -o target/osprey-pkg
echo "doctor" | ./target/osprey-pkg                  # one command line on stdin
```

The frontend reads its command line from standard input and prints `status: N` as its last line because Osprey has no argv or exit-status builtin yet. Both are blocked capabilities below, not design.

## Host capability inventory (plan 0020, Phase 0)

Measured against the compiler at `target/release/osprey` on macOS. `registry/core/src/capabilities.osp` is the single source for the blocked list; `osprey-pkg doctor` prints it.

| Capability | State | Evidence |
| --- | --- | --- |
| Read and write whole text files with OS failure reasons | available | `readFile`/`writeFile`; `tests/src/dispatch_tests.osp` through `Host::Files` |
| Run a shell command, capture stdout/stderr and exit code | available | `spawnProcess` + redirection in `host/src/native.osp` |
| Standard input, one line at a time | available | `input()` in `cli/src/main.osp` |
| HTTP client and server, JSON documents | available | `api/` serves and `cli/lib/fetch.osp` fetches over the runtime HTTP builtins; `e2e/` exercises both on loopback |
| Byte-level string access, 64-bit integers, lists, maps | available | `core/src/bytes.osp`, `core/src/order.osp` |
| Command-line arguments | blocked `process.argv` | no builtin; not probeable from inside a program |
| Process exit status | blocked `process.exit-status` | `main`'s value is ignored |
| Environment variables | blocked `process.environment` | no builtin |
| Binary-safe file reads | blocked `files.binary-read` | `blockers/src/main.osp`: a 5-byte file with a NUL reads as 1 byte |
| Directory listing, stat, mkdir, rename, unlink, fsync | blocked `files.directory` | no builtins; the host shells out, which Windows and atomicity rules will not accept. No atomic rename or fsync exists, so atomic activation (Phase 2) waits on this |
| UTC clock | blocked `time.clock` | no builtin |
| Bitwise operators | blocked `int.bitwise` | SHA-256 emulates them with arithmetic and lookup tables |

A blocked capability is resolved only by Osprey gaining it; no component is written in another language meanwhile (plan 0020, fixed risk controls).

## Security gates that run today

Spec 0029 `[PACKAGE-HARDENING]` lists the gates a penetration test meets; these are implemented and asserted by name. `SECURITY.md` at the repository root is the disclosure policy.

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

Still open from the same section: `lock.release-replaced` and `lock.release-vanished`, which belong to the `lock` command (plan 0020, Phase 2).

## Measured

- The CDCL core refutes the pigeonhole formula php(4,3) in five conflicts; the resolver suite (eight tests including a 200-variable random 3-SAT instance solved three times) runs in 18 s and 1.0 GB. Before the asserting literal was placed at the backjump level the same formula relearned one clause until the process exceeded 9 GB.
- `SourceDigest` of the fixture trees equals the independently computed goldens, including the digest-substitution case.
- SHA-256 in Osprey hashes 64 KiB in about 1.6 s on an Apple Silicon laptop: bitwise operations are nibble-table lookups because the language has none. Correctness is complete; throughput is the `int.bitwise` blocker's cost.

## Defects found while building (reported, not worked around in the compiler)

| Probe in `registry/blockers` | Behavior today | Needed |
| --- | --- | --- |
| `files.binary-read` | `readFile` returns the bytes before the first NUL only. | Whole-file reads so declared PNG/JSON assets can be hashed. |
| `runtime.default-operator-pipeline` | `byteAt(s, i) ?: 0` inside a `map` or `fold` over a million elements crashes (signal 11); the `match` form and a pipeline without `?:` both succeed. | `?:` must behave like the `match` it abbreviates at any iteration count. |

## Known compiler limits that shape the code

- No tail calls are guaranteed, and deep self-recursion dies with SIGBUS instead of a diagnostic. A scanner that recursed once per byte overflowed near 6,000 bytes, so `Json` and `Canonical` consume up to 256 plain bytes per call through `Bytes::runEnd` (`tests/src/json_tests.osp` parses and renders 100,000-byte strings).
- A qualified payload-free variant pattern (`Json::Null =>`, `Shapes::Dot =>`) is a syntax error outside its defining module; code matches the payload variants and `_`, or calls `Json::isNull`.
- In the assembled test program, a handler arm whose parameters go unused, or a handled body answering a `Result` from another module, is "a closure value with a still-generic type" at codegen. Fixture handlers use every parameter and answer concrete records (`tests/src/loader_tests.osp`).
- Patterns cannot rename a bound field (`Success { value: v }`); helpers take the bound value as a parameter instead.

- A recursive function with generic parameter types is instantiated once per program (issue #277). `Order::sortBy` therefore recurses over integer index lists only, and a second use of it at a different element type inside the `registry` namespace still fails to type-check (issue #278), so `Arithmetic::percentile` uses the builtin `listSort`.
- Two records sharing a field name make an untracked receiver a codegen error (issue #273); functions that read such fields spell out the record type and say so in a comment.
- `==` on lists compares identity, and lists are not assertion operands; tests compare rendered strings.
- Imports live at file level, never inside a module, and constructors from another module are qualified (`Names::Key { ... }`).
- The type checker's effect-row solver spends about 300 s of CPU on the 1,150-statement `registry/tests` program (430 s and 3.4 GB now that the resolver and its graph suites are in it: 324 tests in 160 s wall) and 660 s once the TOML parser joins it (`--check`, Apple Silicon laptop). `manifest/`, `resolver/` and `cli/` therefore keep their own test projects, and only the parser-free `ManifestModel` types sit in `core/`.
- A recursive function's return type must be written out (`codegen: … is recursive and its return type is not inferred`), as must the return type of a closure handed to `Maybe::fold`.
- `?:` binds loosest, so `"a" + f() ?: ""` is a type error; the fallback expression is parenthesized.
- A pattern binding named like a function shadows the function, and the diagnostic names the type (`cannot call non-function registry::ManifestModel::Requirement`) rather than the binding; `ManifestJson` names its helpers `requirementJson` and `kindJson` for that reason.
- A written `-> Result<T, P>` return type does not check an `Error { message }` payload against `P`; the mismatch surfaces at the caller as `cannot access field … on non-struct type string`.
- A qualified variant pattern without a binding (`Sat::Satisfiable =>`) is a syntax error outside its defining module even when the variant carries a payload; code binds the payload (`Sat::Satisfiable { model } =>`) or matches `_`.
- `checkAll`/`expectAll` accept only a list literal at codegen, not a computed list; a computed list of booleans is folded and asserted with `expectTrue`.
- `test(label, thunk)` ignores the thunk's return value: only `check`, `checkAll` and `expectTrue` assert. The graph suites were vacuous until each assertion was wrapped.
- An effectful recursive function that constructs a record whose field name another record shares (`Source::Entry.path` and `ManifestModel::Asset.path`) is refused at codegen with `quarantined field access … on an untracked receiver` (issue #273 again). `Install` reads trees recursively as `Release::File` and maps to `Source::Entry` afterwards.
