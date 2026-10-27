# Plan 0032 — Python interoperability

**Contract:** [Python interoperability](../specs/platforms/0042-PythonInterop.md). **Status:** native Python-hosted bridge and installed bidirectional LLM dataset-audit application implemented; declarative typed bindings, GPT-2 model integration, storage interchange and standalone embedding remain open.

The entry workflow is a Python team compiling the parts of its application written in Osprey, calling native functions from its existing Python process, and continuing to call established Python libraries from Osprey. Teams can adopt algebraic effects without replacing the interpreter, environment or model stack. Native compilation is not evidence of a workload speedup: measure boundary overhead and complete workloads before performance claims.

## Scope and coordination

This plan owns the Python host target, checked ABI, runtime callbacks, loader package and interop tests. [Plan 0031](0031-python-flavor.md) owns syntax/frontend and authoring-tool coverage. Follow [repository instructions](../../AGENTS.md), preserve other owners' changes and coordinate validation in the primary checkout.

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
