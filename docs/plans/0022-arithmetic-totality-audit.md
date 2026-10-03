# Arithmetic Totality Audit — where the checked-arithmetic promise leaks

**Status:** Phase 0 shipped and locally validated on 2026-09-10. Plan 0027 subsequently replaced arithmetic Results with explicit `Arith` policies. The only remaining design question is whether non-finite float results should also request an effect. The active checklist below follows the shipped plain-value arithmetic contract; the original Result proposal is historical evidence, not implementation direction.
**Original audited invariant:** *every operation whose exact mathematical result can fall
outside its result type must surface that as a typed failure, discharged once at
the end of an expression — never silently, never as a trap.*
**Original audit scope:** `crates/osprey-types/src/expr.rs`,
`crates/osprey-codegen/src/{expr,conv,cast,gpu}.rs`, `docs/specs/0002`, `0004`,
`0012`, `0013`, `0034`, and the 222-file `.osp`/`.ospml` corpus.

The original findings and quoted probes below describe the pre-fix behavior.
The completion record and checklist state the current implementation status.

---

## Phase 0 completion — 10 September 2026

- **F1:** `cmp_code` emits `fcmp une` for float inequality. The Default and ML
  boolean corpus tests cover all six predicates, NaN in either operand,
  equality complements, finite controls, infinities and signed zero.
- **F5:** Numeric operand requirements travel through the existing generic
  scheme obligations. Sixty invalid-call cases cover both flavors, five
  operators, both operand positions, and strings, bools and lists. Aliases and
  higher-order calls retain the constraint; runtime goldens prove one helper
  still accepts both integer and float callers and preserves known error channels.
  Both-flavor `float_operand_constraint.ospo` fixtures pin exact checker errors.
- **F7:** Internal float narrowing emits `llvm.fptosi.sat.i64.f64`. Existing
  conversion assertions remain, and twelve boundary inputs pin the safe emission.
  Additional native and WASM execution probes passed all twelve boundary results.
- **F9:** Both frontends reject overflowing float literals at the source token.
  Default already had the range guard; ML now matches it. Tests preserve the
  largest finite float, negative values and signed zero, and both-flavor
  `float_literal_overflow.ospo` fixtures pin the exact diagnostics.

The final `make ci` passed with every coverage and duplication threshold
unchanged. All 213 assertion suites and byte-exact goldens passed under each
native allocator, with zero ARC leaks. The WASM corpus passed 147 goldens;
each native/WASM pass also verified 18 alternative GPU-lowering runs and six
doctests. The complete website suite passed 118 browser tests. The compiler,
editor, documentation and application gates remained enabled throughout.

Current contracts are [FLOAT-OPERANDS], [FLOAT-COMPARE] and [FLOAT-CONVERT] in
[spec 0004](../specs/0004-TypeSystem.md), and [FLOAT-LITERAL-RANGE] in
[spec 0002](../specs/0002-LexicalStructure.md).

## 1. Original audit verdict

The integer model is correct and is exactly the model the invariant asks for.
The float model is a deliberate, spec-documented **opt-out** of it, and that
opt-out was never justified in the spec, never bounded, and leaks into three
places where it stops being a defensible IEEE-754 decision and becomes a plain
defect.

The independent float comparison, operand-checking, narrowing and literal
defects are now fixed. The stronger float fault policy remains a separate decision.

| # | Finding | Severity | Kind |
|---|---------|----------|------|
| F1 | Float inequality includes NaN (`fcmp une`) | **Critical** | Resolved |
| F2 | Float `+ - *` produce `inf`/`NaN` silently, untyped | **High** | Design opt-out |
| F3 | Float `/` and `%` return `Result` but detect only a zero divisor | **High** | False assurance |
| F4 | `?:` on a plain float is a hard error — no forward-compatible spelling | **High** | Migration blocker |
| F5 | Numeric constraints survive generic calls and aliases | Medium | Resolved |
| F6 | GPU kernels structurally reject `Result` accumulators | Medium | Blocks the fix |
| F7 | Internal float-to-integer narrowing saturates safely | Low | Resolved |
| F8 | Comparison, operand, literal and narrowing contracts are recorded; stronger float fault policy remains open | **High** | Policy/spec work remains |
| F9 | Overflowing float literals are rejected in both flavors | Medium | Resolved |

---

## 2. Historical integer model — replaced by plan 0027

This is the reference the float path should be measured against, and it already
satisfies the "check only the last step" requirement.

`crates/osprey-types/src/expr.rs:882-885` documents the flattening rule, and
`int_arithmetic_result` (`:958-962`) makes integer `+ - *` fallible. Arithmetic
is the **sole** failure-preserving `Result` flattening context: an operand's
success type is inspected to pick the overload, but one outer `Result` survives
whenever either operand already carries an error channel.

The consequence is the ergonomic the invariant wants. Probe:

```osprey
fn chain(a: int, b: int, c: int) = (a + b) * c
fn main() = print("chain=${chain(2, 3, 4) ?: -1}")
```

```
chain=20
```

Two overflow-capable operations, **one** `Result`, **one** `?:` at the end. Not
`((a + b) ?: 0) * c`. Codegen backs it with `llvm.sadd/ssub/smul.with.overflow.i64`
(`crates/osprey-codegen/src/expr.rs:308-313`). Integer `/`, `%`, `intDiv` and
unary `-` are all guarded, including the `INT64_MIN ÷ -1` poison pair
(`i64_div_guards`, `expr.rs:385-390`).

**Nothing about this model is int-specific.** It transfers to float unchanged.

---

## 3. Original findings

### F1 — Wrong float inequality predicate — resolved

`crates/osprey-codegen/src/expr.rs:643-658`, `cmp_code`:

```rust
("==", true) => "oeq",
("!=", true) => "one",     // <-- ordered not-equal
```

`fcmp one` is *ordered* and not equal: it is `false` when either operand is
`NaN`. The IEEE-754 and universal-language predicate for `!=` is `fcmp une`
(*unordered* or not equal), which is `true` for `NaN`.

Probe:

```osprey
print("C nan==nan is ${nan == nan}, nan!=nan is ${nan != nan}")
```

```
C nan==nan is false, nan!=nan is false
```

Both are `false`. This breaks the law `(a != b) == !(a == b)` for every Osprey
program, and it silently breaks the single idiom every other language uses to
detect a `NaN` — `x != x`. C, Rust, Python, JavaScript, Swift and Java all
return `true` here.

The ordered codes for `<`, `<=`, `>`, `>=` are **correct** — those genuinely
should be `false` under `NaN`. `!=` is the lone deviation. This is a one-token
fix and is independent of the design argument in F2.

### F2 — Float `+ - *` silently manufacture `inf` and `NaN`. **High**

`crates/osprey-types/src/expr.rs:942-952` returns bare `Type::float()` whenever
either operand is float; `crates/osprey-codegen/src/expr.rs:295-307` emits plain
`fadd`/`fsub`/`fmul` with the comment *"IEEE-754 arithmetic stays plain."*

Probe — squaring `2.0` ten times, then subtracting:

```
B inf=inf nan=nan
D nan+1.0=nan
```

The exact mathematical result left the representable range and the program did
not stop, did not fail, and did not change type. It produced a value that is not
a real number and kept going. Composed with F1, a `NaN` then makes **both** `==`
and `!=` return `false`, so a corrupted computation can silently pass an
equality assertion in either direction.

This is the case the user's report is about. The defence — "IEEE-754 has no
trap, `inf` is in-band, so there is nothing to catch" — is true about the
hardware and beside the point about the type. `inf` and `NaN` are in-band
*representations* of *out-of-range* and *undefined*. That is precisely what
`Result` exists to name. The int path does not wrap because i64 overflow is UB;
it wraps because the answer is wrong, and the float answer is equally wrong.

The asymmetry is visible in one screen of the user's own test file,
[tests/core/gpu/buffers.test.osp](../../tests/core/gpu/buffers.test.osp):

| Line | Kernel | Discharges? |
|------|--------|-------------|
| [5](../../tests/core/gpu/buffers.test.osp#L5) | `fn square(x) = (x * x) ?: 0` | yes |
| [8](../../tests/core/gpu/buffers.test.osp#L8) | `fn addInts(acc, x) = (acc + x) ?: acc` | yes |
| [11](../../tests/core/gpu/buffers.test.osp#L11) | `fn scale(x) = x * 1.5` | **no** |
| [14](../../tests/core/gpu/buffers.test.osp#L14) | `fn addFloats(a, x) = a + x` | **no** |

Same operator, same overflow question, two different contracts, no marker at the
call site telling a reader which one they are in.

### F3 — Float `/` and `%` return `Result` but only detect a zero divisor. **High**

`crates/osprey-types/src/expr.rs:896-904` types `/` and `%` as
`Result<float, MathError>`. Codegen (`gen_division`, `gen_remainder`,
`expr.rs:317-362`) guards with `fcmp oeq double ..., 0.0`.

That guard catches exactly one failure mode. Probe:

```
A divzero=-1.0        (1.0 / 0.0  -> Failure, correct)
G 0.0/0.0=-99.0       (0.0 / 0.0  -> Failure, correct)
E inf/2.0=inf         (Success(inf)  <-- overflow passed through as success)
F nan%2.0=nan         (Success(nan)  <-- NaN passed through as success)
```

This is worse than F2, not better. F2 is honestly untyped. F3 hands the caller a
`Result<float, MathError>`, the caller writes `?:` and reasonably concludes the
float is now trustworthy, and receives `Success(inf)`. A `Result` that is
`Success` for a non-finite result is an assurance the type does not deliver.
`1e300 / 1e-300` takes this path.

### F4 — `?:` on a plain float is a hard error, so there is no forward-compatible spelling. **High**

Probe:

```osprey
fn main() = print("v=${(2.0 * 1.5) ?: 0.0}")
```

```
elvis.osp: `?:` needs a Result on its left, found float
```

An author who *wants* to be defensive about float overflow today cannot be. The
language rejects the attempt. This has two consequences:

- No existing program can be written to survive a future totality change.
- Any such change is a hard breaking change to every float expression at once,
  with no deprecation window and no opt-in period.

The blast radius is bounded: **20 of 222** corpus files use float arithmetic.
That is the entire migration cost, and it will only grow.

### F5 — Missing numeric operand constraints — resolved

`crates/osprey-types/src/expr.rs:943-948` returns `Type::float()` without a
`push_unify` on either operand. So in `fn scale(x) = x * 1.5`, `x` stays a free
type variable and `scale` generalises to `∀a. a -> float`.

Per-call-site monomorphisation saves the *values* — verified:

```
int-site=4.5 float-site=3.75 inf-site=inf
```

but the *diagnostic* is lost. `scale("hello")` is rejected only at codegen:

```
loose.osp: codegen: invalid program: expected a number
```

No source location, no type names, no "cannot unify String with float". Compare
the checker's own message for the same class of error: `type mismatch: cannot
unify int with float`. The float branch is the only arithmetic branch that skips
its constraint; `int_arithmetic_result` and the string/list/map branches all
call `push_unify`.

### F6 — GPU kernels structurally reject `Result` accumulators. **Medium**

`crates/osprey-codegen/src/gpu.rs:252` and `:343` reject a `tmpl.result_inner`:

```
"a gpuFold accumulator must be a scalar (int, float, or bool)"
```

and `:161-163` restricts buffer elements the same way. This is correct for the
buffer ABI — a buffer word is 64 bits and a `Result` is a pointer to a payload
triple.

It also means any fix to F2 must keep the discharge **inside** the kernel body,
exactly as `square` and `addInts` already do at
[buffers.test.osp:5](../../tests/core/gpu/buffers.test.osp#L5) and
[:8](../../tests/core/gpu/buffers.test.osp#L8). That is a constraint on the fix,
not an argument against it — the int kernels prove the shape already works. But
it does mean "check only at the last step" has a hard boundary at the kernel
edge, and the spec must say so.

### F7 — Poison-producing internal float narrowing — resolved

`crates/osprey-codegen/src/conv.rs:16`:

```rust
LType::Double => cg.emit_reg(format!("fptosi double {} to i64", v.operand)),
```

LLVM `fptosi` is **poison** when the source is `inf`, `NaN`, or out of i64 range.
With `inf` now freely constructible via F2, the only thing standing between the
corpus and UB is the type checker refusing to route a double into an `i64`
boundary. It currently holds — verified:

```
reject.osp: type mismatch: cannot unify int with float
```

`as_i64` reachable sites are `coerce_to` (`cast.rs:30`), record fields
(`aggregate.rs:194`), collection/GPU indices (`listlit.rs:205`, `gpu.rs:315`),
and range bounds (`iter.rs:148-150`) — all int-typed by the checker. Note the
GPU float path deliberately avoids this: float words `bitcast` rather than
convert (`gpu.rs:143-144`).

So this is not an active bug. It is a UB path whose only guard is F5's branch —
the one branch that skips its constraint. It should be a saturating conversion
or an explicit guard regardless of what happens to F2.

### F8 — The spec states the rule but never its consequence. **High**

The spec is *internally consistent* — implementation matches text in all three
places. That is the problem: it documents the opt-out without ever admitting
what the opt-out costs.

| Location | Text | Missing |
|----------|------|---------|
| [0002:71-74](../specs/0002-LexicalStructure.md) | "floating-point `+`, `-`, `*`, and unary `-` remain plain IEEE-754 operations" | why "remain"; what a reader must do about it |
| [0004:47-49](../specs/0004-TypeSystem.md) | "the IEEE-754 operation returns plain `float`" | no mention of `inf`/`NaN` |
| [0013:41-62](../specs/0013-ErrorHandling.md) `[ARITH-CHECKED]` | full operator table, correct | never says the float row can produce non-finite values, never says `/`'s `Result` does not cover overflow, never states a NaN-comparison rule |

The original audit identified these documentation gaps. Comparison semantics
are now specified by [FLOAT-COMPARE], and spec 0037 explicitly describes
IEEE-754 infinity and NaN. The stronger float fault-policy contract is still open:

1. That `inf` and `NaN` are constructible by ordinary arithmetic. Only
   [0012:401](../specs/0012-Built-InFunctions.md) mentions them, and only to say
   `parseFloat` **rejects** them — which reads as though the language excludes
   them. Arithmetic is the sole producer, and no spec says so.
2. That `Result<float, MathError>` from `/` and `%` means *zero divisor only*.
3. Any statement of comparison semantics under `NaN`. Because the spec is silent,
   F1 is not even a spec violation — there is no text to violate.
4. A `[FLOAT-TOTALITY]` (or similar) spec ID. `[ARITH-CHECKED]` covers the
   checked half; the unchecked half has no ID, so no code comment can reference
   it and `/spec-check` cannot audit it.

---

## 4. Blast radius for a fix

| Area | Files | Note |
|------|-------|------|
| Type rules | `crates/osprey-types/src/expr.rs` `infer_arith` `:895-953`, `infer_negation` `:967-974` | the float arms of `+ - *`; also fixes F5 |
| Codegen | `crates/osprey-codegen/src/expr.rs:295-307` | needs an `fcmp ord`/`isfinite` guard mirroring `gen_checked_arith` |
| Comparison | `crates/osprey-codegen/src/expr.rs:643-658` | F1, independent, one token |
| Division truth | `crates/osprey-codegen/src/expr.rs:317-362` | extend guard from zero-divisor to non-finite result |
| Conversion | `crates/osprey-codegen/src/conv.rs:16` | F7 saturation |
| GPU | `crates/osprey-codegen/src/gpu.rs:161,252,343` | unchanged; discharge stays inside kernels |
| Spec | `0002`, `0004`, `0013` (`[ARITH-CHECKED]`), `0034` | new ID + consequence text |
| Corpus | 20 of 222 `.osp`/`.ospml` files | plus `.expectedoutput` twins and ML twins |

---

## 5. Historical options — superseded by arithmetic effects

Recorded so the decision is explicit rather than inherited.

- **A — Full totality.** Float `+ - *` return `Result<float, MathError>` when
  the result is non-finite and an operand was finite. `/` and `%` extend their
  guard to the same condition. Uniform with int, satisfies the invariant, single
  `?:` at the end via the existing flattening. Costs: 20 files, a real break, and
  every float kernel gains a `?:` like its int sibling already has.
- **B — Totality plus a total escape hatch.** A, plus an explicit
  `unchecked`/`ieee` form for code that genuinely wants raw IEEE semantics
  (numerics kernels where `inf` is a legitimate sentinel). Keeps the default
  safe, keeps IEEE reachable, adds surface area.
- **C — Fix F1, F3, F5, F7, F8 only.** Leave F2's opt-out but make it honest:
  correct `!=`, make `/`'s `Result` actually mean finite, constrain the operands,
  saturate the conversion, and write the spec text that says plainly what
  arithmetic can produce and what the caller must do. Smallest break, but leaves
  the two-contracts-one-operator asymmetry in the language.

F1, F5 and F7 are defects under **all three** options and are not coupled to the
design decision.

## 6. Historical specification proposal

`[ARITH-CHECKED]` in `0013` must gain, and `0002`/`0004` must cross-reference:

- The exhaustive set of operations that can produce a non-finite float.
- What `Result<float, MathError>` from `/` and `%` does and does not cover.
- The comparison contract under `NaN`, for all six operators, as a table.
- A named spec ID for the float contract so implementing code can cite it and
  `/spec-check` can audit it.

---

## 7. Current delivery checklist

[Spec 0037](../specs/0037-ArithmeticEffects.md) is normative. Integer arithmetic returns plain values and requests `Arith` on faults. Float `+`, `-`, `*` and unary negation use IEEE-754 closure; `/` and float `%` retain explicit zero-divisor recovery. No item below authorizes restoring arithmetic `Result` flattening or treating `?:` on a plain value as a no-op.

### Delivered independently of the remaining float decision

- [x] F1: unordered float inequality, with all six NaN predicates pinned in the Default/ML boolean corpus.
- [x] F5: numeric operand constraints survive aliases, generic calls and higher-order transport; both-flavor rejection fixtures retain the exact diagnostics.
- [x] F7: internal float narrowing uses saturating LLVM conversion, with twelve boundary inputs checked natively and on WASM.
- [x] F9: both frontends reject non-finite float literals at the source token and preserve finite boundaries and signed zero.
- [x] F3's Result assurance is removed by plan 0027: float division and remainder return float, and zero divisors request `Arith.divideByZero`. They do not promise a finite answer.
- [x] F4's migration decision is fixed by [ARITH-EFFECT]: `?:` accepts only a genuine Result. No compatibility no-op or arithmetic flattening survives.
- [x] F6's arithmetic Result obstacle is removed: host GPU kernels carry scalar values and use the enclosing arithmetic policy. Device execution remains owned by plan 0023.

### Remaining non-finite-result decision and implementation

- [ ] Decide whether IEEE-754 non-finite results remain ordinary float values or additionally request a new `Arith` operation. Update [ARITH-TOTAL] and the numeric builtin contract before implementing a different policy. Preserve the recorded zero-divisor behavior.
- [ ] Define the complete operand/result matrix: finite overflow, infinities supplied as operands, NaN propagation, signed zero, division by zero and float remainder. State the behavior for operators, conversions and host GPU kernels together.
- [ ] If the decision extends `Arith`, implement the typed operation, inference, scope/discharge, native/WASM dispatch and total IEEE escape policy as one change. Arithmetic still returns plain numeric values; propagation uses the same effect system as integers.
- [ ] Pin the chosen matrix with both-flavor runtime assertions and byte-exact goldens, plus rejection fixtures for any new unhandled operation. Existing comparison, narrowing and literal tests must remain intact.
- [ ] Verify Default, GC, ARC and WASM; retain ARC ownership checks and GPU lowering differentials. Run the applicable float benchmarks on this machine before changing published performance claims.
- [ ] Make the specs, messaging and builtin documentation agree with the selected policy, pass `make ci` without weakening a gate, and retire this plan with named evidence.
