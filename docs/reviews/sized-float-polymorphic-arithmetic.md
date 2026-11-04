# Bug: unannotated helpers can't do arithmetic on `float32` or `float16`

Status: open. Spec: [TYPE-NUMERIC-ARITH-WIDTH] in `docs/specs/language/0004-TypeSystem.md`. Tracked in plan 0050, Phase 3.

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
