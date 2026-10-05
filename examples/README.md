# Osprey Examples

Start with algebraic effects: declare an operation, request it with `perform`, and run the same work under different callable handlers.

| Example | What to try |
| --- | --- |
| [Handlers](handlers/README.md) | Reusable policies, return transformations, static selection, and comparisons with Koka and OCaml |
| [Talon Bank](projects/modules/README.md) | Shared web/iOS/Android application, SQLite and mock storage, auditing, metrics, and platform rendering |
| [Issue Inbox](mobile/README.md) | Shared iOS/Android state and UI, with SQL/HTTP requests interpreted by a platform-command handler |
| [iPhone counter](ios/README.md) | A native logging callback behind a value effect |

`handler E { ... }` creates a callable implementation; `h(work)` installs it while the callback runs. `handle E { ... }` handles the rest of its block. Both forms work in Default (`.osp`) and ML (`.ospml`) syntax. See the [effects contract](../docs/specs/0017-AlgebraicEffects.md) and [current limits](../docs/plans/0016-algebraic-effects-and-handlers.md).

## Flavor convention

| Selector | Effect |
| --- | --- |
| `.ospml` extension | ML flavor |
| `// osprey: flavor=ml` leading marker | ML flavor |
| `--flavor ml` CLI flag | ML flavor |
| `.osp` extension (default) | Default flavor |

Precedence: **flag > marker > extension > Default**. One flavor per file; a project
folder may mix flavors across files. Because every file lowers to the same AST, a
`.osp` module and a `.ospml` module in the same folder compile into one program and
import each other. The [mobile application](mobile/README.md) uses this composition for its C ABI entry file and shared ML application modules.

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
- **[`ios/`](ios/)** — a SwiftUI iPhone app calling Osprey logic through a generated C header, with device/simulator builds and executable smoke checks.
- **[`mobile/`](mobile/README.md)** — the same reactive issue inbox on iOS and Android, with Osprey modules defining the native screen tree, state, SQLite persistence, GitHub requests, and triage workflows.

## Native mobile application

[Issue Inbox](mobile/README.md) has been exercised on a physical iPhone 16 and an Android ARM64 emulator. Its Osprey modules own the application UI and behavior; SwiftUI and Android hosts render the UI tree and execute platform commands. The [screenshots and run instructions](mobile/README.md) include issue details, local notes, priorities, and SQLite cache recovery.

See the [iOS target](../docs/specs/0038-iOSTarget.md), [Android target](../docs/specs/0039-AndroidTarget.md), and [shared application boundary](../docs/specs/0040-ReactiveMobileApplications.md). WebAssembly and mobile C ABI targets reject unsupported capabilities, including resumable effects, during compilation.

## Paired flavor tests (`../tests/flavors/`)

Each exercises a distinct ML-surface feature and runs today:

| Suite | ML feature exercised |
| --- | --- |
| `smoke/` | layout basics, top-level bindings, and interpolation |
| `functions/` | currying, closures, higher-order calls, nesting, and recursion |
| `matching/` | offside-rule matches, literal arms, and wildcard branches |
| `effects/` | handler-owned `mut` state and `:=` mutation |
| `workflows/` | Results, partial application, matching, and pipelines together |

Currying is the one honest difference between the flavors. ML `add x y = x + y`
lowers to the Default **explicit-curry** form `fn add(x) = fn(y) => x + y` — the
same canonical AST, machine-checked by the `.osp`/`.ospml` twins. It is *not* the
same value as a Default multi-parameter `fn add(x, y)`, which is deliberately a
distinct (uncurried) function. To twin that flat form, ML writes the **uncurried**
form `add (x, y) = x + y` — parentheses around a comma-list (argument grouping,
not a tuple; Osprey has none) — which lowers to the same single flat
multi-parameter function. So each twin matches its original form-for-form:
whitespace `f a b` ↔ Default explicit-curry, parens `f (a, b)` ↔ Default
multi-parameter — both emitting byte-identical IR.

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
  ([`[FLAVOR-ML-HANDLER]`](../docs/specs/0024-MLFlavorSyntax.md)), offside-rule
  violations `[FLAVOR-ML-LAYOUT]`, unterminated `(** … *)`
  `[FLAVOR-ML-COMMENTS]`, `->` where a clause needs `=>` `[FLAVOR-ML-MATCH]`, and
  Default-flavor spellings (`{ … }` records, the `?` sigil) that the ML lexer
  refuses outright `[FLAVOR-BOUNDARY]`.
- **Each fixture pairs with `<name>.ospo.expectedoutput`** holding the compiler's
  real stderr, captured verbatim. The file documents the intended diagnostic.
  The shell ratchet checks only for a nonzero exit, so diagnostic-focused work
  must compare stderr with this file explicitly.
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

