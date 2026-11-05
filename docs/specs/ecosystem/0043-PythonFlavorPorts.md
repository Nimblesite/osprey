# Python-Flavor Package Ports

**Status: in progress.** The [`src/osprey/packages/`](../../../src/osprey/packages) tree holds Osprey ports of popular CPython packages, written in the [Python flavor](../language/0041-PythonFlavorSyntax.md) and tested against the real CPython packages. `@osprey/testing` ships and is the harness every port reports through. The catalog distinguishes implemented subsets, ongoing ports and planned packages. The registry accepts Python-flavor packages ([PORT-PUBLISH]).

## What a port is — [PORT-SCOPE]

A port is an Osprey package that reimplements selected APIs and numerical behavior of one named CPython package, such as numpy, torch or tiktoken. It is Osprey source in the Python flavor. It is compiled by `osprey` and runs with no CPython interpreter, no Python interop ([spec 0042](../platforms/0042-PythonInterop.md)) and no binding to the original package's C or CUDA code. Calling CPython from a port is rejected in review. Interop is how an Osprey program reaches a package that has not been ported, not how a port is built.

A port keeps the original's names and argument order wherever the Python flavor can spell them. A Python call `np.sum(a, axis=0)` becomes `Array.sum_axis(a, 0)` only where Osprey cannot express the original: optional arguments, `None` sentinels, keyword-only defaults, operator overloading and in-place mutation. Values are immutable. Expected failure is a `Result`, never an exception. Integer arithmetic performs `Arith` operations and leaves the policy to the caller, as every Osprey library does ([spec 0037](../language/0037-ArithmeticEffects.md)). Each package's `README.md` names the original package and the version it was compared with, maps every ported function to its original, and lists each divergence with the reason.

A port is behaviourally compatible where its suite says so and nowhere else. Python spelling alone is not a compatibility promise ([FLAVOR-PYTHON-BOUNDARY]).

## Layout — [PORT-LAYOUT]

```text
src/osprey/packages/<name>/
  osprey.toml            [project] with python_sources, [package], [dependencies]
  README.md              original package and version, API map, divergences
  src/*.py               the port: `# osprey: namespace=<name>` and one module per file
  tests/osprey.toml      the suite project; entry main.py
  tests/main.py          the assertions, reported through @osprey/testing
  tests/expected.py      generated Osprey constants module holding the oracle's values
  tests/expected_output.txt   the golden transcript
  hunt/                  optional `osprey hunt` drivers ([PORT-HUNT])
```

The namespace is the Python package's import name. Each Python submodule or concept becomes one Osprey module with a PascalCase name, so `from numpy import Linalg` reaches `numpy.linalg`. Functions keep their Python snake_case names. Files stay under 500 lines and functions under 20.

`[project].python_sources` lists every `.py` file the project compiles. Every `.py` file in a package tree is Python-flavor Osprey and begins with `# osprey: flavor=python`, because a release refuses any other `.py` file ([PACKAGE-SOURCE-CANONICAL]). The CPython oracle for a package therefore sits beside its sources as `oracle/<name>.cpython`: an ordinary CPython script whose suffix is not `.py`, shipped with the package together with the notices for anything it retains. A dependency on another port appears both in `[dependencies]`, for the registry, and as a `source_roots` and `python_sources` entry in the suite project. The suite entry needs the latter until projects resolve dependencies themselves.

## Device placement — [PORT-DEVICE]

A port that computes over tensors performs a static storage effect and never names a buffer type, as the GPT-2 port's `Tensor` effect does ([FLAVOR-PYTHON-GPU], [STAGE-BACKEND]). A host handler answers it with lists. A GPU handler answers it with `GpuBuffer` operations, and those kernels run on Metal or OpenCL when `OSPREY_GPU` selects a device ([spec 0034](../runtime/0034-GPUComputation.md)). The handler the program installs decides where arithmetic runs: there is one body for both, with no CuPy-style duplicate API. Each such suite runs its assertions under both handlers. Kernel purity, scalar elements, explicit transfers and arithmetic at the declared element width follow spec 0034. A port states the numeric type its tests exercise; conversion to binary32 is `toFloat32`, and conversion to binary16 is `toFloat16` ([BUILTIN-CONVERT-FLOAT](../runtime/0012-Built-InFunctions.md)).

## Tests against the real package — [PORT-TEST-ORACLE]

Every assertion compares the port with a value that the real CPython package computed. `src/osprey/packages/<name>/oracle/<name>.cpython` imports the original package, computes each case in its `main()` and writes `tests/expected.py`, a Python-flavor constants module, through the shared writer `src/osprey/packages/testing/oracle/expected_module.cpython`, which documents every constant. It records the CPython version and the package versions in its docstring. An oracle runs as `python -I src/osprey/packages/testing/oracle/oracle.cpython <name>`: the runner keeps `oracle/` off `sys.path`, because an oracle is named after the package it imports (`numpy.cpython`, `torch.cpython`, `re.cpython`) and would otherwise import itself. The oracle runs in a development virtual environment, not in CI. CI compares against the committed `expected.py`, so it needs neither CPython nor the package. Regenerating `expected.py` must be deterministic, so a case that the original computes from random state seeds it explicitly.

Integers, booleans, strings, shapes and token ids compare exactly. Floats compare with numpy's `isclose` rule, `abs(got - want) <= atol + rtol * abs(want)`, with equal infinities equal, opposite infinities unequal and NaN equal to NaN. A case states its own `rtol` and `atol`. A value computed only from `+ - * /` and `sqrt` is correctly rounded on every host, so the original's float64 result is compared at `rtol = 1e-12` or tighter. Values downstream of `exp`, `ln` or `tanh` differ in the last digits between libm implementations ([BUILTIN-MATH]) and use `rtol = 1e-9`. A looser tolerance names its reason beside the case. A port never weakens a tolerance to pass, and never copies the port's own output into `expected.py`.

## Upstream test parity — [PORT-UPSTREAM-TEST-PARITY]

A passing golden or high line coverage does not establish upstream behavioral parity. For each supported public API, record the pinned upstream test source/version and map its input families to assertions in the existing end-to-end suite. Include invalid inputs, interacting options, asymmetric values, empty/singleton cases, Unicode and numerical boundaries where relevant. Preserve old assertions when expanding a corpus. Counts describe the measured corpus, never an unmeasured whole upstream suite; missing families and unsupported APIs remain explicit unfinished work. Reference expectations come from the real pinned upstream implementation under [PORT-TEST-ORACLE].

## Typed HTTP request bodies — [FASTAPI-NAP-VALIDATION]

Body binding must decode at the HTTP boundary into typeDiagram-generated records before invoking domain handlers. Tests use the actual generated NAP `BundleFileWriteRequest`, `ApiKeyCreateRequest`, `EndUserExecutionContext` and `EmbeddedResource` as the initial oracle set, with contract and schema hashes; this set does not complete NAP model parity. Required behavior includes field-order error accumulation, original typed error inputs, nullable defaults, required fields, UUID conversion, wire/name aliases and alias precedence, strict extra-field rejection, response filtering, media types and exact JSON error locations. Nested/composed models, arrays and remaining NAP contracts are still required.

The independently recorded corpus must run over real HTTP under both GC and ARC, including during native coverage collection. Duplicate keys, non-finite values, malformed JSON and escaped NUL remain tested even while they expose differences from the shared strict JSON codec; weakening that codec's default policy or dropping cases is not an acceptable compatibility fix. A failing parity suite cannot produce passing package coverage evidence.

The explicit `jsonDecodePython` decoder implements first-key insertion order with the final value at every object depth, including equivalent escaped key spellings. It must handle large duplicate sets in O(n log n), preserve malformed-input rejection and keep the existing strict `jsonDecode` duplicate-key behavior. It additionally accepts Python's NaN/infinity literals and float overflow before model validation, so ignored or overwritten non-finite inputs may succeed while unencodable validation errors produce FastAPI's plain 500. Strict `jsonDecode` retains finite-only numbers. Native int64, nesting and NUL-free string limits remain explicit; this decoder alone does not establish complete CPython JSON compatibility.

## Browser access policy — [FASTAPI-NAP-MIDDLEWARE]

The Python-flavor FastAPI port must wrap routing and its error replies in the NAP CORS policy: wildcard origins without credentials; GET, HEAD, POST, PUT, PATCH and DELETE; Authorization, Content-Type, If-None-Match, X-API-Key, X-Tenant-Id and X-Trace-Id request headers; ETag and X-Trace-Id exposed response headers. Policies are generated typed records from `fastapi/models.td`.

Real HTTP acceptance must compare exact status, body and security-relevant headers against pinned FastAPI/Starlette for absent, empty, null, exact and denied origins, method/header preflights, mixed-case headers, credential reflection, Vary merging and error responses. Preflight failures accumulate their ordered reasons and never dispatch the endpoint. The same corpus must execute during native GC/ARC source coverage collection. Regex origins and allowed private-network access remain unimplemented; private-network preflights are rejected explicitly. Existing coverage ratchets remain enforced even while a new implementation falls below them.

## HTTP benchmark subset — [FASTAPI-BENCHMARK-SUBSET]

`make -C src/osprey/packages fastapi-benchmark` measures the implemented health and path/repeated-query routing subset against pinned CPython FastAPI. Every real HTTP reply must satisfy exact status, content type, byte length and body assertions before contributing a timing. GC, ARC and CPython servers run sequentially with matched requests, explicit response encoding and access logging; test-only trace middleware is disabled and that mode has acceptance assertions. Record compiler/reference versions, binary hashes, workers, warmups and trials. Report median trial throughput, median of trial median latencies, nearest-rank p95 per trial and sampled RSS with its sampling limitation. Model binding, dependency injection, database, streaming and lifespan performance remain unmeasured. Both HTTP drivers refuse disabled Python assertions and bound their owned processes with the shared package process guard.

## The suite — [PORT-SUITE]

`src/toolchain/crates/osprey-cli/tests/python_packages.rs` discovers every library with both `src/osprey/packages/<name>/osprey.toml` and `src/osprey/packages/<name>/tests/osprey.toml` during `cargo test`. The package manager's projects live under `src/osprey/packages/manager/` and run through `make -C src/osprey/packages registry-test`. Under each of the default, GC and ARC memory backends, each library suite must:

- exit successfully;
- print exactly `expected_output.txt`: one `ok <case>` line per assertion and a final `N passed, 0 failed`;
- under ARC, exit with zero live objects.

A suite runs its assertions under `testing.Check.strictly`, so an integer overflow or zero division stops it and the transcript differs from the golden. The same test requires every package to declare an `@osprey/<name>` `[package]` and a README, every `python_sources` path to lie inside a `source_roots` entry, every `.py` file in the package tree to begin with `# osprey: flavor=python`, and `tests/expected.py` to carry the shared writer's provenance line naming an existing `src/osprey/packages/<name>/oracle/<name>.cpython`.

Every name a module's `__all__` exports must be spelled somewhere in the suite's own sources (`src/osprey/tests/*.py` other than `expected.py`). An export the suite never exercises fails `cargo test`, so a port cannot ship an untested public function, and a private helper cannot hide in the public API: it is tested or it leaves `__all__`.

## Individual source coverage — `[PKG-COVERAGE-GATE]`

Every library has its own entry in `coverage-thresholds.json` under `projects.packages`, with a threshold of at least 90%. `make -C src/osprey/packages package-coverage` collects and checks each suite independently. A package's numerator and denominator include only its own `src/` lines reported by the compiler; tests, helpers and other packages cannot increase its result. This is source line coverage, not branch coverage or proof of correctness.

`scripts/packages/package_coverage.py` requires a successful suite execution, a positive assertion count, zero failures and byte-for-byte matching `tests/expected_output.txt`. It rejects missing reports, omitted executable source files, malformed counts and results below the package's threshold. Collection binds the report to hashes of its source, suite, manifests and golden, so changed inputs or altered evidence require a new run. A failing suite remains a failure even when its measured coverage exceeds 90%.

`projects.packages` contains required minimums, not measured percentages. `python3 scripts/packages/package_coverage.py --record-measurements` records a dated, verified snapshot in the separate `package_measurements` section of the same file, including the measurement basis fingerprint. Unavailable or stale evidence is recorded as `null`; measured results below a threshold retain their actual counts. Recording never changes the minimums or turns a failing gate into a pass. CI always checks the collected reports rather than the recorded snapshot.

`make test` runs collection and checks locally. CI collects in its package source coverage job and the required coverage job checks the uploaded evidence. `python3 scripts/packages/test-package-coverage.py` exercises the gate's failure cases without compiling packages.

## Bug hunts — [PORT-HUNT]

Code that consumes input, such as tokenizers, pattern parsers and shape and index validation, gets `osprey hunt` drivers ([SymbolicExecution](../tooling/SymbolicExecution.md)). Each driver is its own project under `hunt/<driver>/`, which compiles the package's sources and reads its input from stdin to drive the public API. Numeric drivers mix symbolic values with concrete NaN, infinities, huge and subnormal values, so the non-finite paths are explored on every run. A driver installs no `@control` handler: each resumable control region runs on its own thread, which symbolic execution cannot follow ([SYMEX-MOCK-UNSUPPORTED](../tooling/SymbolicExecution.md)). A driver discharges `Arith` with a value-mode handler that answers each fault with a value, so the suite's `Check.strictly` policy is exercised by the suite and not by the hunt. The package README records each driver, the paths explored and the findings. Hunt explores paths within the configured symbolic inputs, runtime models and resource budgets. Record incomplete paths, unsupported operations and budget exhaustion; a run with no findings is not a proof that the port is correct. Each finding that reproduces natively is fixed in the port, or in the compiler when the compiler is at fault, and becomes a regression case in the suite. A finding that does not reproduce is recorded in the package README. Numerical code without parsed input is hunted adversarially instead: edge shapes, empty and singleton axes, infinities and NaN, and extreme magnitudes are added as oracle cases.

## Publication — [PORT-PUBLISH]

Each port publishes as `@osprey/<name>`, epoch 1, release ordinal 1, from its package directory. Its `[dependencies]` name the `@osprey` ports it uses. A published manifest may declare `[project].python_sources`: root-confined relative paths ending in `.py`, part of the canonical manifest and its digest, each inside one of the manifest's `source_roots` (`manifest.python-source-unrooted` otherwise) ([PACKAGE-MANIFEST]). Every `.py` file in a release tree, listed or not, must begin with the line `# osprey: flavor=python`, or the registry server and client both refuse the release with `source.python-flavor-header` ([PACKAGE-SOURCE-CANONICAL]). A release therefore never ships CPython code that could be mistaken for Osprey, and installing a port executes nothing. [PORT-SUITE]'s publishability test checks both rules for every package in this tree, so a port that would be refused fails `cargo test` first.

## Catalog — [PORT-CATALOG]

| Package | Ports | Status |
| --- | --- | --- |
| `@osprey/testing` | numpy `isclose`/`allclose` assertions, the strict `Arith` policy | shipped |
| `@osprey/numpy` | `ndarray` creation, shape, broadcasting, elementwise, reductions, `matmul`, `linalg`, host and GPU handlers | in progress |
| `@osprey/torch` | `Tensor`, `nn.functional`, reverse-mode autograd, `nn` modules, `optim` | in progress |
| `@osprey/micrograd` | Karpathy's micrograd `Value` engine (add, mul, pow, relu, tanh, exp, backward) and `nn` (Neuron, Layer, MLP) against micrograd 0.1.0 and torch (1,670 oracle assertions) | shipped |
| `@osprey/tiktoken` | byte-level BPE encoding, decoding and training | in progress |
| `@osprey/tokenizers` | Hugging Face WordPiece, Unigram and WordLevel models, BERT normalizer, pre-tokenizers, decoders and the BERT template post-processor against tokenizers 0.23.3 (1006 oracle assertions) | shipped |
| `@osprey/chromecast` | a CAST v2 sender: dns-sd discovery, protobuf framing, a heartbeat-answering TLS session, LAUNCH/LOAD and the media-status loop, against the reference stdlib sender script (201 oracle and scripted-device assertions) | shipped |
| `@osprey/einops` | `rearrange`, `reduce`, `repeat` patterns | in progress |
| `@osprey/sacrebleu` | BLEU (every smoothing method, effective order, multiple references), chrF2 and chrF++, the `none`/`13a`/`char` tokenizers, against sacrebleu 2.6.0 (3084 oracle assertions) | shipped |
| `@osprey/rouge-score` | `RougeScorer` for rouge1/2/L/Lsum, its tokenizer and nltk's Porter stemmer, against rouge-score 0.1.2 (1137 oracle assertions) | shipped |
| `@osprey/faiss` | `IndexFlatL2`/`IndexFlatIP`, ID maps, selectors, `normalize_L2`, k-means with explicit init, product and scalar quantizers, against faiss-cpu 1.15.1 (482 oracle assertions) | shipped |
| `@osprey/re` | CPython's `re`: pattern parser, backtracking matcher, `match`/`search`/`findall`/`sub`/`split` | in progress |
| `@osprey/safetensors` | the safetensors header and tensor layout, every dtype, `save` byte-identical | in progress |
| `@osprey/gguf` | GGUF v3 reader, tensor layout and a header read that leaves weights on disk against gguf 0.19.0, plus block decoders from stored bytes to binary32 for the nine tensor types the chosen weights file uses, compared with ggml's own compiled `dequantize_row_*` (758 oracle assertions); the writer stays out of scope until a phase needs it | in progress |
| `@osprey/jinja2` | the template lexer, parser, expressions, filters and tests | in progress |
| `@osprey/pydantic-ai` | the pydantic-ai 2.46.0 agent engine: messages and their wire form, `OpenAIChatModel` request/response mapping, `TestModel`, direct requests, the tool-calling `Agent` loop, against pydantic-ai over a mock transport (111 oracle assertions) plus a live DeepSeek agent run judged by a model | shipped |
| `@osprey/scipy` | `scipy.special` functions | in progress |
| `@osprey/sklearn` | metrics, preprocessing and model selection over lists against scikit-learn 1.9.1 (312 oracle assertions) | shipped |
| `@osprey/transformers` | generation layer of transformers 5.20.2 and torch 2.14.1 at binary32: 19 logits processors and warpers, `LogitsProcessorList` order, softmax/argmax/inverse-CDF sampling, stopping criteria, a model-independent `generate` driver (395 oracle assertions and two empty-row controls) | generation shipped; LLaMA/GPT-2 models in progress |
| `@osprey/peft` | LoRA adapters | planned; no package source yet |
| `@osprey/bitsandbytes` | int8 absmax and 4-bit NF4 quantization | planned; no package source yet |
| `@osprey/flash-attn` | `flash_attn_func` and `flash_attn_varlen_func` as the online-softmax tiled pass: causal and sliding-window masks, ALiBi, GQA head mapping, log-sum-exp, against torch SDPA in float64 (1035 oracle assertions) | shipped |

A row moves to shipped only when its suite is in [PORT-SUITE] and passes on every backend.
