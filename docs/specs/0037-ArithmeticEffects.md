# Arithmetic Effects

**Status:** normative target; implementation has not started. Delivery is fixed by [plan 0027](../plans/0027-arithmetic-effects.md). Today's shipped behavior — integer arithmetic returning `Result<int, MathError>` — is specified in [Error Handling](0013-ErrorHandling.md#arithmetic-and-result--arith-checked) and remains authoritative until that plan lands. Issue [#230](https://github.com/Nimblesite/osprey/issues/230) records why the shipped model must change: `?:` with a fabricated fallback is the corpus-wide idiom for discharging arithmetic Results, and a fabricated fallback is a silent wrong answer.

The key words `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are to be interpreted as described by BCP 14 (RFC 2119 and RFC 8174) when they appear in capitals. A feature is not implemented merely because this document specifies it.

## The model — [ARITH-EFFECT]

Integer arithmetic returns `int`. Failure is not erased and does not panic: an operation whose mathematical result is unrepresentable performs an operation of the compiler-declared `Arith` effect, and the statically required handler substitutes the value the region's policy chooses. The overflow check compiled today stays exactly where it is; only the cold branch's destination changes, from constructing `Error("integer overflow")` to dispatching the operation.

| Operator | int, int | float, float | int, float / float, int |
| --- | --- | --- | --- |
| `+ - *` | `int`, MAY perform `Arith.overflow` | `float` (IEEE-754, total) | `float` (int promoted, total) |
| `/` | `float`, MAY perform `Arith.divideByZero` | same | same |
| `%` | `int`, MAY perform `Arith.remainderByZero` | `float`, MAY perform `Arith.divideByZero` | `float` (int promoted), MAY perform `Arith.divideByZero` |
| unary `-`, `abs` | `int`, MAY perform `Arith.overflow` | `float` (total) | — |

Unchanged by this design: string/list/map `+` overloads; float `+ - *` and unary float `-` as plain IEEE-754 ([plan 0022](../plans/0022-arithmetic-totality-audit.md) owns the open float questions); the negated-literal fold [ARITH-NEG-LITERAL](0013-ErrorHandling.md#negated-literals--arith-neg-literal); integer `-9223372036854775808 % -1` producing the representable remainder `0` without faulting; the `checkedAdd`/`checkedSub`/`checkedMul` builtins, which remain the explicit `Result`-returning spelling for code that wants a value-level answer.

There is no `Result` in any arithmetic type, so the failure-preserving chain flattening of [ARITH-CHECKED] and its carve-out in [Result Preservation](0004-TypeSystem.md#result-preservation) are deleted with it. `Result` itself is untouched everywhere it is earned — indexing, HTTP, parsing, user functions.

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
unhandled effect operations at program entry: Arith.overflow; add a matching handle
```

### Provably total sites — [ARITH-EFFECT-TOTAL-SITES]

A site whose failure is impossible MUST NOT seed a requirement:

- `/` or `%` whose divisor is a nonzero numeric literal (after the [ARITH-NEG-LITERAL] fold). `x % 2` and `x / 4` are total and need no handler.
- A constant expression, which is folded under [ARITH-EFFECT-CONST] and never reaches runtime.

This also closes the secondary defect of [#230](https://github.com/Nimblesite/osprey/issues/230): today `x % 2 ?: 1` carries a provably dead fallback the checker accepts. Under this design the site is total, the type is `int`, and any `?:` on it is the existing rejection `` `?:` needs a Result on its left, found int ``.

### Constant folding — [ARITH-EFFECT-CONST]

An integer arithmetic expression whose operands are compile-time constants (literals, or folded constants) is evaluated at compile time. A fold that overflows is a compile error naming the expression, matching what C, Rust, and Zig do for constant expressions:

```text
constant arithmetic overflows: 9223372036854775807 + 1
```

Folding is what keeps file-scope bindings coherent: a file-scope initializer runs before program entry ([MODULES-FILE-SCOPE-BINDING](0025-ModulesAndNamespaces.md#file-scope-bindings-modules-file-scope-binding)), before any handler can be installed, so a file-scope initializer MUST NOT seed an `Arith` requirement. Constant initializers fold; a file-scope initializer with a non-constant fallible operation is rejected with an error directing it into a handled region.

## Handlers substitute; they cannot decline — [ARITH-EFFECT-ARMS]

`Arith` arms are ordinary substituting handler arms: the arm's value is the operation's result, typed by the operation signature, and execution continues after the faulting operation. Because every operation returns `int` or `float`, an arm MUST produce a value — halting the program is not expressible in the signature. `resume` in an `Arith` arm is rejected. This preserves both halves of Osprey's arithmetic promise: no panic, and no silent fabrication — the recovery value is chosen by a named, lexically scoped policy instead of a `?:` literal at every call site.

Substituting arms are the direct handler-call path on native and the supported handler form on wasm32 ([Effects on WebAssembly](0017-AlgebraicEffects.md)), so this design runs on every target.

### No re-entry — [ARITH-EFFECT-ARMS-NO-REENTRY]

Checked arithmetic inside any arm of an `Arith` handler is rejected at compile time. The shipped rule rejects only performing the *active operation* from its own arm; for `Arith` that is not enough, because sibling operations could recurse mutually (an `overflow` arm whose `%` faults into `remainderByZero`, whose `+` faults back into `overflow`). The rejection is whole-effect:

```text
handler arm `Arith.overflow` contains checked arithmetic, which performs `Arith` while that handler is active; use wrapAdd/satAdd or the operation's `wrapped` payload
```

Arms compute with comparisons, literals, the operation payloads, and the total helpers below. Handler-owned `mut` state ([EFFECTS-HANDLER-STATE](0017-AlgebraicEffects.md#handler-owned-state)) is written with total-helper results — the sticky-flag policy needs no arithmetic at all.

### Total helpers — [ARITH-EFFECT-TOTAL-HELPERS]

The compiler provides total integer builtins that never fault and seed nothing: `wrapAdd`, `wrapSub`, `wrapMul` (two's-complement) and `satAdd`, `satSub`, `satMul` (clamping to the `int` range). They exist for `Arith` arms and for code — hashes, checksums, PRNGs — where wraparound is the definition rather than a fault, and they are the sanctioned way to want it without installing a wrapping region.

## Policies

The point of the design is that one region states one policy once, instead of 6,408 `?:` sites each fabricating a value. All four shapes below use only shipped handler machinery.

Wrapping — modular arithmetic by declared intent, C's `-fwrapv` scoped to a region:

```osprey
fn djb2(bytes) = bytes |> fold(5381, fn(h, b) => h * 33 + b)

let digest = handle Arith
    overflow _ _ _ wrapped => wrapped
do djb2(payload)
```

Fault-sticky — IEEE-754's sticky-flag discipline for integers; the value flows, the boundary decides:

```osprey
mut faulted = false
let total = handle Arith
    overflow _ l _ _ => { faulted = true  l }
    remainderByZero l => { faulted = true  l }
do settle(ledger)

print("${faulted ? "REJECTED: ledger overflow" : "settled ${total} cents"}")
```

Saturating — a cap instead of a fault:

```osprey
let delay = handle Arith
    overflow _ _ _ _ => capMs
do backoff(64)
```

Nested and partial — the inner region wraps checksums; everything else faults to the outer policy, per the shipped innermost-arm-wins and partial-handler rules:

```osprey
handle Arith
    overflow _ l _ _ => { faulted = true  l }
do {
    let checksum = handle Arith overflow _ _ _ wrapped => wrapped do djb2(payload)
    let total = settle(postings)
    print("checksum ${checksum}, total ${total}")
}
```

## The Default handle binder — [EFFECTS-HANDLE-DO]

In the Default flavor the handle body binder is `do`: `handle E arms... do body`. ML keeps `in`, which it inherits from `let ... in` and layout tradition. In every mainstream brace language `in` means iteration or membership (`for x in xs`, `foreach (x in xs)`, JavaScript's `in` operator), so `handle ... in run()` misreads as iterating `run()`; `do` reads as "execute this block" in C-family languages and is Haskell's block keyword, and it is already reserved in the ML lexer. Renaming also frees `in` for any future Default iteration surface. Divergent surface spellings over one AST are exactly what [FLAVOR-BOUNDARY](0023-LanguageFlavors.md) permits — Default and ML already differ on `let`, `:=`, and ternaries.

```ebnf
handlerExprDefault ::= "handle" IDENT handlerArm+ "do" expression
handlerExprML      ::= "handle" IDENT handlerArm+ "in" expression
```

The rename is a clean break: `in` in Default handle position is a parse error after it lands, and it lands before the arithmetic change so every migrated handler is written with `do` from birth. `in`/`out` variance markers on type parameters are unaffected.

## What this deletes

- The 6,408 `?:` fabrication sites become compile errors — `` `?:` needs a Result on its left, found int `` — so the compiler enumerates the entire migration. `?:` itself is untouched where its scrutinee is a genuine `Result`.
- `Result<_, MathError>` disappears from arithmetic types, inferred signatures, and diagnostics. Whether any producer of `MathError` remains (and whether the type retires) is an audit item in plan 0027.
- The arithmetic chain-flattening rule and its Result Preservation exception ([0004](0004-TypeSystem.md#result-preservation), [0013](0013-ErrorHandling.md#chaining-arithmetic)) are deleted; there is no wrapper to flatten.
- The `ArithmeticFailure` policy pattern in [0013](0013-ErrorHandling.md#choosing-and-preserving-a-policy) is subsumed: `Arith` is that pattern, built in, statically required.

## What this preserves

- **No panics.** The runtime division guard still branches before the divide; overflow still branches on the intrinsic's bit; an arm cannot decline to produce a value. Arithmetic cannot terminate the program even deliberately.
- **No silent wrap.** Wrapping exists only where a region names it (`wrapped`, `wrapAdd`) — visible in source, scoped, greppable.
- **Hot-path codegen.** The checked intrinsics and branch layout are unchanged; the fault path is the cold branch it already is.
- **Fibers.** A fault inside a fiber reaches its handler through the shipped serialized perform round trip ([EFFECTS-FIBER-PERFORM](0017-AlgebraicEffects.md)) — cold path only.

## Status

Nothing in this document is implemented. The shipped arithmetic model is [ARITH-CHECKED](0013-ErrorHandling.md#arithmetic-and-result--arith-checked). Sequencing, edit sites, risks, and the corpus conversion checklist are in [plan 0027](../plans/0027-arithmetic-effects.md). Prelude-named policies (`handle Arith.saturating do ...`) require handler values ([plan 0016](../plans/0016-algebraic-effects-and-handlers.md) Phase B) and are out of scope for the initial landing; compile-time-selected policies via `handle static Arith` ([Staged Effects](0035-StagedEffects.md)) are a recorded synergy, not a commitment.
