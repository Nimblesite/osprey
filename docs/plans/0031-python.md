# Plan 0031 — Python flavor

**Status:** frontend subset implemented and under targeted verification: routing, basic expressions/functions, restricted records, typed foreign signatures, standard typing imports/Callable, HM inference, Unit, Result/list matching, dynamic/static handler values, forwarding and generic single-shot resumption. GPT-2 generation is proven on the host and on Metal/OpenCL hardware; complete frontend, tooling and the phase E edge-case checks remain open; training (phase F) is deferred; no complete-flavor claim.
**Contract:** [spec 0041](../specs/language/0041-PythonFlavorSyntax.md). The existing [flavor boundary](../specs/language/0023-LanguageFlavors.md) remains authoritative for shipped frontends.
**Outcome:** familiar `.py` source, lowered from its own CST into the same Osprey AST, with effect-selected CPU/GPU GPT-2 execution. Python runtime compatibility and unmeasured comparisons with Mojo are outside the claim.

## Fixed decisions

- Keep `.py`; explicit compiler invocation selects the flavor. Project discovery and editor ownership require opt-in, protecting the independent GPT-2 oracle and repository Python tools.
- Parse Python concrete syntax in-process; lower directly to canonical Osprey nodes. No source translation through another flavor, embedded Python, or Python dependency in compiled programs.
- Keep Hindley–Milner inference, immutable values, explicit arithmetic policies, exhaustive matches, Result failures, isolated fibers, and existing target restrictions.
- Omit annotations wherever the shared Osprey checker can infer them. Never introduce implicit Any or unchecked unannotated bodies; reject genuinely uninferable required types while retaining valid inferred polymorphism.
- Plain classes are closed records. Signature-only `def` declarations are stored function fields supplied by construction. No inheritance, interfaces, implicit receivers, constructors, or method dispatch.
- Use grammar-compatible decorated classes for effects and handlers. `@effect`/`@static_effect`, `@handler(E)`/`@static_handler(E[T])`, `perform(E.op(...))`, and `resume(...)` lower to existing semantics. Decorators are compiler syntax, not executable Python functions.
- Explicit tensor operations and existing handlers decide placement. A host result and a hardware device result are separate acceptance records.

## Scope and coordination

This plan owns frontend syntax, source selection and authoring-tool integration. [Plan 0031](0031-python.md) owns the CPython host bridge; [plan 0020](0020-package-manager.md) owns package resolution and build preparation. Follow [repository instructions](../../AGENTS.md), coordinate file ownership in the primary checkout and serialize resource-heavy validation with other active work.

## Existing seams and exact gaps

| Area | Existing source to extend | Checkpoint and remaining work |
| --- | --- | --- |
| Selection/AST entry | `crates/osprey-syntax/src/lib.rs` | `.py`, explicit selection, leading markers and conflict tests implemented; complete recovery/position matrix remains |
| Frontend | `crates/osprey-syntax/src/python/` | Direct CST lowering includes records, `@union` variants, pattern matching, comprehensions and `for` lowering to `forEach`; unsupported pattern/typing forms and full coverage remain |
| Source identities | `binding_ranges.rs`, `annotation_edits.rs`, `fragment_ranges.rs`, `strings.rs` | Python binding/annotation/f-string positions implemented; full editor position matrix remains |
| Project graph | `crates/osprey-project/`; CLI `project.rs` | Opt-in discovery, imports/aliases, exports and qualified constructors implemented; the complete 27-configuration graph acceptance gate remains |
| Authoring tools | `crates/osprey-fmt/`, `osprey-lsp/`, `vscode-extension/` | Indentation formatting and signature rendering implemented; complete format/rename/debug coverage and explicit editor ownership remain |
| Corpus | `crates/run_test_corpus.sh`, backend runners | Python triples with the same goldens and rejections |
| Model | `examples/gpt2/python/` | Forward model and sampler execute on the host and on Metal/OpenCL hardware with the shared Tensor identity; the phase E edge-case checks remain; training is deferred |
| Device validation | GPU runtime tests, [plan 0023](0023-gpu.md) | Execute this exact new port on strict device backends; existing device evidence does not validate future frontend changes |

`Flavor::Python` now routes `.py` and explicit selections to a pinned in-process Python CST lowerer. Project allowlisting is implemented; complete test/docs/editor discovery remains open. Adding this route does not by itself deliver the complete flavor. Existing `reference.py` relies on Python mutation, object behavior, slicing, tuples and runtime library imports beyond this subset; it cannot simply be renamed or silently accepted as Osprey. Supported Osprey `for` loops do not imply arbitrary Python iterable behavior.

## Phase A — frontend boundary and routing

Write failing tests before implementation. Add the pinned Python tree-sitter grammar as a compiler dependency, retaining its license. Expose a parse/validate/lower seam in `crates/osprey-syntax/src/python/`, with each file under 500 lines and functions under 20. Use shared string/position utilities where the semantics agree; do not copy the ML parser or Default lowerer.

Test `.py`, `--flavor python`, a marker after shebang/encoding comments, unknown markers, conflicts, late/duplicate markers, malformed indentation, Unicode byte/UTF-16 boundaries, missing/error CST nodes, and recovery without panics. Syntax errors point at the original Python source, never a synthesized Osprey string.

**Exit:** simple Python bindings/functions lower to canonical AST twins; the existing frontends and their selection behavior pass unchanged. This phase makes no complete-flavor claim.

## Phase B — values and Python-shaped control flow

Implement functions, lexical bindings and PEP-style typed reassignment/Final, calls/named calls, PEP 484/526/585/591/695 annotation imports, type application/aliases, generic records, signature-only function fields, unions, literals, matches, conditionals, terminal returns, f-strings, and documented builtin spellings. Add comprehensions/`for` only through existing iterator nodes/calls. Preserve evaluation count, left-to-right order and scope when generating callbacks and temporaries. Generated names cannot collide with source.

Reject Python truthiness, reassignment of Final bindings, arbitrary precision, implicit null, early returns, classes with bodies/bases/dunders, default/variadic arguments, arbitrary decorators, attribute/index mutation, unsupported tuple/slice/generator syntax, and Python exception/coroutine machinery. The rejection is evidence that the compiler preserves Osprey semantics, not permission to quietly emulate Python.

**Exit:** canonical AST and byte-identical LLVM checks for equal flat-call triples, execution under GC and ARC, exact rejection spans/messages, and no flavor checks below the AST boundary.

## Phase C — effects, fibers and reactive signals

The `osprey-syntax` Python integration suite (`cargo test -p osprey-syntax --test python`) covers AST/routing/rejection cases, including union classes and `for` lowering. The `osprey-types` Python generic tests cover inferred polymorphism, higher-order body checking, rejection of a still-generic closure value, annotation conflicts, and an explicit Callable constraint. `cargo test -p osprey-cli --test operation_modes --test cross_flavor_equiv` passes native handler/Result/inference/fiber cases under GC and ARC, including clean ARC exits. The existing GPU-combinator checker matrix now exercises Python alongside Default and ML for transitive dynamic-effect rejection, unknown callbacks and arithmetic-policy requirements. These checks do not establish device execution or the remaining phase exit criteria.

A separate shared-backend regression is quarantined: a handler reached through a function can capture file-scope mutable state by value and lose its writes. Compilation rejects an assignment when that captured binding has no shared cell identity. Default/ML regression cases pin the diagnostic and local-shadowing checks preserve valid cell-backed handlers. The state semantics repair remains open.

Implement all forms specified by `FLAVOR-PYTHON-EFFECT`, `FLAVOR-PYTHON-HANDLER`, and `FLAVOR-PYTHON-INTEROP` through the existing AST metadata/lowering helpers. Preserve operation names, stage, value/control mode, multiplicity, replay metadata, captures, effect annotations and open remainders. No new effect engine or callback specialization rules.

Add Python twins to the existing handler/staged/reactive suites, mixing:

- Static generic `Tensor` handlers and higher-order callbacks imported across flavors.
- Dynamic value handlers, control arms that resume or decline, forwarding to an outer activation, and nested handlers.
- Handler requirements propagated through lambdas, records/function fields, fibers and helpers; missing-handler and wrong-stage errors.
- Fiber-local handler state/message isolation and existing target rejection behavior.
- Reactive snapshots with known/unknown dependency rows, branch changes, no-op revisions, owned values and cached pure closures.
- Rejection of deferred static operations escaping construction handlers, including lazy iterators; no invocation-time specialization claim.

**Exit:** new Python cases share the existing goldens and preserve ARC ownership; shared checker diagnostics apply to all three flavors.

## Phase D — project, editor and target integration

Project discovery now implements the literal `python_sources` list and an explicitly selected `.py` entry. Six focused tests cover membership, reference/tool exclusion, duplicate/missing/wrong-extension selections, manifest-relative paths, conflicting overrides, and symlinks escaping source roots. Full `osprey-project` tests pass. Logical Python imports/directives/exports and imported constructor identity are implemented. Python import qualifiers become canonical paths in the frontend, preserving existing Default/ML field and module diagnostics. Editor ownership and the complete mixed graph matrix remain open.

The explicit `python_sources` selection and namespace/module/import/export lowering exist. Complete mixed graph tests to every assignment of three flavors over the existing three-file graph (27 configurations), comparing canonical contracts and LLVM IR. Include a real `reference.py`/tool script beneath a source root and prove it is excluded unless opted in. A selected conflicting `.py` is a diagnostic, never executed as a Python helper.

Wire test discovery, docs examples, formatter, annotation edits, LSP completion/hover/signature/rename, UTF-16 ranges, debugger locations, extension language configuration, web compiler selection, corpus backends, caches and CI/release classification. A Python helper edit must not masquerade as a language fixture, and Python-flavor sources must trigger their real compiler/test lanes. No required gate is weakened or made advisory.

**Exit:** native/WASM/mobile supported cases pass, unsupported target cases reject, installed-extension tests preserve ordinary Python ownership, and `make ci` passes. Keep public guides explicit about the implemented Python subset and remaining editor/target coverage; three frontend routes do not establish feature parity.

## Phase E — real GPT-2 generation port

The [runnable sources](../../examples/gpt2/python/README.md) now live in `examples/gpt2/python/`, with a generation project manifest and `make -C apps gpt2 FLAVOR=python`. The port is self-contained Python-flavor source and reaches no `.osp` module: `backends.py` declares the `Tensor` static effect, its list and GPU static handlers, the `Fault` effect and the `strictly`/`under` policy; `tokenizer.py` is the byte-level BPE tokenizer; `model.py` holds the constants, records, checkpoint loader, forward pass and KV-cached step; `generate.py` is the Default sampler in the Python flavor. `python_gpt2_project_contains_no_osprey_flavor_source` pins the manifest and tree, and `python_gpt2_generator_output_is_byte_identical_to_the_default_generator` compares both generators' stdout bytes on host, Metal and OpenCL (it needs the downloaded checkpoint, so it runs with `--ignored`; the checkpoint-free `python_gpt2_generator_prints_the_default_generator_messages` runs in every suite). [Port notes](../../examples/gpt2/python/README.md#port-notes) record the concrete edits.

Current evidence: the native compiler's `python_gpt2_forward_tensors_match_the_independent_reference` test compares every element of 24 tensors, including every activation in a full block at width 768, under both handlers and both managed memory modes. Host results are exact and ARC exits retain no live objects. `python_gpt2_device_runs_never_pass_on_host_fallback` repeats the comparison with `OSPREY_GPU=metal` and `OSPREY_GPU=opencl` (`auto` off macOS) and fails unless `gpuDevice()` names a `metal:`/`opencl:` device and the trace shows completed dispatches; without GPU hardware it is ignored, never green. The port's row reductions go through `add_all(size, lambda …)`, and every such kernel declined on a device with "a function value has no device form" until the device emitter learned to inline a lambda or named-function argument where the helper calls it (`kernels_reducing_through_higher_order_helpers_keep_their_device_form`, pinned in every flavor). Real checkpoint `248dfc3911869ec493c76e65bf2fcf7f615828b0254c12b473182f0f81d3a707` (`model.safetensors`), `make -C apps gpt2 FLAVOR=python GPU=metal|opencl TOKENS=0|1|20|50 PROMPT=…` under ARC: 0 tokens preserves the prompt; 1 token gives `the`; 20 tokens of `The capital of France is` and `Melbourne is the capital of` on `metal:0 (Apple M4 Max)` and `opencl:0 (Apple M4 Max)` decode byte for byte to the Default example's output on the same device; 50 tokens agree across both devices. The remaining tiny-tensor edge checks (malformed shapes, overlong prompts, negative token counts, context exhaustion, unavailable devices) and the sampling-boundary record are still open; this evidence does not close phase E.

First verify tiny tensors against the independent reference: transpose, ordered reductions, linear, LayerNorm, GELU, causal scores/probabilities, residuals, one block, final projection, first-index tie, and EOS. Test malformed tensor/shape errors, empty/overlong prompts, zero/negative token counts, a one-token prompt, context exhaustion, and unavailable devices. Keep these checks weight-download-free in the ordinary corpus.

The generation manifest opts in to `../backends.py`, `../cpu.py`, `../gpu.py`, `../tokenizer.py`, `../model.py` and `generate.py`; its only source root is the Python port directory, which holds no `.osp` file. Keep `reference.py` and the other program's entry excluded. Phase F is deferred, so there is no Python training project; preserve `BACKEND=lists|gpu`, `GPU=host|auto|metal|opencl`, prompt/token options and ARC.

Run the real checkpoint against the independent reference and Default example for at least `The capital of France is` and `Melbourne is the capital of`, including 0/1/20/50-token requests where context allows. Compare token IDs and decoded bytes, EOS/tie behavior and process status. Record weights hash and exact commands. List/device-specific floating-point differences follow spec 0034; fabricated zeros or an unnoticed CPU fallback fail acceptance.

**Exit:** the new Python model, same body under both Tensor handlers, completes strict CPU and actual-device generation. Device identity plus dispatch evidence is mandatory. Missing hardware is recorded as unverified delivery, never a passing acceleration test.

## Phase F — backward pass and training

**Deferred.** The training port is not being done at the moment: [`examples/gpt2/train`](../../examples/gpt2/train) stays Default-only and `make -C apps gpt2-train` rejects `FLAVOR=python`. The rest of this phase records the work for when it resumes.

Port `linear_back`, `layer_norm_back`, `gelu_slope`, `attention_back`, `block_back`, loss/backprop, embedding gradients, checksums and one SGD update from the reference. Reuse its function names and tensor ordering, changing mutation to immutable buffers/folds. Reuse the new forward helpers, not another transformer copy. This phase is not yet represented by a proposed `train.py` file.

Tiny deterministic cases compare complete gradients element-by-element and run finite-difference checks. The real 124M step compares initial/post-update losses and all 148 established gradient sums with the reference and existing trainer, under host and strict device handlers. Capture arithmetic/shape/load errors as failures; do not change the oracle or accept partial training output.

**Exit:** a Python-flavor training project performs a full step on CPU and real GPU. Keep explicit device tests separate from portable host checks, with the existing required hardware gate enforcing their established coverage.

## Phase G — publication and measurements

Publish only tested `.py` sources, a concise reference-to-Osprey change guide, and reproducible commands. Demonstrate an effect and both handlers before describing compiler internals. State that tokenizer, loading and host orchestration remain on the CPU, and describe actual transfer/dispatch behavior from spec 0034.

Measure generation using the same KV-cache/full-recompute mode on both sides of a comparison; the current Python-flavor generator already implements a KV cache. Compare identical work/checkpoints/prompts/options, loading policy and precision. Record cold/warm timings, repeated samples, device/CPU details and failures. Do not infer a speedup from native compilation or a GPU banner, and do not compare languages without controlled measurements.

## Delivery test inventory

These are planned assertions, not existing passing tests. Extend the established suites rather than scattering one-case files.

| Proposed assertion | Contract pinned |
| --- | --- |
| `python_ast_twins_preserve_flat_calls_and_scope` | Boundary, evaluation order, inference |
| `python_cst_rejects_unsupported_python_without_execution` | CST ownership, rejection, recovery |
| `python_class_signatures_are_required_record_fields` | Construction, field calls, no object model |
| `python_effect_decorators_preserve_operation_metadata` | Stage, control/value, replay/multiplicity |
| `python_handlers_forward_resume_and_capture_like_twins` | Dynamic/static semantics |
| `python_reactive_snapshots_and_fibers_share_the_core` | Signals, ownership, fiber isolation |
| `python_sources_leave_the_reference_and_tools_unselected` | `.py` project/editor coexistence |
| `three_flavor_project_graphs_emit_identical_ir` | All 27 mixed graphs |
| `python_gpt2_forward_matches_the_independent_oracle` | Actual port mathematics |
| `python_gpt2_device_runs_never_pass_on_host_fallback` | Strict hardware execution |
| `python_gpt2_training_matches_all_gradients` | Full backward pass/SGD |

## TODO checklist

- [x] Record `.py`, CST/AST boundary, restricted classes, effects and handler spelling in spec 0041.
- [x] Publish compiled reference-shaped forward/sampling sources and record host execution evidence.
- [ ] Phase A: complete routing/recovery and source-position coverage around the implemented frontend.
- [ ] Phase B: complete core-lowering positive/rejection triples and LLVM equivalence; unions and iterator loops already lower.
- [ ] Phase C: effects, continuations, staged requirements, fibers and signals.
- [ ] Phase D: project/editor/tooling/target integration and complete required CI.
- [ ] Phase E: tiny and real GPT-2 generation on CPU and hardware GPU.
- [ ] Phase F: backward pass and full training on CPU and hardware GPU (deferred: not being ported at the moment).
- [ ] Phase G: measured evidence, runnable publication and spec/status synchronization.

# Plan 0031 — Python interoperability

**Contract:** [Python interoperability](../specs/platforms/0042-PythonInterop.md). **Status:** native Python-hosted bridge and installed bidirectional LLM dataset-audit application implemented; declarative typed bindings, GPT-2 model integration, storage interchange and standalone embedding remain open.

The entry workflow is a Python team compiling the parts of its application written in Osprey, calling native functions from its existing Python process, and continuing to call established Python libraries from Osprey. Teams can adopt algebraic effects without replacing the interpreter, environment or model stack. Native compilation is not evidence of a workload speedup: measure boundary overhead and complete workloads before performance claims.

## Scope and coordination

This plan owns the Python host target, checked ABI, runtime callbacks, loader package and interop tests. [Plan 0031](0031-python.md) owns syntax/frontend and authoring-tool coverage. Follow [repository instructions](../../AGENTS.md), preserve other owners' changes and coordinate validation in the primary checkout.

## Delivered foundation

- `--target=python` builds a shared library and C header using existing checked scalar exports. Embedded ABI metadata configures Python calls without stale sidecars.
- `osprey.build/load`, attribute calls and qualified `library.call` provide a stdlib Python API; `python -m osprey` runs an Osprey entry inside the host. Initialization occurs after callbacks are installed.
- Runtime Python imports/calls use explicit success/error envelopes. Conversion/encoding failures and Python exceptions stay visible. Compiler operation does not execute foreign Python.
- Opaque object handles preserve Python object identity and payloads, accept references and keyword arguments, and require explicit release. This supports tensor objects without promising storage interchange or backend compatibility.
- Default, ML and the tested Python frontend subset use one implementation. Python flavor's typed `@extern` signatures lower to the same canonical foreign declaration and execute the real bridge.

The first implementation confines calls to one loading thread, holds the GIL across native exports and uses the default allocation backend. It intentionally reports those boundaries rather than claiming concurrent training-service readiness. The integration suite is part of Cargo's ordinary CLI tests; no optional GPU dependency is added to the standard gates.

## Typed direct calls and effects

1. Agree the foreign-binding metadata and canonical AST representation with the Python Flavor owner. Reserve `@python("module", "callable")` on signature-only declarations. Record exact Default/ML syntax and logical module identity rules. No source translation through CPython or a second Osprey frontend.
2. Write failing parser/AST twins and type/effect tests before implementation. A call returns an explicit checked `Result`; module/import/conversion/Python exceptions cannot escape into native frames. A nominal `PythonObject` owns an opaque registry reference; no integer-to-object coercion at the public typed boundary.
3. Lower scalar arguments/results and object references through one host request interface. Generated glue must preserve argument order and evaluate arguments once, support mock/live Python handlers, propagate capability requirements through helpers/lambdas, and reject calls from GPU kernels or unsupported targets.
4. Run direct-call triples in Default/ML/Python with identical canonical metadata and outputs. Reject mismatched runtime types, missing handlers, missing modules/attributes, excess arguments, malformed bindings, foreign object IDs, non-finite JSON, overflows, unavailable hosts and cross-thread/fiber escapes.

Do not mark `@python` delivered merely because decorators parse. Frontend/runtime integration must execute a real call and exercise failures through the same path. The Python frontend currently reserves/rejects the marker until this contract is implemented.

## Python team workflow

The installed [LLM dataset audit](../../python/osprey/dataset_audit/README.md) is the substantial application: Python teams can clean local SFT datasets, detect evaluation contamination/conflicting answers and enforce actual tokenizer context budgets before training. Native Osprey Python-flavor effects drive Python services, which call native Osprey policy back for every row. SQLite preserves cross-batch indexes; fresh workers constrain retained native heap lifetime. Atomic publication and all-records transaction verification make callback failures explicit. The acceptance run covers all 15,011 Dolly records with a real GPT-2 tokenizer, without model weights or a GPU. Privacy and near-match limitations are documented alongside the results.

The loader supports editable/wheel installation and runnable effects examples. Python-flavor GPT-2 generation already compiles and runs. The remaining model task is interoperation with the existing Python stack: Python owns checkpoint/model ecosystem; Osprey owns a checked native calculation; both directions use the same bridge. Keep the independent reference unchanged. Mixed projects, installed compiler/runtime discovery and model-stack acceptance remain further work; installed-package execution outside the checkout and module import failures already have evidence.

General aggregate exports, list/record conversion and automatic import hooks need a versioned ABI design; they must not guess layouts from names. Python decorators for compiling source fragments should not execute a function to infer its Osprey type. Existing `.py` ownership selection remains the Python Flavor contract.

## Tensor storage and GPU acceptance

Opaque handles are delivered independently of zero-copy storage. Design a buffer/DLPack bridge with explicit dtype, device, shape, strides, contiguity, bounds, read/write ownership, producer lifetime, capsule single consumption and stream synchronization. Reuse existing Osprey buffer validation and vendor-neutral kernels. Reject incompatible devices and layouts or require an explicit copy; do not label a copy zero-copy. A CUDA tensor is not inherently compatible with Metal/OpenCL storage.

Use tiny deterministic arrays before GPT-2: preserve object/storage identity for legal views, survive producer deletion until consumer release, reject double capsule consumption, test unsupported dtype/device/stride, and compare outputs after synchronization. Device acceptance requires a real selected device and dispatch evidence. No hardware or optional library means unverified acceptance, not a passing GPU test. Full checkpoint generation/training remains coordinated with plan 0031.

## Runtime and deployment follow-through

Add reclaiming memory backends and stable returned-allocation release before claiming bounded memory in long-running services. Define reentrancy and GIL release before parallel Python calls. A fiber cannot transport a mutable Python handle until isolation/ownership rules explicitly permit it. Standalone native executable calls require optional, version-checked CPython embedding with initialization/finalization and interpreter affinity; current `python -m osprey` is Python-hosted execution. Keep embedded Python optional for ordinary Osprey programs.

## Acceptance checklist

- [x] Native scalar export/import bridge and embedded ABI metadata.
- [x] Exception envelopes, callback lifetime and explicit object-reference protocol.
- [x] Default/ML/Python-subset end-to-end integration tests wired into Cargo.
- [x] Runnable effects/Python-flavor examples and installed-wheel smoke test.
- [x] Installed real-world dataset-audit application with native Python-flavor effects and bidirectional calls per record.
- [x] Complete public Dolly/GPT-2-tokenizer acceptance, plus malformed-input, cross-batch, privacy and callback-rollback checks.
- [ ] Full cross-flavor project/editor/target validation against the current frontend and host bridge.
- [ ] Typed `@python` binding AST/effects/types and all-flavor direct-call execution.
- [ ] Python-hosted GPT-2 interoperability example; standalone Python-flavor generation is already implemented.
- [ ] Tensor storage ownership/stream bridge with real device evidence.
- [ ] Reclaiming memory, concurrent host calls and optional standalone embedding.

Only completed, executed work updates public support claims. Each later capability needs its own tests before the checklist changes.
