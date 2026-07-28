# Osprey Examples

**One core. Two surfaces. Zero compromise.**

Osprey is one language — one Hindley-Milner type checker, one effect system, one
runtime, one standard library, one LLVM/wasm backend — fronted by two first-class,
permanent syntaxes called **flavors**:

- **Default flavor (`.osp`)** — C-style braces, `fn`, `f(x: a, y: b)` calls with
  named arguments. Block-structured and explicit. **Fully implemented today**
  (specs 0001–0022).
- **ML flavor (`.ospml`)** — offside-rule layout (indentation, no braces),
  curry-by-default, whitespace application `f a b`, `\x => e` lambdas, `:=`
  mutation, `->` for types and `=>` for clauses. Terse and expression-first.
  **In active development**, with runnable proof in the paired
  [`tests/flavors/`](../tests/flavors) assertion corpus.

Neither flavor is the watered-down one. The Default surface is what a **systems
programmer** reaches for — real braces, explicit calls, nothing optional. The ML
surface is what an **FP devotee** reaches for — real layout, real currying, no
braces in sight. Each goes all the way in its own direction: pick your flavor and
go all in. The language belongs to your tribe.

Both surfaces lower to the same canonical AST before any type checking. After
lowering, nothing — type checker, effect checker, optimiser, codegen — can tell
which flavor you wrote. Same safety, same effects, same performance.

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
import each other normally (per-file selection ships today; multi-file cross-flavor
imports are the design direction).

The differential harness ([`../crates/run_test_corpus.sh`](../crates/run_test_corpus.sh))
discovers programs additively across both flavors. A `.osp`/`.ospml` twin that
produces identical output shares a single `.expectedoutput` file — one golden
proving both flavors print the same bytes.

## Directory layout

- **`tested/`** — working examples that compile and run; output is checked
  byte-for-byte against `.expectedoutput`. Subfolders: `basics/`, `db/`,
  `effects/`, `fiber/`, and `http/`.
- **`../tests/flavors/`** — paired Default/ML executable documentation with
  internal state assertions and edge cases.
- **`failscompilation/`** — programs the compiler must reject, each paired with the
  expected diagnostic.
- **`api/`, `db_postgres/`, `statefulhttp/`, `websocketserver/`, `tui/`, `wasm/`** —
  larger application/runtime examples.
- **`bugs/`** — regression reproductions.

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
  the ML-specific rejection paths: reserved `handler`/`do`
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

## ML status (honest)

- **H1.** Default is fully implemented; ML is in active development with the paired
  assertion suites above as executable proof.
- **H2.** ML effects and handlers are covered by paired suites under
  `tests/flavors/effects/` and `tests/effects/resume/`.
