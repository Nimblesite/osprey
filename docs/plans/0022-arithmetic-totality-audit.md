# Arithmetic Totality Audit — where the checked-arithmetic promise leaks

**Status:** Phase 0 complete and locally validated on 2026-09-10. F1, F5, F7 and F9 are fixed; F10 was already implemented. The remaining float fault-policy decision, implementation and migration keep this plan open.
**Audited invariant:** *every operation whose exact mathematical result can fall
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

## 1. Verdict

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

## 2. What is already right — the integer model

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

## 3. Findings

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

## 5. Options, not recommendations

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

## 6. Minimum spec text any option requires

`[ARITH-CHECKED]` in `0013` must gain, and `0002`/`0004` must cross-reference:

- The exhaustive set of operations that can produce a non-finite float.
- What `Result<float, MathError>` from `/` and `%` does and does not cover.
- The comparison contract under `NaN`, for all six operators, as a table.
- A named spec ID for the float contract so implementing code can cite it and
  `/spec-check` can audit it.

---

## 7. TODO checklist

Sequenced for **Option A (full totality)** — the position that a value which can
be non-finite must carry a `Result`, discharged once at the end of the chain.
Phase 0 is complete under every option. The remaining checklist records the
original Result-based proposal; the chosen float policy must also reconcile with
[spec 0037](../specs/0037-ArithmeticEffects.md), which reserves the non-finite
float decision for this plan while moving integer faults to `Arith`.

### Phase 0 — Design-independent defects

- [x] **F1** Float `!=` uses `fcmp une`; the other five comparison predicates remain ordered.
- [x] **F1** Both-flavor corpus truth tables cover NaN detection and equality complements, with finite, infinity and signed-zero controls.
- [x] **F5** Numeric operands are checked through generic scheme obligations. Integer and float callers remain valid; direct float unification would have broken that requirement.
- [x] **F5** Both-flavor must-reject fixtures pin invalid numeric-call diagnostics before code generation.
- [x] **F7** Bare `fptosi` is replaced by `llvm.fptosi.sat.i64.f64`, with boundary and existing coercion assertions.
- [x] **F9** Both frontends reject non-finite float literals, preserving finite extremes and signed zero.
- [x] **F10** Context-free arithmetic **int-defaulted before the consuming slot
      could constrain it**: `fn plus(a, b) = a + b` could never serve a float
      fold — anywhere in the language — because the operands defaulted to `int`
      inside the definition instead of unifying with the slot's element type
      (`GpuBuffer<float>`, `List<float>`, a `(float, float) -> float`
      parameter). **Fixed.** An arithmetic site whose operands are both still
      unconstrained records a pending overload instead of defaulting
      (`crates/osprey-types/src/expr.rs::deferred_arith`); the choice is made
      once, after all unification, by re-running the ordinary selection over
      the operands' final types. `tests/core/gpu/kernel_frontier.test.{osp,ospml}`
      is the corpus proof — an unannotated `plus` specialising at `float` from
      `gpuFold`'s and `gpuScan`'s slots, alongside an unannotated recursive
      helper — and it satisfies [GPU-KERNEL-ELEM-TYPING].

      **Scope, deliberately:** the operand does NOT generalize, so one
      definition gets ONE overload. A helper used at both `int` and `float` in a
      single program is a type error, not a reinterpretation. Sharing one
      definition across both would need a real numeric class — quantifying over
      the overload, whose two arms differ in SHAPE (checked
      `Result<int, MathError>` versus total `float`), not just element type.
      That is a separate design decision and belongs with the float-totality
      decision below, not with this defect.

### Phase 1 — Spec first (code comments must cite an ID)

- [ ] Add `[FLOAT-TOTALITY]` to `docs/specs/0013-ErrorHandling.md` beside
      `[ARITH-CHECKED]`: float `+ - * / %` and unary `-` yield
      `Result<float, MathError>` when the result is non-finite and every operand
      was finite.
- [x] Add `[FLOAT-COMPARE]`: the six comparison operators under `NaN`, as a
      table, stating `!=` is unordered and the other five are ordered.
- [ ] State the flattening rule explicitly — a chain yields exactly **one**
      outer `Result`; one `?:` discharges it. This is the "only the last step"
      guarantee and it is currently written nowhere.
- [ ] Cross-reference from `0002-LexicalStructure.md` (literals),
      `0004-TypeSystem.md` (arithmetic typing) and
      `0034-GPUComputation.md` (kernel purity + scalar ABI).
- [ ] Delete or rewrite any spec sentence implying float arithmetic is total
      and unchecked (**F8**).

### Phase 2 — Type rules

- [ ] `infer_arith` float arms return `res_math(Type::float())` for `+ - *`,
      matching `int_arithmetic_result`.
- [ ] `infer_negation` — unary `-` on float follows the same rule.
- [ ] Confirm the existing flattening path treats float `Result` exactly like
      int `Result` so nested chains do **not** produce `Result<Result<…>>`.
- [ ] Unit tests in `osprey-types` for: single op, nested chain, mixed
      int/float rejection, and the flattening arity.

### Phase 3 — Codegen guard

- [ ] Mirror `gen_checked_arith` for floats: compute, then test
      `isfinite(result) || !isfinite(operand)` and build the `Success`/`Err`
      payload triple. Cite `[FLOAT-TOTALITY]` in the comment.
- [ ] Replace the "IEEE-754 arithmetic stays plain" comment at
      `crates/osprey-codegen/src/expr.rs:295-307`.
- [ ] Keep the emitted guard branch-free where possible; verify no regression in
      the float benchmarks.

### Phase 4 — Division and remainder truth (**F3**)

- [ ] `gen_division` `:317-329` — extend the guard past `fcmp oeq …, 0.0` to the
      non-finite-result condition, so `Success` genuinely means finite.
- [ ] `gen_remainder` `:335-362` — same.
- [ ] Corpus cases: `inf / 2.0`, `nan % 2.0`, `1.0 / 0.0`, `0.0 / 0.0` — each
      must be `Err`, not `Success(inf)`.

### Phase 5 — `?:` and migration ergonomics (**F4**)

- [ ] Decide and record: does `?:` on a plain non-`Result` stay a hard error, or
      become a no-op so a defensive spelling can land ahead of the break? A
      no-op makes the migration two-step instead of a flag day.
- [ ] If it stays an error, land Phases 2-4 and 6 in a single change — a partial
      landing leaves the corpus unbuildable.

### Phase 6 — GPU boundary (**F6**)

- [ ] Confirm kernels still discharge internally: `gpuMap`/`gpuFold` element and
      accumulator types stay bare scalars; `Result` never crosses the ABI.
- [ ] Update `tests/core/gpu/buffers.test.osp` — `scale` and `addFloats` gain
      `?:` exactly like `square` and `addInts` already have. This is the file
      that triggered this audit; it should read symmetrically when done.
- [ ] Update the `.ospml` twin and confirm both flavors match the single golden.
- [ ] Verify the `gpu.rs:343` accumulator error message still reads correctly.

### Phase 7 — Corpus migration

- [ ] Enumerate the current `.osp`/`.ospml` files using float arithmetic;
      list them in this document before touching any of them.
- [ ] Add `?:` at the last step of each chain — **not** at every operation. Any
      diff that adds more than one `?:` per expression means the flattening is
      wrong, not the test.
- [ ] Regenerate `.expectedoutput` goldens only where output legitimately
      changes; a changed golden with unchanged intent is a bug signal.
- [ ] Run the differential harness under every memory backend **and**
      `OSPREY_TARGET=wasm32`.

### Phase 8 — Error cases and docs

- [ ] `examples/failscompilation/` — undischarged float arithmetic at a kernel
      boundary, at `main`, and as a buffer element; each with an
      `.expectedoutput`.
- [ ] Update `builtin_docs.rs` / `builtin_docs_lang.rs` for any builtin whose
      float signature changes; check `parseFloat`'s non-finite rejection is
      still consistent with the new invariant.
- [ ] Update `docs/messaging.md` if the totality claim is now stronger than what
      is currently written there.

### Phase 9 — Verification

- [ ] `make ci` green.
- [ ] Coverage thresholds in `coverage-thresholds.json` still met.
- [ ] Every new code path cites `[FLOAT-TOTALITY]` or `[FLOAT-COMPARE]`.
- [ ] Re-read this audit's findings F1-F9 and confirm each is closed or
      explicitly deferred with a reason.
