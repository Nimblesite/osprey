# Osprey review

The one review for this repository. It replaces every earlier review and report; those files remain in git history (last present at `ebcb47f3`).

**Status values**, checked against `7910ab65` on 2026-11-03:

- **Open**: the defective code was seen in the tree at that commit.
- **Fixed**: the fix was seen in the tree, or the source review records it with a regression test.
- **Unchecked**: not re-checked since the review. The code may have moved, so treat the finding as a lead and reproduce it first.

Line numbers are as cited at the reviewed commit unless the status gives a newer one.

**Fix these first:** [file server use-after-free](#78be07ec-v0282-to-main), [Windows path traversal](#78be07ec-v0282-to-main), [999th fiber or channel](#runtime-and-security), [WebSocket dead connections](#runtime-and-security).

---

## 1. Regressions by commit

Defects a review attributes to a specific commit, branch or PR, newest first.

### `78be07ec` (v0.28.2 to main)

| Severity | Finding | Where | Failure | Status |
|---|---|---|---|---|
| Critical | Windows folder requests leave the served directory | `compiler/runtime/file_server_runtime.c:216-221` | The guard refuses `/` and a leading `.` but not `\`. `GET /\..\..\Users\me\Documents\secret.txt` from a LAN peer streams the file. Refuse both separators and any `.` or `..` component, or canonicalize and check containment. Add a Windows loopback test. Found independently by two reviewers; not run, no Windows machine. | Open (`:218`) |
| High | Stopping a file server frees state its connection threads still use | `file_server_runtime.c:210-231,279,316-322,388-400` | Stop joins only the accept thread; connections are detached. `release_server` frees `path` and `content_type`, and its `memset` resets `stopping` to false. The casting flow (`packages/chromecast/src/player.py:242-246`) stops while the device holds connections, giving a use-after-free, or the new session's file served to the old device if the slot is reused. An idle connection can also hang 120 s. Count connections and free only after the last exits. Found by two reviewers; not run. | Open (`:279`) |
| High | `osprey_media_unrender` trusts any folder and any PID | `compiler/runtime/media_render_runtime.c:193-229`; `packages/chromecast/src/wire.py:124-125` | The only ownership check is `strstr(folder, "/osprey-cast.")`. A fake `/tmp/osprey-cast.x` makes it SIGTERM the PID in its `pid` file and delete every non-hidden file beside it. A stale genuine folder can kill an unrelated process after PID reuse. Return opaque handles from a runtime registry and track child PIDs directly. | Open (`:213`) |
| High | Deterministic mode deadlocks on channel use inside a fiber | `compiler/runtime/fiber_runtime.c:270-277,615-635`, also `channel_send` | `fiber_await` runs thunks holding the non-recursive `runtime_mutex`, and `select` or `send` re-acquires it. Reproduced: with `fiber_set_deterministic_mode`, a fiber that only calls `send(c, 1)` hangs at `await`. Never hold the lock while running user code. | Unchecked |
| Medium | Escaped quote corrupts interpolation scope | `crates/osprey-syntax/src/strings.rs:205-231,255-264` | Introduced by the #323 fix on `codex/finish-plans`. `\"` reaches `scope.observe` as a real delimiter. ML reproduction: `print "${pick "\"" (\n => n)}"` is a type mismatch, while the `"q"` form prints `q`. | Unchecked (see #323) |
| Low | HTTP externs changed from `int` to `Result` with no external migration path | `crates/osprey-codegen/src/extern_call.rs:84-91` | Programs that call `httpCreateServer`, `httpListen` or `httpGet` stop type-checking after upgrade, guided only by the "undecoded Result" diagnostic. | Unchecked |

### `772d9c2b` and `1609311f` (CI re-phasing)

| Finding | Failure | Status |
|---|---|---|
| Release gate guard rejected `\|\| true` in the new diagnostic step | Format & Analyse stopped at the guard in run 41380639647 and skipped every later analysis step. | Fixed by `772d9c2b` |

### `c6a12df3` (`codex/private-history`, base `40115ff0`)

| Severity | Finding | Where | Failure | Status |
|---|---|---|---|---|
| P1 | `--out` deletes unrelated directories | `crates/osprey-cli/src/hunt/mod.rs:241-253` | Any non-empty directory with an entry named `info` counts as KLEE output, so `osprey hunt file.osp --out existing-directory` deletes its contents. Nothing verifies ownership. | Unchecked (function now at `:269`) |
| P1 | Python version floors use float division | `packages/manager/cli/lib/python_plane.osp:133-135` | `31200` decoded as `3.12.12.0`, so pinned `>=3.12` workspaces failed verification. | Fixed for the component; no full manager-suite result was recorded |
| P1 | Hunt power model dropped libm special cases | `compiler/runtime/klee/hunt_math.c:188-202` | `pow(-1.0, 2^53)` returned NaN, hiding crashes reachable only through the right result. | Fixed (690,680 checks) |
| P1 | Comprehension targets unbound when builtin aliases lower | `crates/osprey-syntax/src/python/comprehensions.rs:12-16,144-148` | `[len(xs) for len in callbacks]` called `length(xs)`. | Fixed |
| P1 | WASM ARC dropped scalars as pointers | `arc_drop_word` | A scalar equal to a live object's address decremented that object's count. | Fixed for the two ownership cases only, not the full WASM corpus |
| P2 | Coverage inventory skipped `export fn` | `scripts/package_coverage.py:57,162-189` | A Default file holding only `export fn` could vanish from a report while the package still passed. | Unchecked (that regex is gone) |
| P2 | Alternate octal ignored precision | `compiler/runtime/klee/hunt_format.c:175-201` | `%#.3o` of `1` gave `0001`; `%#.0o` of `0` gave an empty string. | Fixed |
| | Replay panicked on metadata-like filenames | `crates/osprey-cli/src/hunt/ktest.rs:118` | Real files named `input.len`, `entropy.data` and similar reached a quarantined panic. | Fixed (no `panic!` remains in the file) |
| | Overlay test used stale paths | `overlay_tests.osp:28,117-124` | It read `.osprey/build/osprey.toml` although the command wrote under `--project /abs`. | Unchecked |

### `4170dbd4` and PR #325 (`feat/type-disciplines`)

All of these were resolved before #325 merged. They are kept because they show which areas break first.

| Finding | Evidence | Status |
|---|---|---|
| Classifier rejected valid programs declaring `List` or `Map` | Permanent GC/ARC matrix: 20 callback cases, eight exact `Alarm.ring` refusals, two recursive refusals | Fixed in `4170dbd4` |
| VSIX lacked bundled `osprey-ui/wasi.js` | Runs 40981257660, 40983111344 | Fixed |
| Workbench WASM compile failed on unknown `Nesting::lower`; strict lint failures | Run 40999742311 | Fixed |
| Extension coverage failed on 12 heads: Workbench layout and capability mismatches, lesson Open-button timeout, installed module ordering | Runs 41016854807, 41135572472, 41154533373 | Fixed |
| Rust assertion and warning-hygiene failures | Runs 41046357924, 41154533373 | Fixed |
| Website: missing proof-video elements, broken spec anchors | Runs 40980023928, 41144068677 | Fixed |
| Native macOS: dialog focus (`FrameInheritanceTests.swift:223`), picker overflow of 304 points in a 300-point pane (`MathLayoutTests.swift:62`), text weight of 1,223.95 against 1,461.52 ±75 (`ParagraphLayoutTests.swift:196`) | Runs 41162597660, 41170562511, 41176679238; final 114/114 on macOS 15 | Fixed |

### `c8407566` (upstream lean4export)

The NDJSON transition meant the rev matching pinned Lean 4.25.0 writes format 3.1.0, not the 0.1.2 text the kernel reader took. **Fixed:** the reader now accepts only NDJSON 3.1.0 and refuses other versions by name. Option left unused: pinning a text-format rev (v4.20.0-rc5 era), which breaks the CI toolchain pin.

### `98d2fa6e` (range from release `47577ca1`)

| Severity | Finding | Where | Failure | Status |
|---|---|---|---|---|
| P2 | Invalid Python dedents accepted; statements escape their block | `crates/osprey-syntax/src/python/mod.rs:77-91`; weak test at `crates/osprey-syntax/tests/python.rs:405-414` | `if False:` / 4-space `print("hidden")` / 2-space `print("leaked")` exits 0 and prints `leaked`. CPython raises `IndentationError`. | Open (the pass still checks only tabs) |
| P2 | f-string Unicode brace escapes collapse | `crates/osprey-syntax/src/python/strings.rs:41-52` | `print(f"{{")` prints `{`, not `{{`. Brace syntax must be split before decoding. | Open (`:49`) |
| P2 | `loadTensor` rejected inside a static handler | `runtime_group(files)` | Present at `991bbe80`. | Fixed by `98d2fa6e` |

### `0a92010a` (PR 239, `codex/handler-values`)

**P2: an inner partial runtime handler hides unrelated outer static arms.** `crates/osprey-ast/src/lower_static.rs:138-151`. `handle static Pair { first => 1 second => 40 }` inside `handle Pair { first => 2 }` fails with `unhandled effect operations at program entry: Pair.second`, though the scope contract in [spec 0017](../specs/language/0017-AlgebraicEffects.md) allows complementary partial handlers. **Unchecked.**

### `e5031dc9` (effects mailbox branch, base `a57673e2`)

| Severity | Finding | Status |
|---|---|---|
| P1 | New `any` ownership convention caused ARC use-after-free and leaks (`crates/osprey-codegen/src/lower.rs:298-317`) | Fixed: convention reverted, tests `forward`/`forwarded` and `recovering_a_pointer_from_an_erased_word_takes_no_ownership` |
| P2 | C coverage inventory skipped shipped `test_runtime.c` | Fixed: `C_SHIPPED_UNITS` comes from the archive object lists |
| P3 | Effect docs still listed #182 and #185 as open; stale `__osprey_coro_*` locations | Fixed |

### `30eee4b4` (`fixes`, base `8792de9f`)

All fixed with regression tests, with one exception. Open sub-item: the resolution never confirmed `path_to_uri(&candidate.path).ok()?` (`crates/osprey-lsp/src/diagnostics.rs:212`), which dropped a whole project to standalone analysis when one path failed to convert. **Unchecked.**

| Severity | Finding |
|---|---|
| High | LSP erased a file's real errors whenever an open sibling had a syntax error |
| High | Doctest harness skipped `osprey-ml` fences and blank-line-separated `output` fences, reporting green |
| Medium | Compiler embedded a `website/` file that CI treated as website-only |
| Medium | FLOAT-OPERANDS rejection had no source position |
| Medium | Stray `//!` gave a misleading Default-flavor message |
| Low | Tree-sitter highlighting lost `//!`; three true redundant-annotation warnings vanished; `publish_siblings` analysed the project per keystroke per open file; `resolve.rs` passed 500 lines; new lint gave 835 warnings on the repo's own sources |

### `2691d8dc` (`gpu`, base `2957618e`)

**P0: Windows GC caller-root gate without Windows stack or data/BSS bounds.** `compiler/runtime/memory_gc.c:193-214,265-279,291-318`. `gc_scan_data` handles Apple and glibc Linux only, so global roots go unscanned and the new test depends on the optimizer. Spec 0018 requires stack, registers and data/BSS. **Unchecked.** The rest of that review is in [GPU and staged effects](#gpu-and-staged-effects).

### `9f506c01` (`resume`, base `713366f9`)

| Severity | Finding | Failure | Status |
|---|---|---|---|
| High | Coverage dump parsing turned corrupt evidence green | The dump `# osprey-coverage v1` / `1 1` / `2 garbage` reported 100%. | Unchecked (rewritten at `crates/osprey-cli/src/test_coverage.rs:113`) |
| Medium | `--coverage-json` could write invalid JSON and report success | Tabs and newlines in Unix paths went unescaped. | Unchecked |

---

## 2. System issues not logged on GitHub

Each was compared by title against the 82 open GitHub issues on 2026-11-03; closed issues were not compared. [Findings left out](#already-on-github) because an issue already covers them are listed at the end.

### Runtime and security

| Severity | Finding | Where | Failure | Status |
|---|---|---|---|---|
| Critical | 999th fiber or channel breaks the program | `compiler/runtime/fiber_runtime.c:165-170,256-260,413-417`; `crates/osprey-codegen/src/fiber.rs:44-52` | One ID counter with 999 values for the life of the process, never recycled, even when nothing is concurrent. `await(spawn work(i))` then returns `-1` with exit 0 for `int`, and segfaults (139) for `string`. Channels abort (134). Use growable storage with slot recycling, and surface exhaustion as `Result` or a handled effect. | Open (`:170,:258`) |
| High | `Channel` construction aborts the process | `fiber_runtime.c:449-458`; `crates/osprey-codegen/src/fiber.rs:367-375`; test `compiler/runtime/fiber_runtime_tests.c:340-350` | `Channel(0)`, `Channel(-1)` or exhaustion raises SIGABRT, against [FAILURE-EXPLICIT]. The test asserts the abort, so it must change too. | Open (`:458`) |
| High | WebSocket server keeps dead connections | `compiler/runtime/websocket_server_runtime.c:80,94,131,303-304` | A closed connection stays listed with its old descriptor. `websocketServerBroadcast` writes to whatever socket reuses that number; two clients produced `recipients=2` with one alive and 10 frames instead of 5. After 1000 lifetime connections, new clients are dropped after the handshake. | Open (no decrement of `connection_count`) |
| High | WebSocket stop frees connections still in use | `websocket_server_runtime.c:94,131,335` | The detached thread reads the freed `ws`, then closes a descriptor that may belong to another socket. Read, not run. | Unchecked |
| Medium | WebSocket frame parser unfinished | `compiler/runtime/http_shared.c:237` | Rejects 64-bit lengths and ignores FIN and opcode, so close and ping are handled as text. | Open |
| Medium | HTTP provider turns body and cleanup failures into success | `packages/pydantic_ai/src/providers_http.py:21-38` | Both branches of each conditional are identical; a failed body read returns `Success` with an empty body. | Open (`:25,:36`) |
| Medium | Native store escapes through intermediate symlinks | `hosts/swift/Sources/OspreyNative/StoreAdapter.swift:36-39,60-64`; `hosts/winui/OspreyHost.Core/Store.cs:50-59,86-94` | With `store/link` pointing outside, a write to `link/secret` lands outside the store on both hosts. | Open (neither file mentions symlinks or reparse points) |
| Medium | WinUI output limit applies only after unbounded buffering | `hosts/winui/OspreyHost.Core/Processes.cs:38-65` | A chatty child exhausts host memory before the limit error; stderr is never limited. | Open (`:40-41`) |
| Medium | Compiler stack overflow on extreme nesting | `osprey --check` | 50,000 nested parentheses, 50,000 chained `+`, or 5,000 nested `[` in ML give exit 134. Requires generated source. The language server was not tested. | Unchecked |
| Medium | Deep recursion segfaults without a diagnostic | Compiled programs | Non-tail recursion passes at 300,000 and exits 139 at 1,000,000. | Unchecked |
| Low | `Dns.discover` interpolates `seconds` into a shell command | `packages/chromecast/src/dns.py:35-38` | Current callers pass constants. | Unchecked |
| Low | Stale comment says a test pins a `join` segfault | `crates/osprey-cli/tests/cli_e2e.rs:141-158` | The assertions below it prove the opposite. | Open |
| Unproven | Chromecast parser recursion | `packages/chromecast/src/protocol.py:94-101,135-146`; `hex.py:88-96,102-104` | Reported as a crash, but overstated: a self tail call of the same shape reached depth 20,000,000. `parse_fields` and `points_from` were not checked. | Unchecked |

**Fixed since review, kept for the record:** remote heap overflow in `http_decode_chunked` (`8f96e8bb`, plan 0048), and unsynchronized `gethostbyname` (no longer in `tls_client_runtime.c`).

**Review coverage gaps.** Nothing from the v0.28.2 reviews ran on Linux, Windows, Android or wasm. Not reviewed: GC and ARC internals, effects coroutines, JSON, GPU and media runtime, `hosts/gpui`, most of `hosts/winui`, the packages `signal_flow`, `uitest`, `graphics` and `json`, and most of `packages/pydantic_ai`. Random mutation (1,500 mutants of 79 programs) covered `--check` only. `panic!`, `unwrap` and `expect` sites in production Rust were not audited one by one.

### Language, types and effects

| Severity | Finding | Where | Failure | Status |
|---|---|---|---|---|
| P1 | Abandoned handlers leak owned values | `compiler/runtime/effects_coro.c` (`pthread_exit`, now `:247`) | A control arm that returns without resuming skips generated cleanup: one live ARC object, 13 bytes. Also on `main`. | Unchecked |
| P2 | Operation-returned function fails through a handler value | Effect analysis | `read: fn() -> fn(int) -> int` returned through a handler value is rejected as unprovable, though a block-scoped handler prints `42`. Closest issue is #275. | Unchecked |
| | Generic installers cannot serve several types | `crates/osprey-ast/src/lower_static.rs:436-447` | Static matching compares source spellings. `handle static E<int>` does not discharge an inferred `perform E.echo(42)`. | Unchecked |
| | Unfinished effects surface | Plan 0016 | Open effect rows in function types, named instances, masking, owned or stored continuations, multi-shot resumption, finalization. Wasm rejects dynamic control operations. Native continuations still use pthreads. | Unchecked |
| | `any` needs an ownership-carrying ABI | `crates/osprey-codegen/src/lower.rs` | An erased word lowers like `int`, so no cast-site rule can be sound. Needs a tag or an explicit borrow, forward and consume model. | Recommendation |
| | Reserved word and missing patterns hurt real programs | `examples/proof/kernel/src/reduce.ospml` | `type` cannot be a field name. Nested constructor patterns, and `Result` values in lists, maps and channels, are unimplemented, so two-sided comparison splits into several functions. | Unchecked |
| | LSP test shadow detection is too narrow | `crates/osprey-lsp/src/testing.rs:134-145,191-201` | A value or parameter named `test` produces phantom editor tests. Needs scope-aware resolution. | Open (`shadows_test_builtin`, `:141`) |
| | About 6 s per project analysis | `examples/projects/modules` | Predates the memo, which removed only the repeats per open file. | Unchecked |

### GPU and staged effects

From the `2691d8dc` review: not spec-conformant, not to be presented as completed stages 1-3. All **Unchecked** (the GPU code has since moved to `crates/osprey-codegen/src/gpu/`); paths are as reviewed. Its must-reject golden `gpu_buffer_polymorphic_non_scalar.ospo` and the `scalar_contracts`, `kernel_frontier` and `flavor_parity` goldens were added by that review.

| Severity | Finding | Where |
|---|---|---|
| P0 | `[GPU-BUFFER-ELEM]` bypassed: `fn makeLen(xs) = gpuLength(toGpu(xs))` accepts `List<string>`, because generalized obligations are discarded | `crates/osprey-types/src/builtin_constraints.rs:66-75`; `check.rs:728-738` |
| P0 | Runtime `List<float\|bool>` loses its tag in `toGpu`; `fromGpu` reads those words as integers; typed empty literals stay bare | `gpu.rs:159-174,228-264` |
| P0 | Static-handler monotonicity is not transitive; invalid unused arms are erased before checking | `crates/osprey-ast/src/stage.rs:177-197,225-264`; `lower_static.rs:110-130` |
| P1 | Named GPU helpers take caller bindings: a kernel parameter `bias` shadows global `bias = 10`, computing `x + x`. Reproduced on host, Metal and OpenCL | `crates/osprey-codegen/src/gpu_device/emit.rs:274-295,343-361`; test `cross_flavor_ir_equiv/device.rs:169-203` runs no fixture |
| P1 | `[GPU-KERNEL-ELEM-TYPING]` depends on expression order: `t * x * y` differs from `x * y * t` | `crates/osprey-types/src/expr.rs:482-603` |
| P1 | `[GPU-KERNEL-FORM]` unimplemented: ML cannot spell a block-bodied lambda; unannotated recursive helpers fail | Spec 0034:236-258; `genfn.rs:15-31` |
| P1 | Extraction can regress to zero while the differential stays green; no per-suite floors | Plan 0023:113-123; `crates/run_test_corpus.sh:366-409` |
| P1 | Extract/inline and ARC oracles are fail-open: alternate exit status and leaks are ignored, and a missing sentinel passes | `run_test_corpus.sh:138-144,249-276,294-300,382-395` |
| P1 | Spec overstates: "each kernel" extraction, "fused" host loops, reductions that vary by backend versus byte-identical backends | Spec 0034:131-139,177-186,293-300,381-390 |
| P1 | Staged effects: tail `resume` allowed by spec but always rejected; discharge is per file; handler-owned static state unimplemented; only one of three falsifiers exists | Spec 0035:141-167,457-471; plan 0024:29-41 |
| P1 | `--deps` is approximate and drops parse errors | `stage.rs:304-387`; `crates/osprey-syntax/src/lib.rs:121-130` |
| P1 | D3D12 "working" in plan 0023, never compiled per plan 0023; failures of `Present`, fence wait and drain are discarded; Windows CI runs no bridge check | `examples/graphics/ospgfx_d3d12.c:165-240`; `ci-windows.yml` |
| P1 | Wasm GPU differential non-required and absent from `make ci`; failed allocation becomes an empty buffer while loops keep the requested length; staged wasm rejection is an undefined-symbol skip | `ci.yml:370-385`; `gpu_runtime.c:26-51`; `gpu.rs:371-380`; spec 0035:325-330 |
| P2 | Fusion claims not gated by IR goldens; purity tests miss filter, zip, scan and helper chains; `gpuFilter` loads each element twice; docs parity checks name and arity only | `gpu.rs:442-453`; `builtin_docs_lang.rs:523-545` |
| P2 | `[GPU-IOTA]` omits `n <= 0`; `[GPU-SCAN]` omits how `initial` participates; `[GPU-KERNEL-ELEM-TYPING]`, `[GPU-KERNEL-FORM]`, `[GPU-ROADMAP]` and `[GPU-RESEARCH]` lack references | Spec 0034 |
| P2 | No benchmark artifact for the July numbers; proved-unique free has no direct test; graphics drift guard checks fragments, Metal-to-HLSL only | `graphics_scenes.rs:315-347,449-477` |
| | Spec 0034 calls any handled effect illegal; spec 0035 accepts effects erased by static handlers. Coherent rule: no runtime effect reaches a kernel | Specs 0034, 0035 |
| | Plan 0023 "stages 1-2 complete" contradicts the P0s; the device backend (PTX, Metal compute, launch, transfer, residency) does not exist, and reduction association must be decided first | Plan 0023; `[GPU-BACKEND-DEVICE]` |

### Hunt

| Finding | Where | Status |
|---|---|---|
| Spec calls the budget-limited corpus "green" while plan 0034 keeps Phase 3 open | `docs/specs/tooling/SymbolicExecution.md:300,304` | Unchecked |
| CI skips `clean.osp` by name, the one program proving no false positives on the hardest case (the defect itself is #306) | `HUNT_KNOWN_BUGS`, `scripts/make/tests.mk:229` | Open |
| Phases 4-6 unfinished: typed harness, `libkleeRuntest` replay, regression writer, Windows host, proof benchmarks | Plan 0034 | Unchecked |

**Constraints.** `--max-solver-time=30s` (forked solver) is mandatory: in-process, the solver explored 6 paths and completed 1. **Do not retry:** raw-view bounded parse, O(n) min-reduction line copy, a `gc_scan_range` alignment filter, binary-search digit length in `hunt_format.c`. All measured neutral or worse and were reverted at `f6085332`.

**Evidence, 2026-10-26.** `clean.osp` needs about 49 s of solver work against a 30 s budget; `--solver-backend=z3 --solver-optimize-divides` cut partial states from 18 to 3 (GC) and 19 to 3 (ARC). `make _test_hunt_mock` 948,770 checks, 0 failures. `make _test_hunt_crashes` 15/15. Corpus sweep of 215 programs: 168 fully explored with no findings, 47 refused (#307), 0 planted findings. `staged-rows.osp --memory=arc`: 1 path, 0 findings.

### Proof kernel and Plan 0041

All P0 and P1 findings in the three adversarial reviews were answered at `51d4b91e` and `b6a37b19` with regressions: structural equality, universe instantiation, dependency-ordered imports, recorded exports, gate exit status, the checker deadline, cache keys, the timing harness and duplicate-index rejection. Still open:

| Finding | Where | Status |
|---|---|---|
| Required checks did not require the proof job | Repository ruleset | Unchecked (`ci.yml` now has it) |
| Kernel reader ignores `hints`, `safety` and `all` on `def` and `thm` lines | `examples/proof/kernel/src/export.ospml:329,331` | Unchecked |
| Export corpus has no `letE`, `quot`, `opaque` or `axiom` lines to measure against | `exports/and_swap.ndjson` | Unchecked |
| Three original core-library certificates refused at nested `Lean.Syntax`; only constructive alternatives check | `examples/proof/lean-core/compatible.json`, `frontier.json` | Open by record |
| No performance result for `Nat.add_comm` or Mathlib; Mathlib imports, full discipline frontend, exact-export speed gate and Workbench screenshot acceptance undelivered | Plan 0041 | Open by record |
| Twelve OpenAI lemmas are the checked-export layer, not ports of the headline results; no readable proof frontend exists | [PROOF-RUNNABLE-LIBRARY] | Open by record |
| OpenAI closures keep `propext` (stack closure also `Quot.sound`); arithmetic and injectivity proof bodies differ from upstream, statements unchanged | `examples/proof/openai/` | Tracked by harness |
| Workbench `imported.ospml` coverage 90.8% (2,053/2,261 lines); host interactions and installed-VSIX visual acceptance are separate gates | `apps/workbench/src/imported.ospml` | Unchecked |
| Lean pin is bumped by hand; review of normalization rules is not a soundness proof | [PROOF-SPEED]; kernel | Standing |

#### Recorded proof evidence

Dated evidence for proof claims. Add new dated entries here.

**Road test, 2026-10-29.** M0 kernel: 330 Osprey lines in six files, about four hours to first green (most of it the positional-constructor gap, since fixed), longest function 19 lines. Costs: recursive functions need written signatures that the checker then flags as redundant; no nested patterns.

**Export fixture, 2026-10-29.** Lean 4.25.0 (`cdd38ac5115bdeec5f609e9126cce00f51ae88b3`), lean4export `cfefafacc917b2d14c5f9bb092ed337f71317a4a`, lean4checker `aa03950243dc18750c1546602ab054609ea8d57d`, both tag `v4.25.0`. `exports/id_imp.ndjson`: 24 lines, sha256 `2adb6bc5ff4e2fcaf9d804548fc2eaf0753902effbac3d70e1657d1b5998330d`. `Fixtures::idImp` is that text byte for byte; `idImpBad` changes line 22 from `"body":1` to `"body":2`. `scripts/proof-export.sh --check` exits 1 on a one-byte edit. Format history: 0.1.2 text to `v4.20.0-rc5`; 2.0.0 text `v4.22.0-rc3` to `v4.27.0-rc1` (last `9847384d`); NDJSON from `c8407566`.

**Speed, `m1_export`.** `PROOF_RUNS=10`, arm64 macOS: Osprey 3 ms / 2256 KiB, `lean4checker` 433 ms / 52576 KiB. The Lean figure is a cold process replaying one module, not the cost of its checking loop alone.

<a id="export-shape-and-nat-add-comm"></a>
**Export shape and `Nat.add_comm`.** `exports/and_swap.ndjson`: 312 lines, 17,652 bytes, sha256 `c32affd58cdd28574c2c31d14fe290ed237665952e19b00c5fdf39f839797a2d`; 15 declarations (3 inductive types, 3 constructors, 3 recursors, 3 `recOn` defs, 3 theorems). `exports/nat_add_comm.ndjson`: 1,108 lines, 58,926 bytes, sha256 `448084c203b9857a1dceb6986042987be4185ead138d1504039dec0e6c9c3a9f`; seven inductive blocks, 21 definitions, five theorems (`Eq.symm`, `congrArg`, `Nat.zero_add`, `Nat.succ_add`, `Nat.add_comm`), zero axioms. Checking it needed structure projections, exact arbitrary-size literals and recursor annotation handling. Reader rules: JSON keys are in byte order, not declaration order; `Eq.rec` has `k = true` with `isRec = false`, so assume neither default; recursor types and rules arrive as data and must be checked. Suite then: 164 cases on GC, ARC and wasm32. Reproduce: `bash scripts/proof-export.sh --check --theorem Nat.add_comm`, `make BIN=target/ci/osprey _test_proof_kernel`, `node --test scripts/test-proof-export.mjs`.

**OpenAI batch, updated 2026-10-30.** Upstream [openai/math](https://github.com/openai/math/tree/7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a) on Lean 4.34.1, adapted to pinned 4.25.0. Twelve lemmas: 36 passes, 72 mutation refusals. Compiler sha256 `6f5c55438f39162831fc9c2ccf3dcdb5d45dcc1a9a984deec4c73de768831dae`. OpenAIStacks 2,400 lines, sha256 `f2256725919162d349eca2454b26ebc23fe939caec07650db4f1a9ccc83855f3`; OpenAIWordSeed 4,307 lines (71,953 before replacing `omega`), sha256 `14b3950c6e2ea1e73ed3fa529baf236346cb397da4bfa5d695c0049511b81a33`; `ambient-polar.ndjson` 10,122 lines, sha256 `b3cef268b53ce335e0df7eec9fa50d0fa7fdf6118748d62fc421b4c24bf7a177`. 101 theorem names are dependencies, not ported results. Core library: 175 statements, 525 checks, 1,050 rejected mutations (172 original exports, 3 constructive alternatives). String proofs: six axiom-free receipts. Kernel suite: 325 cases per backend.

### CI and release

| Finding | Evidence | Status |
|---|---|---|
| Android emulator dies after bank instrumentation tests, cause unknown; skips bank e2e, documentation acceptance and web compiler, and turns the run red | PR #337: 9 runs, 3 deaths. Ruled out host memory and disk, OOM kill, step limit. Next: capture the guest log | Open |
| One green run is not stability: longest job varies (Proof kernel 11.2 to 15.9 min) | Run 41382962612, 21.6 min, 227 runner-min | Open |
| Two Talon Bank UI tests each wait out a 60 s XCUITest idle timeout | One test takes 4.2 min | Unchecked |
| Slow tests were sharded, not sped up: `every_ported_package_matches_its_cpython_oracle_under_every_memory_backend` (18.6+ min), `workbench_retains_record_identity_through_shared_proof_helpers` (6.7), `workbench_apple_host_includes_shared_proof_dependencies` (3.4) | Run 41362564685 | Unchecked |
| Workbench compile 615 s on CI at `6119a002` against 125 s on a Mac at `d7360f5d`; re-measure after the compiler fixes | Plan to re-measure not recorded as done | Unchecked |
| `cancel-in-progress: true` with rapid pushes prevented convergence on #325: 25 of 62 intervals under 15 min, main CI 49 cancellations, 13 failures, 1 success (overlapping). Cancelled runs hid real failures. Push one verified candidate | `.github/workflows/ci.yml:53` | Open |
| Manual deploy workflows bypassed the audit gate | `deploy-pages.yml` now has an audit job | Fixed for pages; `deploy-webcompiler.yml` unchecked |
| Required CI job named "Format" did not run formatting | `ci.yml:44-47,131-145,268-278` | Unchecked |
| Live ruleset was never compared with the required checks | No live configuration inspected | Unchecked |
| Assertion refactors in `crates/testkit.rs` verified by sample only | Across the `c6a12df3` diff | Unchecked |
| GPT-2 full-checkpoint parity test is ignored by default; differential corpus finds only `*.test.osp` and `*.test.ospml`, so Python flavor has no equivalent coverage | Release `47577ca1` review | Unchecked |
| Delivery targets open in plans 0020, 0031, 0032, 0034, 0035: hosted trust, Python tooling, training, profiler heap and Windows | Open by plan | Open by record |

**Fixed:** CI re-phased from 57 min and 324 runner-min to about 21 min and 223 runner-min (build first, analysis parallel, Apple and Rust jobs split, timeouts about 1.5x healthy time); GC scan hash and bounds took `signal_flow` from 906 s to 33 s on Linux (`compiler/runtime/memory_gc.c`); missing LFS goldens (`32636a2a`, `8356f2f7`); v0.25.0 glob/minimatch audit (#330).

**Standard for closing a finding.** A higher percentage closes nothing. A test must come from a named spec obligation and prove the observable contract: exact values, every rejection including boundary cases, side effects and their absence, ownership across cleanup, exit status, and parity across CLI, runtime, LSP, native, ML and wasm.

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
