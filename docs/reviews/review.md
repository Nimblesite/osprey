# Osprey review

The one review for this repository: what is still wrong, and the evidence worth keeping. Fixed findings are deleted, not kept; earlier versions and the reports merged into this one are in git history.

Each status was checked against the tree at `8c046f20b` on 2026-11-04:

- **Open**: the defect was reproduced, or seen in the code, on that date.
- **Unchecked**: not established on that date. Treat it as a lead and reproduce it first.

Reproductions used the debug compiler on macOS on Apple silicon. Nothing here was run on Linux, Windows or Android. Paths name the code as of that commit.

**Fix these first:** [GPU helpers take the kernel's parameters on Metal](#gpu-and-staged-effects), [`Channel` construction aborts](#runtime-and-security).

## Folder migration audit [REPO-ROOT-LAYOUT-AUDIT]

On 2026-11-05, independently compared all 5,228 tracked files in `31a3aa71a^` with their expected destinations in `31a3aa71a`: none are missing, 4,631 have identical content and modes, and 597 have modified content. Line-by-line comparison of those 597 found no deletion-only blocks or replacements shortening a block by more than two lines. This inventory establishes tracked-file retention, not functional correctness or preservation of unknown, untracked edits. FastAPI's previously dirty dependency test and oracle remain present at their current paths.

- [x] Compare every pre-move tracked path and blob against the destination tree.
- [x] Review changed-file deletion blocks; inspect apparent deleted manifests at their new paths.
- [x] Repair coverage configuration keys, physical source markers, selectors, report names and fixture paths; all 42 package contracts, 45 app/entry contracts and three process-level watchdog regressions pass.

The subsequent staged split into `src/osprey/` and `src/toolchain/` was checked independently against HEAD `3b38ffc35`: all 5,232 tracked paths have staged and live destinations, 4,576 with identical blobs/modes and 656 changed. Evidence: `target/logs/resource-audit/folder-second-move-inventory.json`. This is a second retention check; coverage-path regressions have since passed the independent 87-contract suite.

Detailed local evidence: `target/logs/resource-audit/folder-move-inventory.json` and `folder-move-content-review.json`. The sole missing worktree document found during the first check, `docs/RELEASING.md`, is present again; no restoration of code was needed from this inventory.

---

## 1. Open regressions by commit

| From | Finding | Where | Failure | Status |
|---|---|---|---|---|
| `78be07ec` | HTTP externs changed from `int` to `Result` with no migration path | `src/toolchain/crates/osprey-codegen/src/extern_call.rs` | Programs calling `httpCreateServer`, `httpListen` or `httpGet` stop type-checking after upgrade, guided only by the "undecoded Result" diagnostic. | Open |
| `2691d8dc` | Windows GC caller-root gate without Windows data/BSS bounds | `src/toolchain/compiler/runtime/memory_gc_native_roots.h` | Data roots are scanned on Apple and glibc Linux only: the file has no Windows branch, so Windows globals go unscanned and the test depends on the optimizer. Spec 0018 requires stack, registers and data/BSS. Read, not run. | Open |

---

## 2. System issues not logged on GitHub

Compared by title against the open GitHub issues on 2026-11-03. [Findings an issue already covers](#already-on-github) are listed at the end.

### Runtime and security

| Severity | Finding | Where | Failure | Status |
|---|---|---|---|---|
| High | `Channel` construction aborts the process | `src/toolchain/compiler/runtime/fiber_runtime.c`; `src/toolchain/crates/osprey-codegen/src/fiber.rs`; test `fiber_runtime_tests.c` | `Channel(0)` exits 134 with `FATAL: Channel(0) has no channel to give`, against [FAILURE-EXPLICIT]. The test asserts the abort, so it must change too. | Open |
| Medium | `spawn` running out of memory answers a negative handle | `src/toolchain/compiler/runtime/fiber_runtime.c` | Fiber tables grow rather than recycle, so each fiber costs memory until exit, and exhaustion isn't a `Result` or an effect. | Open |
| Medium | WebSocket frame parser unfinished | `src/toolchain/compiler/runtime/http_shared.c` | Rejects 64-bit lengths and ignores FIN and opcode, so close and ping are handled as text. | Open |
| Medium | HTTP client's TLS handshake and request send have no total time limit | `src/toolchain/compiler/runtime/http_client_runtime.c` (`transport_secure`, `send_request`) | Only reading the response has a whole-read limit. Each read and write of the handshake and the request gets the timeout on its own, so a dripped handshake holds the client far past it. Read, not run. | Open |
| Medium | HTTP provider turns body and cleanup failures into success | `src/osprey/packages/pydantic_ai/src/providers_http.py` | `result if closed == 0 else result` has identical branches, and a failed body read returns `Success` with an empty body. | Open |
| Medium | Native store escapes through intermediate symlinks | `src/toolchain/hosts/swift/Sources/OspreyNative/StoreAdapter.swift`; `src/toolchain/hosts/winui/OspreyHost.Core/Store.cs` | With `store/link` pointing outside, a write to `link/secret` lands outside the store. Neither file handles symlinks or reparse points. | Open |
| Medium | WinUI output limit applies only after unbounded buffering | `src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs` | `ReadToEndAsync` buffers everything before the limit is checked, so a chatty child exhausts host memory. stderr is never limited. | Open |
| Medium | Compiler stack overflow on extreme nesting | `osprey --check` | 50,000 nested parentheses, 50,000 chained `+`, or 5,000 nested `[` in ML exit 134. Requires generated source. The language server was not tested. | Open |
| Medium | Deep recursion segfaults without a diagnostic | Compiled programs | Non-tail recursion to depth 1,000,000 exits 139. | Open |
| Low | `Dns.discover` interpolates `seconds` into a shell command | `src/osprey/packages/chromecast/src/dns.py` | Current callers pass constants. | Open |
| Low | Stale comment says a test pins a `join` segfault | `src/toolchain/crates/osprey-cli/tests/cli_e2e.rs` (`join_accepts_every_string_list_representation`) | The assertions below it prove the opposite. | Open |
| Unproven | TLS writes to a closed peer may raise SIGPIPE on Linux | `src/toolchain/compiler/runtime/http_client_runtime.c`; `socket_safety.h` | `osp_socket_configure_no_sigpipe` does nothing on Linux, where only `MSG_NOSIGNAL` on each send protects, and OpenSSL's socket writes don't pass it. Read, not run. | Unchecked |
| Unproven | Chromecast parser recursion | `src/osprey/packages/chromecast/src/protocol.py`; `hex.py` | Reported as a crash, but overstated: a self tail call of the same shape reached depth 20,000,000. `parse_fields` and `points_from` were not checked. | Unchecked |

**Review coverage gaps.** Nothing from the v0.28.2 reviews ran on Linux, Windows, Android or wasm. Not reviewed: GC and ARC internals, effects coroutines, JSON, GPU and media runtime, `src/toolchain/hosts/gpui`, most of `src/toolchain/hosts/winui`, the packages `signal_flow`, `uitest`, `graphics` and `json`, and most of `src/osprey/packages/pydantic_ai`. Random mutation (1,500 mutants of 79 programs) covered `--check` only. `panic!`, `unwrap` and `expect` sites in production Rust were not audited one by one.

### Language, types and effects

| Finding | Where | Failure | Status |
|---|---|---|---|
| Operation-returned function fails through a handler value | Effect analysis | `read: fn() -> fn(int) -> int` called through a handler value is refused: `program entry invokes a dynamic callable whose effect provenance cannot be proven`. Closest issue is #275. | Open |
| Nested constructor patterns unimplemented | `src/osprey/examples/proof/kernel/src/reduce.ospml` | `P(X(n))` is refused, so two-sided comparison splits into several functions. | Open |
| LSP test shadow detection is too narrow | `src/toolchain/crates/osprey-lsp/src/testing.rs` (`shadows_test_builtin`) | It looks only at functions and externs, so a value or parameter named `test` produces phantom editor tests. Needs scope-aware resolution. | Open |
| Abandoned handlers leak owned values | `src/toolchain/compiler/runtime/effects_coro.c` | A control arm that returns without resuming skipped generated cleanup: one live ARC object, 13 bytes. Code now has an abandon path; the audit is open in plan 0016. | Unchecked |
| `any` needs an ownership-carrying ABI | `src/toolchain/crates/osprey-codegen/src/lower.rs` | An erased word lowers like `int`, so no cast-site rule can be sound. Needs a tag or an explicit borrow, forward and consume model. | Recommendation |
| `Result` values in lists, maps and channels | Language | Reported unimplemented. | Unchecked |
| About 6 s per project analysis | `src/osprey/examples/projects/modules` | Predates the memo, which removed only the repeats per open file. | Unchecked |

### GPU and staged effects

| Severity | Finding | Where | Status |
|---|---|---|---|
| P1 | Named GPU helpers take caller bindings on Metal: with `let bias = 10`, `fn add(x) = wrapAdd(x, bias)` and `gpuMap(value, fn(bias) => add(bias))`, element 1 gives 11 on the host and **2** on Apple M4 Max. A silently wrong answer. OpenCL not run | `src/toolchain/crates/osprey-codegen/src/gpu_device/emit.rs` | Open |
| P0 | Static-handler monotonicity is not transitive; invalid unused arms are erased before checking | `src/toolchain/crates/osprey-ast/src/stage.rs`; `lower_static.rs` | Unchecked |
| P1 | `[GPU-KERNEL-ELEM-TYPING]` depends on expression order: `t * x * y` differs from `x * y * t` | `src/toolchain/crates/osprey-types/src/expr.rs` | Unchecked |
| P1 | Spec overstates: "each kernel" extraction, "fused" host loops, reductions that vary by backend versus byte-identical backends | Spec 0034 | Unchecked |
| P1 | Staged effects: tail `resume` allowed by spec but always rejected; discharge is per file; handler-owned static state unimplemented; only one of three falsifiers exists | Spec 0035 | Unchecked |
| P1 | `--deps` is approximate (parse errors are now reported) | `src/toolchain/crates/osprey-ast/src/stage.rs` | Unchecked |
| P1 | D3D12 bridge draws in software only, and Windows CI runs no bridge check | `src/osprey/examples/graphics/ospgfx_d3d12.c`; `ci-windows.yml` | Open |
| P1 | Wasm GPU differential non-required and absent from `make ci`; failed allocation becomes an empty buffer while loops keep the requested length; staged wasm rejection is an undefined-symbol skip | `ci.yml`; `gpu_runtime.c`; spec 0035 | Unchecked |
| P2 | Fusion claims not gated by IR goldens; purity tests miss filter, zip, scan and helper chains; `gpuFilter` loads each element twice; docs parity checks name and arity only | `src/toolchain/crates/osprey-codegen/src/gpu.rs`; `builtin_docs_lang.rs` | Unchecked |
| P2 | `[GPU-IOTA]` omits `n <= 0`; `[GPU-SCAN]` omits how `initial` participates; `[GPU-KERNEL-ELEM-TYPING]`, `[GPU-KERNEL-FORM]`, `[GPU-ROADMAP]` and `[GPU-RESEARCH]` lack references | Spec 0034 | Unchecked |
| P2 | No benchmark artifact for the July numbers; proved-unique free has no direct test; graphics drift guard checks fragments, Metal-to-HLSL only | `graphics_scenes.rs` | Unchecked |
| | Spec 0034 calls any handled effect illegal; spec 0035 accepts effects erased by static handlers. Coherent rule: no runtime effect reaches a kernel | Specs 0034, 0035 | Unchecked |

### Hunt

CI skips `clean.osp` by name (`HUNT_KNOWN_BUGS`, `scripts/make/tests.mk`), the one program proving no false positives on the hardest case; the defect itself is #306. **Open.** Remaining phases are in [plan 0034](../plans/0034-symbolic-execution.md).

**Constraints.** `--max-solver-time=30s` (forked solver) is mandatory: in-process, the solver explored 6 paths and completed 1. **Do not retry:** raw-view bounded parse, O(n) min-reduction line copy, a `gc_scan_range` alignment filter, binary-search digit length in `hunt_format.c`. All measured neutral or worse and were reverted at `f6085332`.

**Evidence, 2026-10-26.** `clean.osp` needs about 49 s of solver work against a 30 s budget; `--solver-backend=z3 --solver-optimize-divides` cut partial states from 18 to 3 (GC) and 19 to 3 (ARC). `make _test_hunt_mock` 948,770 checks, 0 failures. `make _test_hunt_crashes` 15/15. Corpus sweep of 215 programs: 168 fully explored with no findings, 47 refused (#307), 0 planted findings. `staged-rows.osp --memory=arc`: 1 path, 0 findings.

### Proof kernel and Plan 0041

| Finding | Where | Status |
|---|---|---|
| Kernel reader ignores `hints`, `safety` and `all` on `def` and `thm` lines | `src/osprey/examples/proof/kernel/src/export.ospml` | Open |
| Export corpus has no `letE`, `quot`, `opaque` or `axiom` lines to measure against | `exports/and_swap.ndjson` | Open |
| Three original core-library certificates refused at nested `Lean.Syntax`; only constructive alternatives check | `src/osprey/examples/proof/lean-core/compatible.json`, `frontier.json` | Open by record |
| No performance result for `Nat.add_comm` or Mathlib; Mathlib imports, full discipline frontend, exact-export speed gate and Workbench screenshot acceptance undelivered | Plan 0041 | Open by record |
| Twelve OpenAI lemmas are the checked-export layer, not ports of the headline results; no readable proof frontend exists | [PROOF-RUNNABLE-LIBRARY] | Open by record |
| OpenAI closures keep `propext` (stack closure also `Quot.sound`); arithmetic and injectivity proof bodies differ from upstream, statements unchanged | `src/osprey/examples/proof/openai/` | Tracked by harness |
| Workbench `imported.ospml` coverage 90.8% (2,053/2,261 lines); host interactions and installed-VSIX visual acceptance are separate gates | `src/osprey/workbench/src/imported.ospml` | Unchecked |
| Lean pin is bumped by hand; review of normalization rules is not a soundness proof | [PROOF-SPEED]; kernel | Standing |

#### Recorded proof evidence

Dated evidence for proof claims. Add new dated entries here.

**Road test, 2026-10-29.** M0 kernel: 330 Osprey lines in six files, about four hours to first green (most of it the positional-constructor gap, since fixed), longest function 19 lines. Costs: recursive functions need written signatures that the checker then flags as redundant; no nested patterns.

**Export fixture, 2026-10-29.** Lean 4.25.0 (`cdd38ac5115bdeec5f609e9126cce00f51ae88b3`), lean4export `cfefafacc917b2d14c5f9bb092ed337f71317a4a`, lean4checker `aa03950243dc18750c1546602ab054609ea8d57d`, both tag `v4.25.0`. `exports/id_imp.ndjson`: 24 lines, sha256 `2adb6bc5ff4e2fcaf9d804548fc2eaf0753902effbac3d70e1657d1b5998330d`. `Fixtures::idImp` is that text byte for byte; `idImpBad` changes line 22 from `"body":1` to `"body":2`. `scripts/proof/proof-export.sh --check` exits 1 on a one-byte edit. Format history: 0.1.2 text to `v4.20.0-rc5`; 2.0.0 text `v4.22.0-rc3` to `v4.27.0-rc1` (last `9847384d`); NDJSON from `c8407566`.

**Speed, `m1_export`.** `PROOF_RUNS=10`, arm64 macOS: Osprey 3 ms / 2256 KiB, `lean4checker` 433 ms / 52576 KiB. The Lean figure is a cold process replaying one module, not the cost of its checking loop alone.

<a id="export-shape-and-nat-add-comm"></a>
**Export shape and `Nat.add_comm`.** `exports/and_swap.ndjson`: 312 lines, 17,652 bytes, sha256 `c32affd58cdd28574c2c31d14fe290ed237665952e19b00c5fdf39f839797a2d`; 15 declarations (3 inductive types, 3 constructors, 3 recursors, 3 `recOn` defs, 3 theorems). `exports/nat_add_comm.ndjson`: 1,108 lines, 58,926 bytes, sha256 `448084c203b9857a1dceb6986042987be4185ead138d1504039dec0e6c9c3a9f`; seven inductive blocks, 21 definitions, five theorems (`Eq.symm`, `congrArg`, `Nat.zero_add`, `Nat.succ_add`, `Nat.add_comm`), zero axioms. Checking it needed structure projections, exact arbitrary-size literals and recursor annotation handling. Reader rules: JSON keys are in byte order, not declaration order; `Eq.rec` has `k = true` with `isRec = false`, so assume neither default; recursor types and rules arrive as data and must be checked. Suite then: 164 cases on GC, ARC and wasm32. Reproduce: `bash scripts/proof/proof-export.sh --check --theorem Nat.add_comm`, `make BIN=target/ci/osprey _test_proof_kernel`, `node --test scripts/proof/test-proof-export.mjs`.

**OpenAI batch, updated 2026-10-30.** Upstream [openai/math](https://github.com/openai/math/tree/7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a) on Lean 4.34.1, adapted to pinned 4.25.0. Twelve lemmas: 36 passes, 72 mutation refusals. Compiler sha256 `6f5c55438f39162831fc9c2ccf3dcdb5d45dcc1a9a984deec4c73de768831dae`. OpenAIStacks 2,400 lines, sha256 `f2256725919162d349eca2454b26ebc23fe939caec07650db4f1a9ccc83855f3`; OpenAIWordSeed 4,307 lines (71,953 before replacing `omega`), sha256 `14b3950c6e2ea1e73ed3fa529baf236346cb397da4bfa5d695c0049511b81a33`; `ambient-polar.ndjson` 10,122 lines, sha256 `b3cef268b53ce335e0df7eec9fa50d0fa7fdf6118748d62fc421b4c24bf7a177`. 101 theorem names are dependencies, not ported results. Core library: 175 statements, 525 checks, 1,050 rejected mutations (172 original exports, 3 constructive alternatives). String proofs: six axiom-free receipts. Kernel suite: 325 cases per backend.

### CI and release

| Finding | Evidence | Status |
|---|---|---|
| Android emulator dies after bank instrumentation tests, cause unknown; skips bank e2e, documentation acceptance and web compiler, and turns the run red | PR #337: 9 runs, 3 deaths. Ruled out host memory and disk, OOM kill, step limit. Next: capture the guest log | Unchecked |
| One green run is not stability: longest job varies (Proof kernel 11.2 to 15.9 min) | Run 41382962612, 21.6 min, 227 runner-min | Unchecked |
| `cancel-in-progress: true` with rapid pushes prevented convergence on #325: 25 of 62 intervals under 15 min, main CI 49 cancellations, 13 failures, 1 success. Cancelled runs hid real failures. Push one verified candidate | `.github/workflows/ci.yml` | Open |
| GPT-2 full-checkpoint parity test is ignored by default; differential corpus finds only `*.test.osp` and `*.test.ospml`, so Python flavor has no equivalent coverage | `src/toolchain/crates/osprey-cli/tests/cross_flavor_equiv.rs` | Open |
| Two Talon Bank UI tests each wait out a 60 s XCUITest idle timeout | One test takes 4.2 min | Unchecked |
| Slow tests were sharded, not sped up: `every_ported_package_matches_its_cpython_oracle_under_every_memory_backend` (18.6+ min), `workbench_retains_record_identity_through_shared_proof_helpers` (6.7), `workbench_apple_host_includes_shared_proof_dependencies` (3.4) | Run 41362564685 | Unchecked |
| Workbench compile 615 s on CI at `6119a002` against 125 s on a Mac at `d7360f5d`; re-measure after the compiler fixes | No re-measurement recorded | Unchecked |
| Assertion refactors in `src/toolchain/crates/testkit.rs` verified by sample only | Across the `c6a12df3` diff | Unchecked |

**Standard for closing a finding.** A higher percentage closes nothing. A test must come from a named spec obligation and prove the observable contract: exact values, every rejection including boundary cases, side effects and their absence, ownership across cleanup, exit status, and parity across CLI, runtime, LSP, native, ML and wasm.

### Duplication

Target `[MAINTENANCE-DEDUPLICATION]`: `.config/deslop.toml` caps duplication at 2.98% under pinned Deslop 0.38.0 (6,816 of 229,147 lines, 2.9745%), excluding `docs/book/**` at the maintainer's request. The aim is to lower it without weakening an assertion.

| Finding | Failure | Status |
|---|---|---|
| The newer Deslop measures far above the cap | 0.40.1 measured 12,101 of 263,034 lines, 4.60%, against a 3% target, before later changes. The two versions' numbers aren't comparable, and the pin hasn't moved. Remaining: reach and verify 3% on 0.40.1 | Open |
| Deslop can't see the C runtime | Scanning `src/toolchain/compiler/runtime` finds zero supported files, so an empty result there isn't evidence. 187 C, header, Objective-C and Metal files rely on a manual review | Open |
| Twelve ML files don't parse under 0.40.1 | Native runtime and stream, three workbench modules and the bounded-discipline modules. The percentage says nothing about them; don't hide them with exclusions | Unchecked |
| Governance doesn't compile, with or without the deduplication | `cannot access field 'packages' on non-struct type Result<...>`, and unproven dynamic callable effect provenance. Reproduced on the original source | Unchecked |

**Kept deliberately.** These look like duplication and aren't worth merging:

- Generated coordinates from `src/osprey/examples/gpui/painting/reference/logo.py`, the highest-mass findings: preserve the values.
- `http_chunked_tests.c` and `http_framing_tests.c` refusal rows: input permutations to shared helpers.
- `gpu_metal_tests.m` and `gpu_opencl_tests.c`: each pins its own SDK's calls.
- `http_origin.c` and `http_tls_tests.c`, and `http_origin_tests.c` and `websocket_runtime_tests.c`: different transports and refusal protocols.
- Host IEEE conversions and device implementations: `gpu_float_tests.c` uses the host as the oracle, so merging them makes the comparison tautological.

### Already on GitHub

Left out of this section because an issue covers them.

| Finding | Issue |
|---|---|
| `clean.osp` never completes in budget | #306 |
| GC roots refuse 47 of 215 programs | #307 |
| Pre-receive hook declines every push | #308 |
| Channel element metadata lost on returns, function values and generic fields; `gen_recv` raw fallback; missing route assertions | #227 |
| No Windows parity suite for process failure | #228 |
| Inferred stateful handler factories fail with a still-generic closure | #160 |
| Recursive functions and signatures needed for generalization | #287, #278 |
| Lambda in interpolation lexed as an escape | #323 |
| Coverage gates that overstate what they enforce | #232 |
| Effects ABI and dispatcher soundness | #200 |


## Python flavor and package acceptance — 2026-11-05

- [x] Reproduced Python named-pattern shadowing natively (`left:left` instead of `left:right`); fixed lexical binders across syntax, module resolution, type/effect analysis, closures and code generation. Nested/sibling aliases, reordered/partial fields, ignored fields and Result aliases execute under GC and ARC; invalid/duplicate fields are rejected.
- [x] Independently replayed 331 pinned FastAPI 0.135.3 HTTP reference records under GC and ARC, plus graph-configuration failures and cleanup/recovery sequences. Exact response bytes/status, validation locations and input values, applicable headers, request-local caching and LIFO cleanup are asserted. Latin-1 header fixtures preserve the recorded bytes for the native client.
- [x] FastAPI own source: 498/508 lines, 98.0%, separately collected and gated under GC and ARC; ARC exits with zero live objects. The enforced threshold increased from 97.8 to 98.0. Reports: `target/coverage/fastapi-verified-{gc,arc}/`; counts stay outside threshold configuration.
- [x] Added loaded Python-flavor closures, results and cross-module execution scenarios. All five Python interop tests pass, including the existing CPython bridge; relevant Clippy checks pass with warnings denied.
- [x] Expanded SciPy to 2,334 independently derived assertions, preserving existing cases. The new negative/exact-combinatorics inputs exposed 31 mismatches; fixed factorial/combination/permutation domain guards, verified exact GC/ARC goldens and zero live ARC objects. Rejected pi's draft golden containing modeled FAIL rows; the committed golden requires every assertion to pass.
- [x] Made the package transcript gate accept strictly verified native TAP and reject hidden failed rows, inconsistent totals, absent/duplicate plans, skipped/TODO cases and duplicate test numbers. Its refusal cases fail on the previous implementation and pass after the fix; package provenance/source-format checks pass.
- [x] Micrograd: 1,670 upstream-derived assertions (+1,122), all 548 originals preserved; exact GC/ARC goldens, zero live ARC objects, measured/gated 156/160 own-source lines (97.5%).
- [x] Einops: 258 upstream-derived assertions (+155), all original values/labels retained; exact GC/ARC goldens and 432/448 own-source lines (96.4%), threshold raised from 95.3. Independently regenerated from einops 0.8.2; rejected pi's incorrect claim that left-only ellipsis needs a new dedicated rejection. Unicode axis identifiers, tensor execution and complete upstream-suite parity remain open.
- [x] Strengthened HTTP driver reliability: occupied-port fallback passes under concurrent GC/ARC; all eight non-GET methods, empty HEAD responses, non-NUL binary/CRLF/larger echo payloads pass. Both CPython entrypoints refuse `-O`; process guards bound owned descendants and retain diagnostics. Corrected benchmark p95 indexing after a real HTTP red run demonstrated the rank error.
- [x] Fixed the package export scanner's single-quoted Python names after a red integration regression. The broader lexical export gate still reports missing direct names in FastAPI/numpy/tokenizers; indirect execution and 98% FastAPI coverage are not substitutes for resolving that gate. No full-CI pass is claimed.
- [ ] Finish blanket upstream/spec parity for every Python-flavor package. Pi's read-only inventory covers 21 Python-flavor ports; its remaining behavioral candidates need individual real-reference reproduction. Golden assertion counts do not establish equivalence to the upstream suites.
- [x] Added CORS via a generated policy record: 300 pinned FastAPI 0.135.3 / Starlette 1.3.1 real-HTTP reference cases pass under both GC and ARC, externally and from the native coverage suite; original 331 records remain byte-identical. Reviewed pi's fixture and corrected its changed readiness/echo semantics before regenerating. Scoped Starlette BSD notice retained for adapted CORS decision logic; NIMBLESITE MIT license unchanged.
- [ ] CORS-era coverage is 565/577 (97.9%) under both GC/ARC, below the unchanged 98.0% ratchet: gate fails. Reports: `target/coverage/fastapi-cors-{gc,arc}/`. The earlier 498/508 receipt above describes the pre-CORS source only. A process-closure callback helper exposed SIGBUS: preserved crash report `suite-cab2770a963aa3ac.out-2026-11-05-121853.ips`, handed to compiler/runtime owners; the HTTP coverage driver uses the existing named-callback ABI and still asserts child exit status.
- [x] Body baseline now imports the actual four NAP generated models and records 290 real HTTP requests, including model/contract hashes. Typed boundary readers, optional fields, UUIDs, alias precedence, strict extras and response filtering match 277 under both GC and ARC; 331 older routing/dependency and 300 CORS records remain byte-identical. Corrected pi's private-attribute-only response-filter control to return a real extra response key on extra-ignore models.
- [ ] Body parity stays RED in both native coverage runs: nine last-wins duplicate-key cases, three non-finite inputs and one escaped-NUL body differ from FastAPI. All 13 assertions remain in the required suite. Previous 97.9% CORS receipts describe older source; no current passing body-source coverage receipt is claimed. Reports/logs: `target/coverage/fastapi-body-{gc,arc}/`, `target/fastapi/body-failures-*.json`. `spawnProcess` lambda SIGBUS was reported but no owner accepted it while the compiler incident is active; the regression still needs a fix.
- [x] Explicit last-wins decoder fixes all nine original duplicate-key cases without weakening `jsonDecode`. Pi added 49 real upstream cases (339 total), preserving all 290 originals; 334/339 match under both GC/ARC, including nested, escaped-key, malformed and 2,000-repeat cases. The ordinary Default/ML JSON suites match exact goldens under GC/ARC (four clean controls); hardened C builds, targeted Rust Clippy and formatting checks pass.
- [ ] Five body cases remain red: four non-finite inputs (one added by the expanded corpus) and one escaped NUL. Whole native suite/coverage collection therefore still fail; no passing current coverage claim. Earlier 277/290 and 13-failure figures describe the pre-fix snapshot.
- [x] Added 40 actual upstream non-finite controls (379 body cases total), preserving all 339 previous body records and the 331 routing/dependency plus 300 CORS records. The explicit decoder is now named `jsonDecodePython`, with last-wins keys and NaN/infinities/overflowed floats. Both GC/ARC match 378/379; ignored and overwritten non-finite values are genuine 200 controls, not blanket 500 responses. Serialization failures return the same plain 500 as FastAPI. Hardened managed C builds, targeted Rust Clippy/fmt and all four strict Default/ML JSON golden checks pass.
- [ ] Escaped NUL is the remaining body mismatch, retained RED. Native GC/ARC coverage suites still fail and no passing current receipt is claimed. Logs: `target/coverage/fastapi-python-json-{gc,arc}/`. Freshly compiled standalone servers also twice missed the existing five-second readiness limit before executing requests; a later diagnostic launch was healthy with the main fiber sleeping and a listening server. Root cause remains unproven (`target/logs/fastapi/startup-sample.txt`); the readiness gate is unchanged.
- [x] Expanded the NUL oracle by 66 actual NAP HTTP records: all 379 earlier body records are retained; current body corpus is 445. Replayed the existing current native GC/ARC binaries against the new external inputs: 378 match and all 67 NUL cases remain RED. This is test expansion, not a completed NUL fix.
- [x] Reviewed pi's string-boundary inventory and independently inspected scalar/structural/container equality, concat, JSON copying/encoding and file I/O. The JSON NUL guard currently prevents truncation; it must remain until all length consumers are corrected. Pi's abbreviated path list missed three codegen files, which the primary inspected at their actual paths.
- [x] Independently reproduced a separate existing silent failure: both GC and ARC return `success:4:head` for the nine bytes `head\0tail`, while plain and empty-file controls succeed correctly. Added an enforced parallel CLI E2E regression, confirmed RED for leading/middle/trailing/post-growth NUL. The minimal typed file-error guard now passes that unchanged regression under GC and ARC, including clean ARC ownership; all six Python interoperability tests also pass. Rust formatting and hardened C compilation pass. Scoped Clippy is blocked by unrelated GPU numeric conversion lints; the full repository suite has not been run for this fix. Full native NUL string support remains incomplete.
- [ ] Finish full NimblesiteAgenticPlatform/FastAPI parity: body models, registered exception handlers, lifespan, cancellation, streaming, ACP/WebSockets, binary/multipart/files, OpenAPI and all endpoint families remain on the package README checklist.
- [x] Benchmark the supported explicit-response HTTP subset fairly against pinned CPython FastAPI; disable fixture file logging, assert every response and report method/resource limits. These measurements do not establish parity or performance for unfinished framework features.

One pre-existing/shared type-checker performance test still failed during the compiler consumer run: `spaced_necessary_annotations_keep_independent_proofs_batched` observed 134 full solves against a limit of 96. The active compiler-performance owner was notified. Two accidentally broad local test filters launched full package matrices; the 180-second process watchdog reaped both groups, and their 178 MB of owned temporary caches were removed. Exact-name reruns of the intended gate tests pass; those interrupted matrices are not passing evidence.

### FastAPI routing subset benchmark — [FASTAPI-BENCHMARK-SUBSET]

Measured 2026-11-05T01:05:50.558964+00:00 on macOS-26.7.1-arm64-arm-64bit-Mach-O; CPython 3.12.13, FastAPI 0.135.3, Uvicorn 0.43.0. Native output uses -O2; compiler SHA-256 `660f8aea6cca50c5eaf261207a535a28587afaf635e9959c8f5e3f60669d4dd1`.

Four client workers; five trials of 300 requests per workload and 50 warmup requests. Every status, byte length, content type, exact body and parsed body is asserted: 9,303 actual HTTP responses across the three runtimes, including readiness. Servers run sequentially with fresh TCP connections and access logging; fixture trace-file middleware is disabled and tested.

| Runtime | Workload | Median requests/s | Median latency (ms) | Maximum sampled RSS (MiB) |
|---|---|---:|---:|---:|
| osprey-gc | health | 7,172 | 0.402 | 23.9 |
| osprey-gc | bundle | 6,981 | 0.406 | 32.5 |
| osprey-arc | health | 9,827 | 0.376 | 2.3 |
| osprey-arc | bundle | 9,891 | 0.376 | 2.3 |
| cpython | health | 8,757 | 0.426 | 73.9 |
| cpython | bundle | 8,597 | 0.417 | 74.0 |

This measures only the supported explicit-response health and path/repeated-query routing subset. It excludes model binding, dependencies, database access, streaming and lifespan. Latency is the median of trial medians. Throughput includes client-side validation; RSS is sampled after each trial and is not a measured peak. Shared-machine measurements are informational, not a whole-framework performance claim. The independent GC diagnostic run completed five collections with 1–2 KiB live and no disabled-GC message; sampled RSS alone is not evidence of a leak.

Reproduce with `make -C src/osprey/packages fastapi-benchmark`; complete per-trial metrics and binary hashes are in `target/fastapi/benchmark/results.json`, with owned-process resource logs in `target/logs/fastapi/http-benchmark.process.jsonl`.

- [x] [FASTAPI-NAP-VALIDATION] Preserved the IDP real-HTTP RED (three upstream 200/indexed-422 responses versus native 404), added typed `list[str]` binding and numeric validation locations, and reviewed pi's 111-case expansion plus six primary-added multi-error arrays. All 445 previous body records remain byte-for-byte identical. Fresh GC/ARC binaries match 494/562 body records and 116/117 IDP records; every remaining mismatch contains escaped NUL. Full acceptance remains RED with 68 asserted failures, not a passing coverage receipt. All three CORS modes pass both backends; service and native suite type-check. Logs: `target/logs/fastapi/idp-red.log`, `idp-typecheck.log`, `idp-suite-typecheck.log`, `idp-http-final.log`; exact failures: `target/fastapi/body-failures-{18786,18788}.json`. Deslop reports existing fixture/renderer similarities; no unrelated refactor included. Full `make test` was not run because its build prerequisite still builds the forbidden old memory backend.

- [x] [PORT-STRING-NUL-KEYS] Isolated the arena-key identity failure without admitting NUL through the public parser: parsed key buffers mutated to `a\0b`, `a\0c`, repeated `a\0b` and `a` collapsed under `strcmp`. The unchanged isolated regression is RED then GREEN after storing key byte lengths and sharing bounded comparison across strict pairwise/sorted duplicate detection, lookup and Python last-wins. Full C JSON suite passes 5,326 assertions; eight actual Default/ML JSON goldens pass across GC/ARC. Hardened JSON GC/ARC archives rebuilt; freshly linked FastAPI servers retain 494/562 matching body cases and all 68 NUL failures, with all three CORS modes passing both backends. Logs: `target/logs/fastapi/json-arena-{red,green,full-gc}.log`, `json-key-runtime-build.log`, `json-key-goldens.log`, `json-key-http.log`. This is one prerequisite, not completed native String ABI migration.
- [x] FastAPI package publication refusal reproduced (`source.shadowed-builtin`) and fixed in `ee555195`: renamed the `http_exception` renderer, whose reserved `http*` prefix triggered the scanner, to `exception_response` with every caller updated. Exact local publication command changes status 1 to 0; the classifier is unchanged. GC/ARC services build and CORS modes pass; the same 68 body NUL failures remain enforced. Logs: `target/logs/fastapi/idp-publish-{red,green}.log`, `publish-fix-build.log`, `publish-fix-http.log`.

- [x] [PORT-STRING-NUL-OWNERSHIP] Preserved an isolated C RED for owned `head\0tail` reporting length 4 instead of 9, then verified GC/ARC length/cursor inspection with leading/middle/trailing/repeated NUL, Unicode and 14,000 live allocations through table growth and collection. The existing allocation metadata carries length; no extra map or sentinel encoding. Review found a GC finalizer-stamp regression and the primary found invalid ARC STRING sizes plus a metadata-read deadlock inside locked finalizers; each has a preserved RED and unchanged GREEN case. Full shared JSON/String suites pass 55,594 GC and 55,595 ARC assertions. Native GC/ARC ownership suites pass 140,882 and 601,052 assertions; the existing scalar String suite also passes. Wasm32 ARC ownership, allocation-failure and bookkeeping-failure controls pass with the new String lifetime case. Exact focused macOS C line coverage is GC 93.54% (387 executable lines), ARC 99.60% (249); thresholds ratcheted to 93/99. Logs/receipt: `target/logs/fastapi/owned-text-{red-gc,empty-red-arc,finalizer-red,finalizer-green,finalizer-lock-red,finalizer-lock-green,full-gc,full-arc,wasm,http}.log`, `owned-text-receipt.json`, `owned-cov-{gc,arc}/summary.txt`. Fresh linked HTTP servers still match 494/562 body cases and fail all 68 NUL cases; all three CORS modes pass both backends. Public JSON/readFile guards remain in place; full String migration, FastAPI parity and passing native package coverage remain unfinished.
