# Bug: unannotated helpers can't do arithmetic on `float32` or `float16`

Status: fix implemented; full corpus verification in progress. Spec: [TYPE-NUMERIC-ARITH-WIDTH] in `docs/specs/language/0004-TypeSystem.md`. Tracked in plan 0050, Phase 3.

## Reproduce

```osprey
fn div(a, b) = a / b
fn main() = {
  print(toString(div(toFloat32(1.5), toFloat32(2.0))))
}
```

Actual, from `target/debug/osprey file.osp --run`:

```
file.osp:1:17: operator `/` requires int or float; got float32
```

Expected: the helper stays polymorphic and the program divides in binary32, the way it does for `float`. An annotated `fn div(a: float32, b: float32)` isn't the fix: a program must not need an annotation the checker can infer.

## Cause

`invalid_use` in `crates/osprey-types/src/builtin_constraints.rs` refuses sized floats for arithmetic operators when the obligation is checked at instantiation:

```rust
let sized_float =
    crate::numeric::Numeric::of_type(ty).is_some_and(|n| n.is_sized() && n.is_float());
let arithmetic = matches!(op, "+" | "-" | "*" | "/" | "%");
let valid = is_numeric_scalar(ty) && !(arithmetic && sized_float);
```

The guard names all five operators. Only `/` was reproduced; check each operator at both `float32` and `float16`. The fix isn't necessarily just deleting the guard: codegen for the instantiated helper has to compute at the sized width too, so find out why the guard was added first.

## Where it lives

- Not on `main`: `origin/main` (78be07ec) has no `float32` yet, so this has no GitHub issue.
- Introduced in ffdd0b01, which is on `docs/adversarial-review-reports` and in the unmerged `wip/plan-0050-pis`.
- Reproduced on `wip/plan-0050-pis` at c843e895, on 2026-11-04.

## Finish it

1. Add a failing regression first, in both flavors, for every operator at `float32` and `float16`, with expected values from an independent source such as Python `struct`.
2. Fix the checker, plus codegen if it needs it.
3. Remove the `toFloat32(0.0)` anchors in `packages/transformers/src` that this bug forced, and keep its 395 assertions passing under GC and ARC.
4. Run the GC, ARC and wasm32 corpus lanes, then tick the item in `docs/plans/0050-sized-numeric-types.md`. That item is on `wip/plan-0050-pis`, commit 3dc2821b.

## Verification checklist

- [x] Reproduce RED in the existing `sized_floats` corpus regression: inferred division rejects both widths and incorrectly fixes its result to `float`.
- [x] Fix the operand guard and retain division's operand/result relation in each instantiation. Integer `/` still returns `float`; mixed `int`/`float` calls remain valid.
- [x] Native Default regression matches independently computed Python `struct` bit goldens for all five operators at both widths. Cancellation distinguishes binary32 and binary16 intermediate rounding from binary64.
- [x] All 940 type-checker tests and strict package clippy pass.
- [x] Current primary checkout transformer package retains all 395 GC assertions. It has no `toFloat32(0.0)` workaround anchors; the separate Phase 4/5 migration owner has been notified.
- [x] Verify Default/ML regression, aliases, signed zero and subnormals under GC, ARC and wasm32. Added a further `Result` wrapper regression after the wide corpus exposed premature value generalization; its final rerun remains part of the full verification below.
- [ ] Finish full GC, ARC and wasm32 corpus lanes and final regression assertions. Transformer package GC and ARC both retain all 395 assertions (ARC: zero live objects).

Receipts are in `target/logs/plan-0050/polymorphic-*`. No codegen change was needed for the verified native arithmetic: existing specialization emits the sized operations once inference supplies the correct result type. Other arithmetic operators retain the existing one-overload-per-definition rule; the division relation preserves its existing ability to instantiate mixed integer/binary64 operands.

The first wide run exposed a division `Result` regression (`good elvis = 0.0` instead of `5.0`), fixed by keeping computed numeric values monomorphic and generalizing the relation only with all its operand variables. The original golden is unchanged. Wasm transformer math assertion 4 also fails with the older release compiler; the GPU owner has its receipts, and this remains an open whole-corpus gate.
