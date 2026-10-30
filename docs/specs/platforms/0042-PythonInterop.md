# Python interoperability

**Status:** Python-hosted native scalar calls, exception envelopes and opaque object handles are implemented and covered by the interop integration suite. Declarative `@python` bindings, standalone CPython embedding and tensor-storage interchange are delivery targets, not shipped behavior. [Plan 0032](../../plans/0032-python-interop.md) tracks those gaps. The Python flavor belongs to [spec 0041](../language/0041-PythonFlavorSyntax.md); the bridge does not execute Python-flavor source as CPython.

## Host and compilation — [PYTHON-HOST]

`osprey source-or-project --compile --target=python -o library.so --memory=arc` builds a native shared library and matching `.h`; macOS output ends in `.dylib`. The default output uses the same extension. Linux and macOS are supported; Windows, WebAssembly and mobile hosts are outside this target. The target defaults to ARC and rejects tracing GC; the matching managed runtime must be built before linking. Debug, profiling, `--entry-only`, and executable `--run` are rejected. `--check` validates the host ABI without linking or importing Python modules. `--llvm` emits the adapted native library IR. Ordinary frontend, type, capability and export-effect checks still apply.

The synchronous host target rejects dynamic control continuations, fibers, HTTP/WebSocket and process-spawning operations. Value effects and static discharge remain available. Native fiber callbacks cannot safely enter a Python host whose loading thread retains the GIL; rejecting them is part of the host contract, not an optional runtime convention.

The `python/` package installs as `osprey-interop` and imports as `osprey`. `osprey.build(path, output=None, compiler=None)` invokes the compiler and loads its artifact. `osprey.load(path)` loads a previously built artifact. `python -m osprey path` compiles and runs an entry inside the Python host. The host installs callbacks before running the program's top-level statements and `main`, exactly once per loaded library. Repeated loading, including symlink/hard-link aliases, returns the existing native image and its object registry. Rebuilding over a loaded path or its hard-link alias is rejected; loading a previously loaded path replaced by a different file is also rejected. Reentrant initialization/loading is an error. Compilation never imports or runs the Python modules named by foreign calls.

## Initialization and lifetime — [PYTHON-TARGET-ENTRY]

Callbacks and loaded libraries remain rooted for the host process lifetime. Calls stay on the loading Python thread. The loader retains the GIL while calling native exports; moving CPU-heavy work off the GIL or supporting concurrent Python threads requires a separate ownership/runtime contract. Callbacks may invoke ordinary Python APIs, which can release the GIL internally. No second interpreter, subprocess RPC service or Python development headers are needed.

## Python calls Osprey — [PYTHON-HOST-ABI]

The target reuses the compiler's checked scalar C host boundary. An emitted monomorphic function is callable when all parameters and its return have a supported representation. Parameters support `int`, `float`, `bool`, and `string`; returns additionally support `Unit`. Osprey integers are signed 64-bit, floats are binary64, booleans use the native C convention, and strings are NUL-terminated UTF-8. Generic, aggregate and closure signatures are excluded from this initial boundary, and so is the opaque `Ptr` the app-library slices accept: an export that passes one is a compile error, because the Python wrapper has no representation for it. `main` is initialization, not an ordinary export. Every exported function must discharge its own effects; Python is not an implicit algebraic-effect handler.

The artifact embeds versioned JSON signature metadata in `osprey_python_abi()`. The Python loader reads that metadata from the loaded library, preventing stale sidecar signatures from configuring native calls. `library.function(args...)` calls an unqualified export; `library.call("namespace::Module::function", args...)` also supports qualified names and names that overlap Python loader methods. C exports retain the existing `osprey_` naming and collision checks. Bridge symbols are reserved.

The Python wrapper validates argument counts, exact primitive types, signed integer range and string encoding before native calls. Python `bool` is not an Osprey `int`. Embedded NUL, `None` and byte strings are rejected for string parameters. Returned strings are immediately copied into Python strings and are never freed by Python. The unmanaged `none` allocator retains general allocations: this target does not yet promise bounded native allocation over an indefinitely running service or a native returned-string release API.

```python
import osprey

model = osprey.build("model/osprey.toml")
score = model.score(3.0)
```

`score` above must be an emitted scalar export; building a project does not make a generic tensor function callable through the scalar ABI.

## Osprey calls Python — [PYTHON-CALL]

The implemented low-level declaration is:

```osp
extern fn osprey_python_call(module: string, name: string, args: string) -> string
```

Python flavor spells the same declaration `@extern` above a fully typed signature-only `def ...: ...`; [the runnable example](../../../examples/python/interop.py) executes both directions through this shared boundary. This verifies the frontend subset used there, not every planned Python surface form.

`module` is an importable Python module, `name` a dot-separated attribute path, and `args` a JSON array of positional values. The host imports the module at runtime and calls the selected object. Python uses the active host's environment and module search path. Arguments and results are JSON values; this entry serializes values and is not the tensor path.

Success is `{"ok": value}`; failure is `{"error":{"type":"ExceptionClass","message":"description"}}`. Import, attribute lookup, argument decoding, invocation and result encoding failures share that envelope. Exceptions including `SystemExit` and `KeyboardInterrupt` become explicit failures; none unwind through C or become a pretend scalar success. Invalid JSON/non-finite JSON numbers fail. A reply that itself cannot be encoded uses a fixed error envelope. The C shim copies each reply into the selected Osprey allocator before the host may replace its temporary callback buffer. A missing installed host also returns an error envelope.

Osprey callers must decode the envelope, preferably into `Result`, and can put this capability behind an ordinary effect with live and deterministic test handlers. The current raw `extern` has the same trust boundary as C FFI: it is not automatically an effect declaration, and `--no-ffi` rejects it. Restrictions on Osprey builtins do not sandbox arbitrary Python libraries. Calling Python is never a GPU-kernel operation.

## Python objects and tensors — [PYTHON-OBJECT]

Python objects stay in a strong-reference registry owned by the loaded library. `library.keep(value)` returns an opaque positive integer; `library.resolve(id)` returns the same Python object, and `library.release(id)` drops that reference. Handles are not pointers, cannot be dereferenced in Osprey, and never contain tensor storage. Each allocation has a process-unique ID and is validated against its owning registry; stale, foreign and forged handles fail. Explicit release is required, and dropping a handle integer in Osprey does not imply release.

```osp
extern fn osprey_python_object(operation: string, objectId: int, name: string, args: string) -> string
```

All object operations return the same success/error envelope:

| Operation | Inputs | Successful result |
| --- | --- | --- |
| `import` | module name in `name` | new module handle |
| `getattr` | object handle and attribute `name` | new attribute handle |
| `call` | callable handle, or object handle plus method `name`; request in `args` | new result handle |
| `call_value` | same request as `call` | JSON result directly, or an encoding error; no new handle |
| `value` | object handle | JSON value, or an encoding error |
| `release` | object handle | JSON null |

A call request is `{"args":[token...],"kwargs":{"name":token...}}`. A token is exactly `{"value":jsonValue}` or `{"ref":objectId}`. Missing `args`/`kwargs` mean empty collections. This explicit distinction allows arbitrary dictionaries, including dictionaries with a `ref` field, to pass as values without accidentally becoming references. Handles can refer to callables, arrays, PyTorch/JAX tensors, modules, streams or other Python objects. The foreign operation must itself accept those arguments; the bridge grants no extra backend compatibility.

The handle path does not serialize or copy the referenced payload. Calling a Python tensor method can still allocate, synchronize or transfer data according to that library's own semantics. It does not make Python tensor storage an Osprey `GpuBuffer`, transfer a CUDA allocation to Metal/OpenCL, or prove Osprey kernel execution. GPU claims require actual device/dispatch evidence from the exact workload. DLPack, buffer-protocol views, dtype/shape/stride validation, producer lifetime, stream handoff and backend-compatible zero-copy storage transfer remain separate work.

## Training-data application — [PYTHON-DATASET-AUDIT]

The installed `osprey-dataset-audit` application uses the same bridge in both directions for every record. A native Python-flavor `Audit` effect drives Python JSONL/gzip, SQLite and optional offline Hugging Face tokenization services; the Python service calls native Osprey acceptance policy back. No separate native Python runtime or application-specific C shim is introduced. [The application guide](../../../python/osprey/dataset_audit/README.md) defines supported record schemas, formatting, policy priorities, outputs and reproducible public-data acceptance.

Evaluation prompts are indexed before training. Exact identity uses configurable normalization and includes chat history/roles; conflicting completions are distinguished from duplicate targets. Optional indexed near matching verifies candidate Jaccard scores without promising exhaustive recall. Privacy detection is explicitly heuristic. Requested token budgets require the actual local tokenizer; chat uses its model chat template. Clean outputs preserve original JSON values. Diagnostic artifacts identify rejected records without republishing their text.

Input files are read-only and fingerprinted. Invalid/oversized lines are quarantined independently. A worker commits only after every requested record has a valid native decision: callback errors or missing decisions roll back the entire batch. Fresh worker processes limit the lifetime of the unmanaged `none` allocator's retained allocations; they do not establish a fixed byte memory ceiling. Output publication is exclusive and occurs only after all workers and exports succeed. Operational failures leave no completed output directory. Working databases containing original input content are removed unless explicitly retained.

Acceptance includes a complete 15,011-record Dolly run with a real local GPT-2 tokenizer, independent token-count checks and failure/rollback tests. This proves an actual data-preparation workload, not GPU computation, a speedup or tensor-storage interchange. Ordinary application tests use stdlib input files and require no model/dependency downloads.

## Declarative bindings — [PYTHON-BINDING]

This section is a normative delivery target. The current Python frontend must reject the marker until its lowering and checks are implemented. A Python-flavor declaration reserves this spelling:

```python
@python("math", "sqrt")
def square_root(value: float) -> Result[float, str]:
    ...
```

The decorator is compile-time Osprey syntax, not a Python function that runs at import time. It binds a literal module and qualified callable to a signature-only declaration, making the use site `square_root(value)`. No implicit fallback from unresolved Osprey names/imports to Python is allowed. Ordinary imports keep their logical Osprey meaning. The declaration is explicit about the foreign result; Osprey inference still removes redundant annotations at its callers.

The canonical boundary must be flavor-independent: Default/ML bindings must expose the same foreign metadata, types, effects and diagnostics. A foreign call requests the Python capability, returns `Result<T, PythonError>` (the example's explicit string error is the initial envelope adapter), and supports mocked handlers. Dynamic Python exceptions and conversion failures must remain errors; checked integer conversion cannot truncate. Arbitrary Python objects use a nominal opaque `PythonObject`, not `any`, `Ptr`, or automatic record conversion. Only a host handler executes Python. Exported Osprey functions callable by Python must discharge this capability internally, as with every other effect.

Initial declarative binding support should cover the scalar types above plus explicit opaque object parameters/results, positional calls and named parameters. Unsupported variadics, defaults, asynchronous results, descriptor behavior and coercions require diagnostics rather than best-effort emulation. Python inspection must not run during compilation. If an actual runtime value disagrees with the declared type, return a conversion error. Error type and effect metadata must be finalized in the shared AST/type contract before marking this section delivered.

## Verification — [PYTHON-TESTS]

`cargo test -p osprey-cli --test python_interop` runs the package integration suite against Cargo's compiler. It proves native calls both ways in Default, ML and the Python frontend subset used by the example, UTF-8, scalar bounds/type rejection, callback rooting and copied responses, import/call/encoding errors, object identity/lifetime/keyword arguments, and rejection of unsupported options/signatures. Compiler ABI regression tests continue to cover symbol collisions, initialization and unresolved export effects.

The ordinary suite must not download weights or require PyTorch, NumPy or a GPU. Optional tensor/device acceptance must record actual libraries, devices, storage identity, stream ownership and dispatch; a tensor-shaped test double proves the opaque object protocol only. The existing required checks remain enforced. A library loading or native-call crash is a test failure, never an import-success claim.
