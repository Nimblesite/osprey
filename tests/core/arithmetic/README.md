# Arithmetic safety tests

Every executable suite in this directory has a Default (`.osp`) and ML
(`.ospml`) twin. Test names and semantics are paired so the cross-flavor LLVM
gate can prove both surfaces lower identically.

| Suite | Named tests per flavor | Coverage |
| --- | ---: | --- |
| `calculator` | 3 | Calls, pipelines, lambdas, matching and interpolation |
| `deep_integer_expressions` | 100 | 17–46 operand chains, depth labels through 45, and up to 90 nested arithmetic nodes |
| `precedence_associativity_stress` | 90 | Parenthesized/unparenthesized precedence and associativity |
| `boundary_error_stress` | 102 | Signed limits, overflow, zero divisors, `intDiv`, `%`, and checked aliases |
| `result_chain_unary_stress` | 110 | Unary/`abs`, flattened chains, first-error propagation, `?:`, and explicit matches |
| `effect_policies` | 6 | Error-preserving conversion and algebraic-effect recovery policies |

That is 411 independently registered tests in each flavor, 822 registrations
across the paired arithmetic surface. Stress cases are deliberately separate
`test` declarations so one compiler failure cannot hide the remaining cases
inside a helper loop or aggregate assertion.

The normative invariant is `[ARITH-CHECKED]`: integer `+`, `-`, `*`, unary `-`
and `abs` return `Result<int, MathError>`; `/` and `%` also preserve their
failure channel. Overflow and invalid divisors must never wrap, panic, or be
implicitly unwrapped. A caller must keep the `Result`, exhaustively `match` it,
use an explicit `?:` fallback, or route the error through a statically
discharged algebraic effect.

Run the directory with:

```sh
osprey test tests/core/arithmetic
```

Do not delete, ignore, collapse, or weaken a failing case. A red arithmetic
test is evidence of a language-safety or compiler-depth defect until the
implementation is corrected.
