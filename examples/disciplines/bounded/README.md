# `@osprey/bounded` — the reference interval discipline

`bounded` is the second reference discipline of [spec 0050](../../../docs/specs/language/0050-TypeDisciplines.md#reference-disciplines-types-discipline-reference): a deliberately small checker that reads an integer interval annotation, holds every call site inside the module to it, and erases the module to plain `int`. It is a template, not a product: a real discipline grows from here outside `crates/`.

```osprey-ml
// osprey: types=bounded

namespace demo

module Percent
    // @bound p in 0..100
    export scale (p, total) = intDiv (total * p, 100)

    export half total = scale (50, total)
```

## What it checks

A **bound annotation** is a comment line directly above a function definition:

```text
// @bound <parameter> in <lo>..<hi>
```

Several annotations may precede one definition; each bounds one parameter, and the interval is inclusive. The annotation applies to the next definition in the file — `name (a, b) = …` or `name a = …` at the top level of a module or namespace.

An **argument expression** at a call site is one of:

- an integer literal, including a negative one: `150`, `-5`;
- a parameter of the enclosing function **when that parameter is itself bounded**;
- `a + b`, `a - b` or `a * b` of such expressions;
- any of those in parentheses.

Literals are point intervals, bounded parameters keep their declared interval, and `+`, `-` and `*` use interval arithmetic (`[lo, hi] * [lo, hi]` is the hull of the four corner products). A multiplication or addition whose endpoint overflows `int` is treated as unbounded rather than guessed.

Every call to an annotated function, where the callee parameter carries a bound, is checked:

- the interval is inside the bound: the call is accepted;
- the interval is outside the bound: `bounded/out-of-range` with the computed interval, the declared bound and the line and column of the call;
- no interval can be established: `bounded/unknown` (`argument `x` to `f` has no bound`).

Parameters of the callee that carry no bound are not checked, so `scale (50, total)` above passes even though `total` is unbounded: only `p` is annotated.

## Erasure and the response

The erased module is the source with the annotation lines removed and the marker rewritten to `// osprey: types=hm`. Its export signature is taken from the compiler itself — `options.compiler` runs `--signature` on the erased file — so a checker never carries a second inference. A source that exports no `module` has no export signature: `--signature` prints nothing and the answer carries an empty `signature`, which the seam accepts. An accepted response is `{version, result:"accepted", signature, erased:{flavor, source}, diagnostics:[]}`; a rejected one is `{version, result:"rejected", diagnostics:[{code, severity, line, column, message}]}`. Codes are unprefixed: the compiler prints them as `bounded/<code>`.

Any internal failure — an unreadable cache directory, a compiler that refuses the erased module, a request that is not one JSON object — is a rejection with one `bounded/internal` or `bounded/check` diagnostic, exit status 0 and never an empty standard output.

## The subset this template does not read

The scan is textual and the README is its contract. It does not:

- resolve scope: a call is matched by the bare name only, so a local binding that shadows a bounded function's name is still read as a call to it, and two modules of one file that define the same name share one bound set;
- parse arguments beyond the arithmetic above: a nested call (`f (g 1)`), a record field, a `match`, an `if`-less conditional or a string interpolation inside an argument is `unknown`, so the call is rejected, never silently accepted;
- follow calls across modules, or check a call site in a module that has no annotation;
- know that a definition is nested: annotations attach to the next definition line anywhere in the file, and the enclosing function of a call is the nearest preceding definition at a smaller indentation;
- track unary minus beyond a literal's own sign, or any operator other than `+`, `-` and `*`;
- read an annotation whose interval is inverted (`100..0`) or whose endpoints are not integer literals: it is not an annotation;
- evaluate arithmetic at run time: an argument that is a bounded parameter *plus* a bounded parameter is bounded, but the values it actually takes are not.
- read a **curried** call to a function of more than one parameter: `f a b` is one argument text, so it is refused as `unknown` (pass the flat form, `f (a, b)`); a one-parameter function is read in both forms.

Comments and string literals are not stripped before the scan, so a call spelled inside one is still read as a call.

## Layout

| Path | What it is |
| --- | --- |
| `osprey.toml` | the package, the `[discipline]` table, and this package's own suite project |
| `src/` | the checker project: one file per module, with `src/main.ospml` the entry |
| `tests/main.ospml`, `tests/checks.ospml` | the suite entry and the seam assertions, run by `osprey test examples/disciplines/bounded` |
| `tests/rate.ospml` | a module marked `// osprey: types=bounded` that the suite's own build checks |
| `tests/protocol.ospml` | the protocol cases: in-range, out-of-range, sum, product, unknown, annotation parsing, erasure and the exact JSON answers |
| `tests/signature-stub.sh`, `tests/silent-stub.sh`, `tests/failing-stub.sh` | test doubles for the compiler's `--signature` step: a fixed signature, an empty one, and a compiler that refuses the erased module |

The checker is a project directory because the compiler builds a checker with `<compiler> <checker-directory> --compile`, exactly like any other project: `src/main.ospml` is the entry and each module of the checker lives in its own file under `src/`. Its suite therefore reaches the discipline the same way a user does: `tests/rate.ospml` is checked by the `src/` project during the suite's own build, and `tests/protocol.ospml` drives the checker binary that same build produced, one request per case, holding each answer to a byte-exact golden. It looks for the package beside the working directory the suite was started in, so run the suite from the checkout root or from the package directory. The compiler cannot run a suite inside the checker itself: naming `test` links the TAP reporter, which writes its plan after the response and would break the one-line protocol.

The checker's only use of the compiler is the `--signature` step, and the protocol cases answer it with `tests/signature-stub.sh` so their JSON stays deterministic; `tests/silent-stub.sh` pins the empty answer a module with no exported `module` gets, and `tests/failing-stub.sh` proves a refused erased module becomes one `bounded/check` diagnostic. The real `<compiler> --signature` path is proven end to end by `../bounded_roundtrip` in `_test_disciplines`.

## Running it

```bash
./target/release/osprey test examples/disciplines/bounded
./target/release/osprey examples/disciplines/bounded_roundtrip --run --memory=gc
```

The roundtrip project in `../bounded_roundtrip` is the end-to-end proof: an `hm` entry source links a module the discipline checked, under GC, ARC and wasm32, and an out-of-range variant of the same module is rejected with its golden stderr.
