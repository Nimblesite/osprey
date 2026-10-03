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

Floating-point `+`, `-`, `*`, and unary `-` satisfy the same totality through IEEE-754 closure — `inf` and `NaN` are defined values of `float`, not failures. Whether they should *additionally* surface through `Arith` is [plan 0022](../plans/0022-arithmetic-totality-audit.md)'s open float decision, out of scope here.

The numeric builtins are inside the guarantee: `abs` and `intDiv` follow the operators — plain `int` results, with `abs(-9223372036854775808)` and `intDiv(-9223372036854775808, -1)` performing `Arith.overflow` and `intDiv(_, 0)` performing `Arith.remainderByZero`. `checkedAdd`/`checkedSub`/`checkedMul` remain the explicit value-level spelling; an `Error` they return is ordinary data, produced totally.

Conformance requires a rejection fixture or differential runtime test for every clause above, exercised on native under all three memory backends and on wasm32. The verification matrix below names those tests.

## The model — [ARITH-EFFECT]

Integer arithmetic returns `int`. Failure is neither erased nor raised: an operation whose mathematical result is unrepresentable performs an operation of the compiler-declared `Arith` effect, and the statically required handler substitutes the value the region's policy chooses. The overflow test is a non-trapping intrinsic and its fault branch is cold, so a total site costs one predictable branch.

| Operator | int, int | float, float | int, float / float, int |
| --- | --- | --- | --- |
| `+ - *` | `int`, MAY perform `Arith.overflow` | `float` (IEEE-754, total) | `float` (int promoted, total) |
| `/` | `float`, MAY perform `Arith.divideByZero` | same | same |
| `%` | `int`, MAY perform `Arith.remainderByZero` | `float`, MAY perform `Arith.divideByZero` | `float` (int promoted), MAY perform `Arith.divideByZero` |
| unary `-`, `abs` | `int`, MAY perform `Arith.overflow` | `float` (total) | — |

Outside the `Arith` channel: string, list and map `+` overloads; float `+ - *` and unary float `-` as plain IEEE-754 ([plan 0022](../plans/0022-arithmetic-totality-audit.md) owns the open float questions); the negated-literal fold [ARITH-NEG-LITERAL](0013-ErrorHandling.md#negated-literals--arith-neg-literal); and `checkedAdd`/`checkedSub`/`checkedMul`, which return `Result<int, Error>` for code that wants overflow as data.

No arithmetic type contains a `Result`, so [Result Preservation](0004-TypeSystem.md#result-preservation) governs arithmetic vacuously and has no arithmetic exception. `Result` is reserved for failures a value genuinely carries — indexing, HTTP, parsing, user functions.

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
| Retired `MathError` cannot be named as a builtin | `explicit_arguments_validate_nested_types_and_enclosing_binders` in `crates/osprey-types/src/methods.rs` |

`make ci` runs the Rust suites, coverage gates, exact rejection diagnostics, native default/GC/ARC goldens, editor tests and application acceptance tests. The ARC corpus also requires zero live objects at exit. `make wasm` runs the same target-supported corpus against the same goldens through WASI; unsupported capabilities are enumerated by the target manifest. ML twins share the Default golden; the standalone ML currying suite has its own golden because it has no Default twin. The arithmetic policy and stress suites participate in each supported target run.

The policy examples above can be compiled with concrete iterator inputs and definitions for their application-specific names (`payload`, `ledger`, `postings`, `settle`, `backoff`, `capMs`). The `Arith` declaration describes the compiler builtin; redeclaring it in source is deliberately rejected.

Benchmark measurements and their machine/provenance limits live in [the benchmark report](../../website/src/benchmarks.md). `make bench-osprey` refreshes Osprey's four backend columns while preserving the other languages' recorded measurements.
