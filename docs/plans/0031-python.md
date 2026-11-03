# Plan 0031 — Python flavor and Python interoperability

Contract: [spec 0041](../specs/language/0041-PythonFlavorSyntax.md), [spec 0042](../specs/platforms/0042-PythonInterop.md)
Depends on: [plan 0023](0023-gpu.md), [spec 0034](../specs/runtime/0034-GPUComputation.md) for device runs and measurement records

## Prerequisites in place

- `.py`/`--flavor python`/marker routing and direct CST lowering (records, unions, comprehensions, `for`→`forEach`, typed imports/Callable, HM inference, effects/handlers, fibers) are implemented in `crates/osprey-syntax/src/python/` and covered by `crates/osprey-syntax/tests/python.rs`, `crates/osprey-cli/tests/operation_modes.rs`, `python_arc.rs`.
- `--target=python` scalar bridge, ABI metadata, exception envelopes and opaque object handles are implemented and Cargo-tested (`crates/osprey-cli/tests/python_interop.rs`); `@python` declarative binding is not implemented and the frontend still rejects the marker.
- GPT-2 forward/sampler port (`examples/gpt2/python/`) matches the Default generator byte-for-byte on host/Metal/OpenCL; training is not ported.

Constraints:
- Parse Python CST in-process; no translation through another flavor, no Python execution of source, no Python dependency in compiled programs.
- Keep HM inference, immutable values, exhaustive matches, Result failures, isolated fibers and existing target restrictions; never introduce implicit Any.
- Reject unsupported constructs with diagnostics at original Python source positions; never fall through to CPython.
- No flavor checks below the AST boundary; equivalent triples must emit identical LLVM IR.
- A strict device run must show adapter identity plus dispatch evidence; host fallback never counts as a pass.
- Keep `examples/gpt2/reference.py` unchanged and excluded from projects unless opted in.
- No speedup claim from native compilation or a GPU banner.

## Phase A — routing/recovery and source-position matrix

- [ ] [FLAVOR-PYTHON-CST] [FLAVOR-PYTHON-SELECT] Complete the recovery/position matrix in `crates/osprey-syntax/tests/python.rs`: Unicode byte and UTF-16 boundary cases, missing/error CST nodes, malformed-indentation variants, recovery without panics. Acceptance: every syntax error points at the original Python source; no synthesized Osprey string appears in a diagnostic.
- [ ] [FLAVOR-PYTHON-VERIFICATION] Add Python cases to the editor-position coverage (`crates/osprey-syntax/src/binding_ranges.rs`, `annotation_edits.rs`) and LSP UTF-16 tests (`crates/osprey-lsp/src/analysis/more_tests.rs`, `features/tests.rs`): binding/annotation/f-string ranges resolve to exact original byte ranges under UTF-16 encoding. Acceptance: Python identities pass the same position matrix as Default/ML.

## Phase B — core-lowering triples and LLVM equivalence

- [ ] [FLAVOR-PYTHON-BOUNDARY] Extend `crates/osprey-cli/tests/cross_flavor_ir_equiv.rs` (and its `gpu.rs`/`device.rs` helpers) so equal flat-call Default/ML/Python triples emit byte-identical LLVM IR. Acceptance: canonical AST equal after scrubbing positions, IR identical for each triple.
- [ ] [FLAVOR-PYTHON-TYPING] [FLAVOR-PYTHON-EXPRESSIONS] Complete positive/rejection triples in `crates/osprey-syntax/tests/python.rs` for the remaining surface: PEP 484/526/585/591/695 edge forms; unsupported tuple/slice/generator syntax, default/variadic arguments, class bodies/bases/dunders, early/partial returns, attribute/index mutation, and `try`/`except`/`raise`/coroutine machinery. Acceptance: exact rejection spans/messages shared with the Default/ML convention.

## Phase C — effects, fibers, signals; shared handler-state repair

- [ ] [FLAVOR-PYTHON-HANDLER] Add Python twins for reactive signals in `crates/osprey-cli/tests/staged_hygiene.rs`/`staged_effects.rs`: `signalSnapshot`/`signalRefresh`/`signalDependencies` with known/unknown dependency rows, branch changes, no-op revisions, owned values, cached pure closures. Acceptance: same goldens as the Default twins; ARC exits with zero live objects.
- [ ] [FLAVOR-PYTHON-HANDLER] Add Python twins for staged multiplicity/hygiene and deferred static operations escaping construction handlers (lazy iterators) in `crates/osprey-cli/tests/staged_multiplicity.rs`/`staged_hygiene.rs`. Acceptance: rejection behavior identical to Default/ML.
- [ ] [EFFECTS-HANDLER-STATE] Repair value-captured handler state in `crates/osprey-codegen/src/stmt.rs`: `cg.handler_value_captures` currently errors with "shared cell identity is missing"; give the capture one shared cell so a handler reached through a function writes the originating binding. Update `crates/osprey-codegen/src/ir_tests/lowering_4.rs` (`value_captured_handler_state_is_quarantined_in_default`/`_ml`) to the repaired behavior and add a Python twin.

## Phase D — project, editor and target integration

- [ ] [FLAVOR-PYTHON-SELECT] [FLAVOR-PYTHON-VERIFICATION] Replace the 2^3 Default/ML gate with the full 27-configuration gate in `crates/osprey-cli/tests/cross_flavor_ir_equiv.rs::mixed_flavor_project_graphs_emit_identical_ir` (every Default/ML/Python assignment over the three-file graph), comparing canonical contracts and LLVM IR.
- [ ] [FLAVOR-PYTHON-VERIFICATION] Wire remaining tooling to the Python flavor: test discovery, docs examples, formatter beyond indentation (`crates/osprey-fmt/src/python.rs`), annotation edits, LSP completion/hover/signature/rename, debugger locations, VS Code language config (`vscode-extension/`), web compiler selection (`website/src/js/wasm-studio.js`), corpus backends (`crates/corpus_backends.sh` has no `run_python`), caches, CI/release classification. Acceptance: a Python helper edit never masquerades as a language fixture; opted-in Python sources trigger their real compiler/test lanes; `make ci` passes.

## Phase E — GPT-2 edge checks and sampling-boundary record

- [ ] [FLAVOR-PYTHON-GPU] Extend `scripts/check-python-gpt2.py` with weight-download-free tiny-tensor edge checks: ordered reductions, final projection, first-index tie, EOS, malformed tensor/shape errors, empty/overlong prompts, zero/negative token counts, one-token prompt, context exhaustion, unavailable devices. Acceptance: runs in the ordinary corpus; a device case without an actual adapter is ignored, never green.
- [ ] [FLAVOR-PYTHON-GPU] [GPU-DEVICE] Record the real-checkpoint sampling boundary (token IDs plus decoded bytes, EOS/tie behavior, process status, weights hash `248dfc3911869ec493c76e65bf2fcf7f615828b0254c12b473182f0f81d3a707`, exact commands) for `The capital of France is` and `Melbourne is the capital of` at 0/1/20/50 tokens on host/Metal/OpenCL.

## Phase F — backward pass and training (deferred)

- [ ] [FLAVOR-PYTHON-GPU] Port `linear_back`, `layer_norm_back`, `gelu_slope`, `attention_back`, `block_back`, loss/backprop, embedding gradients, checksums and one SGD update from `examples/gpt2/reference.py` into a new `train.py` Python-flavor project reusing the forward helpers; mutation becomes immutable buffers/folds. Tiny deterministic gradients plus finite-difference; real 124M step compares losses and all 148 gradient sums. Keep `make -C apps gpt2-train` rejecting `FLAVOR=python` (`Makefile:403`) until done.

## Phase G — publication and measurements

- [ ] [FLAVOR-PYTHON-VERIFICATION] Publish the tested `.py` sources with a concise reference-to-Osprey change guide and reproducible commands (`examples/gpt2/python/README.md#port-notes`); demonstrate an effect and both handlers first; state tokenizer/loading/orchestration stay on CPU; describe transfer/dispatch per spec 0034.
- [ ] [FLAVOR-PYTHON-VERIFICATION] Measure generation with identical KV-cache mode on both sides (same checkpoint/prompts/options/loading/precision); record cold/warm timings, repeated samples, device/CPU details, failures. No speedup inference.

## Python interoperability — spec 0042

- [ ] [PYTHON-BINDING] Implement typed `@python("module", "callable")` in `crates/osprey-syntax/src/python/declarations.rs` (today `decorated_statement`/`required_function` rejects it), with flavor-independent canonical foreign metadata in AST/types, `Result<T, PythonError>` return, mock handlers and the capability requirement; un-reject the marker and update `python_classes_and_externs_reject_python_object_or_dynamic_decorator_behavior` in `crates/osprey-syntax/tests/python.rs`. Acceptance: Default/ML/Python `@python` declarations execute a real call and exercise failures through the same bridge path.
- [ ] [PYTHON-OBJECT] Design and test a buffer/DLPack bridge: dtype/device/shape/strides/contiguity/bounds/read-write ownership/producer lifetime/capsule single consumption/stream sync; reuse Osprey buffer validation. Tiny deterministic arrays first (legal-view identity, producer-deletion survival, double-capsule rejection, unsupported dtype/device/stride, post-sync compare); real selected device plus dispatch evidence required.
- [ ] [PYTHON-HOST] [PYTHON-TARGET-ENTRY] Add reclaiming memory backends and returned-allocation release; define reentrancy and GIL release before parallel Python calls; pin fiber mutable-Python-handle isolation rules; add optional version-checked CPython embedding for standalone native executables.
- [ ] [PYTHON-DATASET-AUDIT] Fix `osprey.build` in `python/osprey/__init__.py` to pass the `--memory` flag it currently omits and reconcile `python/osprey/dataset_audit/README.md` (which documents the `osprey-dataset-audit` command as blocked) with the actual target default (`arc`, `crates/osprey-cli/src/invocation.rs::apply_default_memory`). Acceptance: `osprey-dataset-audit` runs end-to-end again; the README note is removed or made accurate.
- [ ] [PYTHON-HOST] Python-hosted GPT-2 interoperability example: Python owns the checkpoint/model ecosystem, Osprey owns a checked native calculation, both directions use the same bridge; keep `examples/gpt2/reference.py` unchanged. Acceptance: generation runs through Python-hosted orchestration calling native Osprey, and native policy calls back.
