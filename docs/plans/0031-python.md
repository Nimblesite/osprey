# Plan 0031 — Python flavor and Python interoperability

Contract: [spec 0041](../specs/language/0041-PythonFlavorSyntax.md), [spec 0042](../specs/platforms/0042-PythonInterop.md)
Depends on: [plan 0023](0023-gpu.md), [spec 0034](../specs/runtime/0034-GPUComputation.md) for device runs and measurement records

## Prerequisites in place

- `.py`/`--flavor python`/marker routing and direct CST lowering (records, unions, comprehensions, `for`→`forEach`, typed imports/Callable, HM inference, effects/handlers, fibers) are implemented in `src/toolchain/crates/osprey-syntax/src/python/` and covered by `src/toolchain/crates/osprey-syntax/tests/python.rs`, `src/toolchain/crates/osprey-cli/tests/operation_modes.rs`, `python_arc.rs`.
- `--target=python` scalar bridge, ABI metadata, exception envelopes and opaque object handles are implemented and Cargo-tested (`src/toolchain/crates/osprey-cli/tests/python_interop.rs`); `@python` declarative binding is not implemented and the frontend still rejects the marker.
- GPT-2 forward/sampler port (`src/osprey/examples/llms/gpt2/python/`) matches the Default generator byte-for-byte on host/Metal/OpenCL; training is not ported.

Constraints:
- Parse Python CST in-process; no translation through another flavor, no Python execution of source, no Python dependency in compiled programs.
- Keep HM inference, immutable values, exhaustive matches, Result failures, isolated fibers and existing target restrictions; never introduce implicit Any.
- Reject unsupported constructs with diagnostics at original Python source positions; never fall through to CPython.
- No flavor checks below the AST boundary; equivalent triples must emit identical LLVM IR.
- A strict device run must show adapter identity plus dispatch evidence; host fallback never counts as a pass.
- Keep `src/osprey/examples/llms/gpt2/reference.py` unchanged and excluded from projects unless opted in.
- No speedup claim from native compilation or a GPU banner.

## Phase A — routing/recovery and source-position matrix

- [x] [FLAVOR-PYTHON-CST] Reject invalid dedents in `src/toolchain/crates/osprey-syntax/src/python/mod.rs` (regression from `98d2fa6e`): `if False:` / 4-space `print("hidden")` / 2-space `print("leaked")` exits 0 and prints `leaked`, where CPython raises `IndentationError`. Check: `src/toolchain/crates/osprey-syntax/tests/python.rs` rejects every misaligned statement and clause and accepts every layout CPython accepts.
- [x] [FLAVOR-PYTHON-EXPRESSIONS] Split f-string brace syntax before decoding escapes in `src/toolchain/crates/osprey-syntax/src/python/strings.rs` (regression from `98d2fa6e`): `print(f"\u007b\u007b")` and the `\x7b` form print `{`, where CPython prints `{{`. Check: `src/osprey/tests/regressions/python/` output matches CPython byte for byte across every escape form.
- [x] [FLAVOR-PYTHON-CST] [FLAVOR-PYTHON-SELECT] The Python parser reports original byte positions for error and missing CST nodes, mixed indentation, CRLF, Unicode prefixes and truncated input. Missing tokens identify the expected syntax. The LSP verifies both UTF-8 and UTF-16 columns after an emoji and accent.
- [x] [FLAVOR-PYTHON-VERIFICATION] Python binding identities, annotation highlights, comment-preserving edits and f-string definitions/references resolve against original CRLF source. The editor test checks UTF-8 and UTF-16 offsets separately. Verified 2026-11-23 with 267 LSP and 28 Python frontend tests.

## Phase B — core-lowering triples and LLVM equivalence

- [x] [FLAVOR-PYTHON-BOUNDARY] Flat-call triples for host reductions, device kernels and record-held Result values compare their position-free canonical AST and full LLVM module in Default, ML and Python. The device case also requires a real launch site and extracted kernel source.
- [x] [FLAVOR-PYTHON-TYPING] [FLAVOR-PYTHON-EXPRESSIONS] PEP 484/526/585/591/695 declarations compare equal to the Default and ML canonical contracts. The Python syntax suite pins exact diagnostics and original byte positions for unsupported tuple, slice, generator, parameter, class, return, mutation, exception and coroutine forms. All 30 Python frontend tests passed on 2026-11-23.

## Phase C — effects, fibers, signals; shared handler-state repair

- [x] [FLAVOR-PYTHON-HANDLER] The Python reactive-signals twin exercises known and unknown dependencies, branch changes, no-op revisions, independently owned values and cached closures. Default, ML and Python share one 10-case golden, including every factory and build call. The CLI enforces zero ARC live objects; all three twins also match the golden on wasm32.
- [x] [FLAVOR-PYTHON-HANDLER] Default, ML and Python reject once/abort/static multiplicity violations, non-replayable callbacks and lazy iterators escaping reusable static handlers. Staged hygiene checks compare locally consumed values and lexical shadowing under GC and ARC. Verified 2026-11-23 in the staged CLI and effect-row suites.
- [ ] [EFFECTS-HANDLER-STATE] The shared module cell now survives inlined handler calls in Default and ML. The real CLI checks a shadowing parameter, two writes, self-assignment and retained snapshots under GC and ARC. Remaining: a Python twin that writes the originating outer binding.

## Phase D — project, editor and target integration

- [x] [FLAVOR-PYTHON-SELECT] [FLAVOR-PYTHON-VERIFICATION] All 27 assignments of Default, ML and Python to the entry, ledger model and effect policy produce identical checked ASTs, public contracts and LLVM IR. Eight additional native-flavor assignments retain the abstract signature coverage. All 31 cross-flavor IR tests passed on 2026-11-23.
- [ ] [FLAVOR-PYTHON-VERIFICATION] Wire remaining tooling to the Python flavor: test discovery, docs examples, formatter beyond indentation (`src/toolchain/crates/osprey-fmt/src/python.rs`), annotation edits, LSP completion/hover/signature/rename, debugger locations, VS Code language config (`src/toolchain/vscode-extension/`), web compiler selection (`src/toolchain/website/src/js/wasm-studio.js`), corpus backends (`src/toolchain/crates/corpus_backends.sh` has no `run_python`), caches, CI/release classification. Acceptance: a Python helper edit never masquerades as a language fixture; opted-in Python sources trigger their real compiler/test lanes; `make ci` passes.

## Phase E — GPT-2 edge checks and sampling-boundary record

- [ ] [FLAVOR-PYTHON-GPU] Extend `scripts/tests/check-python-gpt2.py` with weight-download-free tiny-tensor edge checks: ordered reductions, final projection, first-index tie, EOS, malformed tensor/shape errors, empty/overlong prompts, zero/negative token counts, one-token prompt, context exhaustion, unavailable devices. Acceptance: runs in the ordinary corpus; a device case without an actual adapter is ignored, never green.
- [x] [FLAVOR-PYTHON-GPU] [GPU-DEVICE] Both prompts matched across host, Metal and OpenCL at 0/1/20/50 tokens. Verified 24 production runs, 24 token-ID traces and nine EOS/tie controls on 2026-11-23. [Exact IDs, decoded bytes, checkpoint, commands and process results](../../src/osprey/examples/llms/gpt2/python/README.md#sampling-checks)

## Phase F — backward pass and training (deferred)

- [ ] [FLAVOR-PYTHON-GPU] Port `linear_back`, `layer_norm_back`, `gelu_slope`, `attention_back`, `block_back`, loss/backprop, embedding gradients, checksums and one SGD update from `src/osprey/examples/llms/gpt2/reference.py` into a new `train.py` Python-flavor project reusing the forward helpers; mutation becomes immutable buffers/folds. Tiny deterministic gradients plus finite-difference; real 124M step compares losses and all 148 gradient sums. Keep `make -C src/toolchain/targets/llms gpt2-train` rejecting `FLAVOR=python` (`Makefile:403`) until done.

## Phase G — publication and measurements

- [x] [FLAVOR-PYTHON-VERIFICATION] The reference-to-Osprey guide shows `Tensor.mat_mul` and the actual list and GPU handler arms, explains CPU loading and dispatch, and includes reproducible checkpoint-free and real-weight runs. [Port notes](../../src/osprey/examples/llms/gpt2/python/README.md#port-notes)
- [ ] [FLAVOR-PYTHON-VERIFICATION] Measure generation with identical KV-cache mode on both sides (same checkpoint/prompts/options/loading/precision); record cold/warm timings, repeated samples, device/CPU details, failures. No speedup inference.

## Python interoperability — spec 0042

- [ ] [PYTHON-BINDING] Implement typed `@python("module", "callable")` in `src/toolchain/crates/osprey-syntax/src/python/declarations.rs` (today `decorated_statement`/`required_function` rejects it), with flavor-independent canonical foreign metadata in AST/types, `Result<T, PythonError>` return, mock handlers and the capability requirement; un-reject the marker and update `python_classes_and_externs_reject_python_object_or_dynamic_decorator_behavior` in `src/toolchain/crates/osprey-syntax/tests/python.rs`. Acceptance: Default/ML/Python `@python` declarations execute a real call and exercise failures through the same bridge path.
- [ ] [PYTHON-OBJECT] Design and test a buffer/DLPack bridge: dtype/device/shape/strides/contiguity/bounds/read-write ownership/producer lifetime/capsule single consumption/stream sync; reuse Osprey buffer validation. Tiny deterministic arrays first (legal-view identity, producer-deletion survival, double-capsule rejection, unsupported dtype/device/stride, post-sync compare); real selected device plus dispatch evidence required.
- [ ] [PYTHON-HOST] [PYTHON-TARGET-ENTRY] Add reclaiming memory backends and returned-allocation release; define reentrancy and GIL release before parallel Python calls; pin fiber mutable-Python-handle isolation rules; add optional version-checked CPython embedding for standalone native executables.
- [ ] [PYTHON-DATASET-AUDIT] Fix `osprey.build` in `src/toolchain/python/osprey/__init__.py` to pass the `--memory` flag it currently omits and reconcile `src/toolchain/python/osprey/dataset_audit/README.md` (which documents the `osprey-dataset-audit` command as blocked) with the actual target default (`arc`, `src/toolchain/crates/osprey-cli/src/invocation.rs::apply_default_memory`). Acceptance: `osprey-dataset-audit` runs end-to-end again; the README note is removed or made accurate.
- [ ] [PYTHON-HOST-WINDOWS] Windows support for `--target=python`: build the managed ARC runtime for Windows, emit a `.pyd` (with its `.h`) from `osprey --compile --target=python`, make `osprey.load`, `osprey.build` and `python3 -m osprey` in `src/toolchain/python/osprey/` load it on CPython for Windows, and run `src/toolchain/crates/osprey-cli/tests/python_interop.rs` on `windows-latest` in `ci-windows.yml`. Acceptance: the interop guide's `interop.osp` example prints `9.0` and `test answer` on Windows, and the docs drop "Windows coming soon".
- [ ] [PYTHON-HOST] Python-hosted GPT-2 interoperability example: Python owns the checkpoint/model ecosystem, Osprey owns a checked native calculation, both directions use the same bridge; keep `src/osprey/examples/llms/gpt2/reference.py` unchanged. Acceptance: generation runs through Python-hosted orchestration calling native Osprey, and native policy calls back.
