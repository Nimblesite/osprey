# Arithmetic Effects

**Status:** shipped in [PR #241](https://github.com/Nimblesite/osprey/pull/241). Implementation and verification evidence is recorded below.

The key words `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are to be interpreted as described by BCP 14 (RFC 2119 and RFC 8174) when they appear in capitals. A feature is not implemented merely because this document specifies it.

## The guarantee — [ARITH-TOTAL]

**In an accepted Osprey program, arithmetic cannot fail silently or trap.** A total operation produces a defined value of its static type. A fallible operation transfers to an explicitly installed `Arith` handler; normal completion of its value arm supplies the typed result. An arm may diverge or request another explicit effect, as allowed by [the effects contract](0017-AlgebraicEffects.md). A conforming compiler MUST reject any program for which it cannot prove every clause below.

- **No trap, panic, or abort.** No arithmetic operation may raise a hardware fault or terminate the program. The zero-divisor and minimum-value guards branch *before* any faulting instruction, overflow is detected by non-trapping intrinsics, and `-9223372036854775808 % -1` produces `0` without executing the faulting `srem` path.
- **No silent wraparound.** A two's-complement result reaches the program only where the program names it: the `wrapped` payload of `Arith.overflow` inside a handler some region installed, or the total helpers `wrapAdd`/`wrapSub`/`wrapMul` ([ARITH-EFFECT-TOTAL-HELPERS](#total-helpers--arith-effect-total-helpers)).
- **No unspecified value.** No arithmetic result is undefined behavior, poison, or target-dependent.
- **No unhandled fault.** Every arithmetic site is, statically, exactly one of three things: proven total ([ARITH-EFFECT-TOTAL-SITES](#provably-total-sites--arith-effect-total-sites), [ARITH-EFFECT-CONST](#constant-folding--arith-effect-const), float IEEE-754 closure); discharged by an `Arith` handler on every execution path, through helpers, lambdas, and fibers ([ARITH-EFFECT-DISCHARGE](#static-discharge--arith-effect-discharge)); or the program is rejected at compile time. There is no fourth case.
- **Value-mode recovery.** `Arith` uses value operations: normal arm completion supplies the operation result, and `resume` is rejected. Explicit effects performed by a policy remain visible obligations under [the effects contract](0017-AlgebraicEffects.md); value mode alone does not prove termination or forbid an explicitly requested outer control effect.
- **No implicit self-reentry.** An arm executes outside its own installation. Fallible arithmetic in that arm requires a distinct enclosing `Arith` policy ([ARITH-EFFECT-ARMS-NO-REENTRY](#forwarding-recovery--arith-effect-arms-no-reentry)); without one, the program is rejected.
- **No fabricated fallback.** A plain `int`/`float` is never a `?:` scrutinee (`` `?:` needs a Result on its left, found int ``), and there is no ambient or implicit default policy: a recovery value exists only inside a handler a region installed by name.

Floating-point `+`, `-`, `*`, and unary `-` satisfy the same totality through IEEE-754 closure — `inf` and `NaN` are defined values of `float`, not failures. They MUST NOT additionally request `Arith` for a non-finite result. The complete float contract is [FLOAT-IEEE-RESULTS] below.

The numeric builtins are inside the guarantee: `abs` preserves the input numeric type and `intDiv` returns `int`, with `abs(-9223372036854775808)` and `intDiv(-9223372036854775808, -1)` performing `Arith.overflow` and `intDiv(_, 0)` performing `Arith.remainderByZero`. `checkedAdd`/`checkedSub`/`checkedMul` remain the explicit value-level spelling; an `Error` they return is ordinary data, produced totally.

Conformance requires a rejection fixture or differential runtime test for every clause above, exercised on native under all three memory backends and on wasm32. The verification matrix below names those tests.

## The model — [ARITH-EFFECT]

Integer arithmetic returns `int`. Failure is neither erased nor raised: an operation whose mathematical result is unrepresentable performs an operation of the compiler-declared `Arith` effect, and the statically required handler substitutes the value the region's policy chooses. The overflow test is a non-trapping intrinsic and its fault branch is cold, so a total site costs one predictable branch.

| Operator | int, int | float, float | int, float / float, int |
| --- | --- | --- | --- |
| `+ - *` | `int`, MAY perform `Arith.overflow` | `float` (IEEE-754, total) | `float` (int promoted, total) |
| `/` | `float`, MAY perform `Arith.divideByZero` | same | same |
| `%` | `int`, MAY perform `Arith.remainderByZero` | `float`, MAY perform `Arith.divideByZero` | `float` (int promoted), MAY perform `Arith.divideByZero` |
| unary `-`, `abs` | `int`, MAY perform `Arith.overflow` | `float` (total) | — |

Outside the `Arith` channel: string, list and map `+` overloads; float `+ - *` and unary float `-` as plain IEEE-754 ([FLOAT-IEEE-RESULTS]); the negated-literal fold [ARITH-NEG-LITERAL](0013-ErrorHandling.md#negated-literals--arith-neg-literal); and `checkedAdd`/`checkedSub`/`checkedMul`, which return `Result<int, Error>` for code that wants overflow as data.

No arithmetic type contains a `Result`, so [Result Preservation](0004-TypeSystem.md#result-preservation) governs arithmetic vacuously and has no arithmetic exception. `Result` is reserved for failures a value genuinely carries — indexing, HTTP, parsing, user functions.

## Floating-point results — [FLOAT-IEEE-RESULTS]

`float` is IEEE-754 binary64. Finite values, positive and negative infinity, NaN, and both signed zeros are ordinary values of this type. Arithmetic MUST retain IEEE behavior without fast-math assumptions that discard non-finite values, signed zero or subnormal results. Producing infinity or NaN MUST NOT request `Arith.overflow`, return a `Result`, trap or implicitly substitute a finite value. This also applies to float expressions in file-scope initializers; source literals themselves MUST be finite ([FLOAT-LITERAL-RANGE](0002-LexicalStructure.md#finite-float-literals--float-literal-range)).

| Operands or result | Required behavior |
| --- | --- |
| Finite `+`, `-`, `*`, or division by a nonzero value exceeds the finite range | Signed infinity according to IEEE arithmetic |
| Infinity supplied to `+`, `-`, `*`, `/` | IEEE result; for example, `inf + 1` is `inf`, `inf + -inf` and `inf / inf` are NaN, and `inf * 0` is NaN |
| NaN supplied to either operand of `+`, `-`, `*`, or nonzero-divisor `/` and `%` | NaN; a NaN divisor is not zero and MUST NOT request zero-divisor recovery |
| Unary `-` | Reverse the sign, including infinities and signed zero; NaN remains NaN |
| Float `abs` | Clear the sign, including negative zero and negative infinity; NaN remains NaN. No arithmetic effect, including through aliases, callbacks and returned or stored function values |
| Underflow | Gradual underflow into subnormal values, then signed zero when rounding requires it; no arithmetic effect |
| Signed zeros | Compare equal; operations retain their IEEE signs, for example `-0.0 * 2.0`, `-0.0 / 2.0`, and `-0.0 % 2.0` produce negative zero |
| `/` with divisor `0.0` or `-0.0` | Request `Arith.divideByZero("/", lhs)` before division, including when `lhs` is zero, infinity or NaN |
| Float `%` with divisor `0.0` or `-0.0` | Request `Arith.divideByZero("%", lhs)` before remainder, including when `lhs` is zero, infinity or NaN |
| Float `%` with a nonzero divisor | Remainder for a quotient truncated toward zero, with the dividend's sign; `-5.5 % 2.0` is `-1.5`. Infinite dividend or NaN operand gives NaN; finite dividend with infinite divisor returns the dividend |

Integer operands are promoted to binary64 when paired with a float, and both operands of `/` are promoted. `toFloat` is the explicit integer conversion; rounding may lose precision outside the exactly representable integer range. Neither promotion nor `toFloat` overflows for an Osprey `int`. Float-to-int coercion is rejected; internal representation conversion uses saturating narrowing with defined NaN and boundary behavior ([FLOAT-CONVERT](0004-TypeSystem.md#internal-numeric-narrowing--float-convert)). The six comparison operators follow [FLOAT-COMPARE](0004-TypeSystem.md#floating-point-comparison--float-compare); in particular, NaN compares unequal to every value including itself. NaN payload bits and NaN's sign are not specified.

Host GPU kernels MUST obey this same scalar contract, including the enclosing `Arith` policy for zero divisors. List/buffer transfers preserve non-finite values and signed zero. This specifies the shipped host backend; it does not claim a device backend is implemented. Programs needing finite-only results must explicitly test their results at the application boundary; arithmetic makes no implicit finite-only guarantee.

## The `Arith` effect — [ARITH-EFFECT-OPS]

`Arith` is declared by the compiler and is in scope in every program without any import. User code MUST NOT redeclare it.

```osprey
effect Arith {
    overflow:        fn(string, int, int, int) -> int
    divideByZero:    fn(string, float) -> float
    remainderByZero: fn(int) -> int
}
```

- `overflow(op, lhs, rhs, wrapped)` — `op` is the operator spelling (`"+"`, `"-"`, `"*"`, `"neg"`, `"abs"`); `lhs`/`rhs` are the operands (`rhs` is `0` for the unary forms); `wrapped` is the two's-complement result. The overflow intrinsics already produce the wrapped value in the same register pair as the overflow bit, so passing it costs nothing, and it is what makes a wrapping policy expressible with **no arithmetic in the arm**.
- `divideByZero(op, lhs)` — a zero divisor at a float-result site: `/` with any operands, or `%` with a float operand. `op` is `"/"` or `"%"`.
- `remainderByZero(lhs)` — a zero divisor at an integer `%` site.

## Static discharge — [ARITH-EFFECT-DISCHARGE]

An arithmetic operation that MAY perform an `Arith` operation seeds that requirement exactly as a syntactic `perform` does. Everything in [EFFECTS-STATIC-DISCHARGE](0017-AlgebraicEffects.md#effectful-function-types) then applies unchanged: requirements propagate through named calls, lambdas passed to higher-order functions, and fibers; discharge is operation-specific, so a handler covering only `overflow` leaves `remainderByZero` for an enclosing handler; the program entry MUST have no remaining requirements. A program that computes and never installs a policy is rejected at compile time with the existing diagnostic shape:

```text
unhandled effect operations at program entry: Arith.overflow; add a matching `handle`
```

Host GPU combinators (`gpuMap`, `gpuFilter`, `gpuFold`, `gpuScan` and `gpuZipWith`) invoke their callbacks and therefore propagate the callbacks' arithmetic requirements. This applies to inline lambdas, named helpers and builtin function values. Kernel stage-legality checks do not discharge `Arith`; an integer `abs` or `intDiv` kernel still requires a matching surrounding policy. A recovery arm's own fallible arithmetic still requires an outer handler.

### Provably total sites — [ARITH-EFFECT-TOTAL-SITES]

A site whose failure is impossible MUST NOT seed a requirement:

- `/` or `%` whose divisor is a nonzero numeric literal (after the [ARITH-NEG-LITERAL] fold). `x % 2` and `x / 4` are total and need no handler.
- A constant expression, which is folded under [ARITH-EFFECT-CONST] and never reaches runtime.

A total site's type is `int` or `float`, so `?:` on it is rejected: `` `?:` needs a Result on its left, found int ``.

### Constant folding — [ARITH-EFFECT-CONST]

An integer arithmetic expression whose operands are compile-time constants (literals, or folded constants) is evaluated at compile time. A fold that overflows is a compile error naming the expression, matching what C, Rust, and Zig do for constant expressions:

```text
constant arithmetic overflows: 9223372036854775807 + 1
```

Folding is what keeps file-scope bindings coherent: a file-scope initializer runs before program entry ([MODULES-FILE-SCOPE-BINDING](0025-ModulesAndNamespaces.md#file-scope-bindings-modules-file-scope-binding)), before any handler can be installed, so a file-scope initializer MUST NOT seed an `Arith` requirement. Constant initializers fold; a file-scope initializer with a non-constant fallible operation is rejected with an error directing it into a handled region.

## Handlers substitute; they cannot decline — [ARITH-EFFECT-ARMS]

`Arith` operations use the value mode defined in [Algebraic Effects](0017-AlgebraicEffects.md).
Normal arm completion supplies the declared numeric result; `resume` is invalid.
A policy's own effects remain requirements, including an explicit request to an
outer control handler. The arithmetic operation itself never implicitly traps,
aborts or fabricates a recovery value. Target availability follows the canonical
capability rules.

### Forwarding recovery — [ARITH-EFFECT-ARMS-NO-REENTRY]

An arm executes outside its own installation. Fallible arithmetic inside a
recovery arm therefore requires an outer `Arith` policy; it cannot recursively
select that same active arm. With no outer policy it is rejected as an unhandled
operation. Prefer the total helpers or the operation's `wrapped` payload when
recovery should have no further arithmetic requirements. This follows the
ordinary handler scoping rule; arithmetic has no separate re-entry mechanism.

### Total helpers — [ARITH-EFFECT-TOTAL-HELPERS]

The compiler provides total integer builtins that never fault and seed nothing: `wrapAdd`, `wrapSub`, `wrapMul` (two's-complement) and `satAdd`, `satSub`, `satMul` (clamping to the `int` range). They exist for `Arith` arms and for code — hashes, checksums, PRNGs — where wraparound is the definition rather than a fault, and they are the sanctioned way to want it without installing a wrapping region.

## Policies

A region states its policy once, and every arithmetic fault inside it — through helpers, lambdas and fibers — is answered by that policy.

Wrapping — modular arithmetic by declared intent, C's `-fwrapv` scoped to a region:

```osprey
fn djb2(bytes) = bytes |> fold(5381, fn(h, b) => h * 33 + b)

let wrapping = handler Arith { overflow _ _ _ wrapped => wrapped }
let digest = wrapping(|| => djb2(payload))
```

Fault-sticky — IEEE-754's sticky-flag discipline for integers; the value flows, the boundary decides:

```osprey
mut faulted = false
let recording = handler Arith {
    overflow _ l _ _ => {
        faulted = true
        l
    }
    remainderByZero l => {
        faulted = true
        l
    }
}
let total = recording(|| => settle(ledger))

print(faulted ? "REJECTED: ledger overflow" : "settled ${total} cents")
```

Saturating — a cap instead of a fault:

```osprey
let capped = handler Arith { overflow _ _ _ _ => capMs }
let delay = capped(|| => backoff(64))
```

Nested and partial — the inner region wraps checksums; everything else faults to the outer policy, by the innermost-arm-wins and partial-handler rules of [Algebraic Effects](0017-AlgebraicEffects.md#handlers):

```osprey
fn report() = {
    handle Arith {
        overflow _ l _ _ => {
            faulted = true
            l
        }
    }
    let wrapping = handler Arith { overflow _ _ _ wrapped => wrapped }
    let checksum = wrapping(|| => djb2(payload))
    let total = settle(postings)
    print("checksum ${checksum}, total ${total}")
}
report()
```

## Handler application

[EFFECTS-HANDLE-REST](0017-AlgebraicEffects.md#handling-the-rest-of-a-block-effects-handle-rest)
owns Default/ML body syntax, callable handlers and bodyless installation.
Arithmetic policies use those forms without a separate handler grammar.

## Scope

Named policies are ordinary callable handlers, such as `saturating(work)`.
`Arith` has runtime policies only: `handle static Arith` is rejected, and a
host-backend GPU kernel dispatches to the enclosing runtime policy. A checked
static interpretation for device regions under
[STAGE-HANDLE-STATIC](0017-AlgebraicEffects.md#static-handlers--stage-handle-static)
belongs to the staging delivery.
Shared handler and staging delivery belongs to [plan 0016](../plans/0016-algebraic-effects-and-handlers.md).

## Implementation and verification

Integer arithmetic produces plain numeric values. `MathError` and arithmetic Result flattening have been removed; `checkedAdd`, `checkedSub` and `checkedMul` preserve their explicit `Result<int, Error>` interface. `osprey-types::arithmetic` classifies requirements and constant folds, `effect_rows` propagates and discharges them, and `osprey-codegen::arithmetic` sends faults through the ordinary value-handler path.

| Contract | Regression evidence |
| --- | --- |
| Correct square, fold accumulator and ledger results after overflow (#230); total helpers; outer-policy forwarding | `tests/core/arithmetic/effect_policies.test.osp` and its ML twin |
| Return annotations preserve arithmetic values (#163) | `arithmetic_return_annotations_preserve_values_in_both_flavors` in `crates/osprey-cli/tests/redundant_annotations.rs`, executed under default, GC and ARC |
| Direct handlers preserve complete integer/float `Result` operation payloads (#183) | `direct handler calls preserve Result operation values` in `effect_policies` |
| Ordered overflow payloads, unary boundaries, integer remainder and float zero divisors | `result_chain_unary_stress` and `boundary_error_stress`, both flavors; each case checks the numeric answer and complete ordered fault trace |
| No implicit policy, reserved `Arith`, value-mode arms, constant-overflow errors, file-scope restrictions and recovery requiring an outer policy | `arith_unhandled`, `arith_redeclared`, `arith_resume`, `arith_constant_overflow`, `arith_file_initializer`, `arith_recovery_needs_outer` in `examples/failscompilation/`, each with an `ml_` fixture and exact diagnostic golden |
| IEEE closure, signed zero, subnormals, nonzero remainder and ordered zero-divisor recovery | `boolean_consolidated.test.osp` and its ML twin; ten cases with a shared golden, including float absolute values and integer recovery through function values |
| Host GPU IEEE values, scalar transfers, numeric `abs` callbacks and ordered fault recovery | `scalar_contracts.test.osp` and its ML twin, under both inline and extracted kernel lowering |
| GPU callback requirements, builtin extraction, lexical shadowing and ordered recovery | `effect_rows_tests::gpu`, `builtin_kernels_are_extracted_with_specialized_scalar_abis` and the existing `kernel_frontier` twins under both kernel modes |
| Numeric operands through generic aliases and higher-order calls; finite source literals | `float_operand_constraint` and `float_literal_overflow` rejection fixtures in both flavors, plus type and frontend unit tests |
| Defined internal float narrowing for NaNs, infinities, fractions and signed range boundaries | `float_coercions_use_defined_saturation_for_extreme_inputs` in `osprey-codegen::conv`; saturating LLVM intrinsic over twelve boundary operands |
| Native float remainder links its platform math runtime | `native_float_remainder_links_its_platform_math_runtime` in the CLI driver tests, plus the float corpus on native and WASM |
| Retired `MathError` cannot be named as a builtin | `explicit_arguments_validate_nested_types_and_enclosing_binders` in `crates/osprey-types/src/methods.rs` |

`make ci` runs the Rust suites, coverage gates, exact rejection diagnostics, native default/GC/ARC goldens, editor tests and application acceptance tests. The ARC corpus also requires zero live objects at exit. `make wasm` runs the same target-supported corpus against the same goldens through WASI; unsupported capabilities are enumerated by the target manifest. ML twins share the Default golden; the standalone ML currying suite has its own golden because it has no Default twin. The arithmetic policy and stress suites participate in each supported target run.

The policy examples above can be compiled with concrete iterator inputs and definitions for their application-specific names (`payload`, `ledger`, `postings`, `settle`, `backoff`, `capMs`). The `Arith` declaration describes the compiler builtin; redeclaring it in source is deliberately rejected.

Benchmark measurements and their machine/provenance limits live in [the benchmark report](../../website/src/benchmarks.md). `make bench-osprey` refreshes Osprey's four backend columns while preserving the other languages' recorded measurements.
