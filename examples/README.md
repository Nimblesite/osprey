# Osprey Examples

Start with algebraic effects: declare an operation, request it with `perform`, and run the same work under different callable handlers.

| Example | What to try |
| --- | --- |
| [Handlers](handlers/README.md) | Reusable policies, return transformations, static selection, and comparisons with Koka and OCaml |
| [Talon Bank](projects/modules/README.md) | Shared web/iOS/Android application, SQLite and mock storage, auditing, metrics, and platform rendering |
| [Issue Inbox](mobile/README.md) | Shared iOS/Android state and UI, with SQL/HTTP requests interpreted by a platform-command handler |
| [iPhone counter](ios/README.md) | A native logging callback behind a value effect |
| [Storage browser](macos/README.md) | One SwiftUI app for the Mac and iPhone: Osprey owns every screen of a folder-by-folder storage browser, with hidden files on or off |
| [Graphics scenes](graphics/README.md) | Osprey runs three real-time scenes; a Swift Metal host draws them on macOS and a Direct3D 12 bridge on Windows |
| [GPT-2](gpt2/README.md) | The published GPT-2 weights, generation and a training step written once against a static `Tensor` effect and run by two handlers — host lists or the GPU surface, on the CPU or on Metal or OpenCL — with identical output on the CPU, checked against a pure-Python reference |
| [Python application ports](python/ports/README.md) | Train a micrograd classifier, rank vectors with a FAISS-style index, and evaluate text with ROUGE using Osprey's Python syntax flavor |

`handler E { ... }` creates a callable implementation; `h(work)` installs it while the callback runs. `handle E { ... }` handles the rest of its block. Default (`.osp`) and ML (`.ospml`) support these forms; the [Python syntax flavor](../website/src/docs/python-flavor.md) uses decorators for effects and handlers. See the [effects contract](../docs/specs/language/0017-AlgebraicEffects.md) and [current limits](../docs/plans/0016-algebraic-effects-and-handlers.md).

## Syntax flavors

A syntax flavor is a way to write Osprey: Default uses braces, ML uses indentation and curried calls, and Python flavor uses familiar Python-style syntax. All three share the compiler, types and effects.

| Selector | Effect |
| --- | --- |
| `.ospml` extension | ML flavor |
| `// osprey: flavor=ml` leading marker | ML flavor |
| `--flavor ml` CLI flag | ML flavor |
| `.osp` extension (default) | Default flavor |
| `.py` extension passed directly to the compiler | Python flavor; examples start with `# osprey: flavor=python` |
| `# osprey: flavor=python` marker | Python flavor |
| `--flavor python` CLI flag | Python flavor |

Precedence: **flag > marker > extension > Default**. Without a flag, a marker that conflicts with the extension is an error. Choose one syntax flavor per file and import across flavors in a project. Projects select Python-flavor files explicitly through `python_sources` or a `.py` entry in `osprey.toml`; other `.py` files are ignored. See the [Python-flavor project example](../website/src/docs/python-flavor.md). The [mobile application](mobile/README.md) combines a Default C ABI entry file with shared ML modules.

The differential harness ([`../crates/run_test_corpus.sh`](../crates/run_test_corpus.sh))
discovers programs additively across both flavors. A `.osp`/`.ospml` twin that
produces identical output shares a single `.expectedoutput` file — one golden
proving both flavors print the same bytes.

## Directory layout

- **`../tests/regressions/`** — working programs that compile and run; output is checked
  byte-for-byte against `.expectedoutput`. Subfolders: `basics/`, `db/`,
  `effects/`, `fiber/`, and `http/`.
- **`../tests/flavors/`** — paired Default/ML executable documentation with
  internal state assertions and edge cases.
- **`failscompilation/`** — programs the compiler must reject, each paired with the
  expected diagnostic.
- **`api/`, `db_postgres/`, `statefulhttp/`, `websocketserver/`, `tui/`, `wasm/`** —
  larger application/runtime examples.
- **`bugs/`** — regression reproductions.
- **[`ios/`](ios)** — a SwiftUI iPhone app calling Osprey logic through a generated C header, with device/simulator builds and executable smoke checks.
- **[`macos/`](macos/README.md)** — a SwiftUI storage browser for the Mac and iPhone on the same C boundary: Osprey owns the screens, Swift draws them and measures the disk.
- **[`mobile/`](mobile/README.md)** — the same reactive issue inbox on iOS and Android, with Osprey modules defining the native screen tree, state, SQLite persistence, GitHub requests, and triage workflows.

## Native mobile application

[Issue Inbox](mobile/README.md) has been exercised on a physical iPhone 16 and an Android ARM64 emulator. Its Osprey modules own the application UI and behavior; SwiftUI and Android hosts render the UI tree and execute platform commands. The [screenshots and run instructions](mobile/README.md) include issue details, local notes, priorities, and SQLite cache recovery.

See the [iOS target](../docs/specs/platforms/0038-iOSTarget.md), [Android target](../docs/specs/platforms/0039-AndroidTarget.md), and [shared application boundary](../docs/specs/applications/0040-ReactiveMobileApplications.md). WebAssembly and mobile C ABI targets reject unsupported capabilities, including resumable effects, during compilation.

## Paired flavor tests (`../tests/flavors/`)

Each exercises a distinct ML-surface feature and runs today:

| Suite | ML feature exercised |
| --- | --- |
| `smoke/` | layout basics, top-level bindings, and interpolation |
| `functions/` | currying, closures, higher-order calls, nesting, and recursion |
| `matching/` | offside-rule matches, literal arms, and wildcard branches |
| `effects/` | handler-owned `mut` state and `:=` mutation |
| `workflows/` | Results, partial application, matching, and pipelines together |
| `operators/` | operator syntax and precedence |
| `text_and_pipelines/` | text operations and pipeline composition |

ML `add x y = x + y` is curried: it lowers to Default `fn add(x) = fn(y) => x + y`. For a flat multi-parameter function, ML uses `add (x, y) = x + y`, matching Default `fn add(x, y) = x + y`. The parentheses group arguments; Osprey has no tuples. Paired tests compare the corresponding forms' ASTs and emitted LLVM IR.

## Must-reject fixtures (`failscompilation/`)

Every file here is an ill-formed program the language defines as a compile error.
The must-reject suite in
[`../crates/osprey-cli/tests/examples_compile.rs`](../crates/osprey-cli/tests/examples_compile.rs)
runs each one through the pipeline and requires rejection. The bar is ZERO
escapes — a fixture the compiler *accepts* names itself in the failure and
breaks CI. Never add one without confirming it is actually rejected.

- **Negatives use `.ospo`.** That extension is not a source extension anywhere in
  the toolchain — no harness, build, or editor path compiles it — so a broken
  program can sit next to working examples without any suite trying to run it.
  `find … -name '*.ospo'` is exactly how both harnesses discover this corpus.
- **An ML negative is `.ospo` plus a leading `// osprey: flavor=ml` marker** —
  never a bare `.ospml`, which no harness discovers. `.ospo` implies no flavor
  (`flavor_from_extension` has no opinion on it), so the marker alone selects the
  ML frontend with no extension/marker conflict, and the marker path and
  `--flavor ml` produce byte-identical diagnostics. The `ml_*.ospo` fixtures pin
  the ML-specific rejection paths: removed `handle ... in/do` forms
  ([`[FLAVOR-ML-HANDLER]`](../docs/specs/language/0024-MLFlavorSyntax.md)), offside-rule
  violations `[FLAVOR-ML-LAYOUT]`, unterminated `(** … *)`
  `[FLAVOR-ML-COMMENTS]`, `->` where a clause needs `=>` `[FLAVOR-ML-MATCH]`, and
  Default-flavor spellings (`{ … }` records, the `?` sigil) that the ML lexer
  refuses outright `[FLAVOR-BOUNDARY]`.
- **Each fixture pairs with `<name>.ospo.expectedoutput`** holding the compiler's
  real stderr, captured verbatim. The Rust test `failscompilation_corpus_matches_its_expected_diagnostics` compares every fixture's diagnostics byte-for-byte. The shell ratchet separately checks for a nonzero exit.
- **A case the compiler cannot reject yet is parked with `.notimplemented`** (for
  example `infinite_handler_recursion.notimplemented`). The extension keeps it
  out of discovery so it neither passes nor inflates the ratchet; rename it back
  to `.ospo` when the validation lands.

## Running the paired flavor smoke tests

```bash
# Default flavor
osprey test tests/flavors/smoke/smoke.test.osp

# ML flavor
osprey test tests/flavors/smoke/smoke.test.ospml
```

The flavor is resolved automatically from the extension; add `--flavor ml` only to
force the ML surface on a file without the `.ospml` extension or marker.

