# Bug: inferred division rejects `float32` and `float16`

Status: fixed and verified on the primary checkout. Spec: [TYPE-NUMERIC-ARITH-WIDTH] in `docs/specs/language/0004-TypeSystem.md`. Tracked in plan 0050, Phase 3.

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
- [x] Retain division's operand/result relation in each instantiation, while preserving the existing rejection for helpers fixed to binary64 by a literal. Integer `/` still returns `float`; mixed `int`/`float` calls remain valid.
- [x] Native Default regression matches independently computed Python `struct` bit goldens for all five operators at both widths. Cancellation distinguishes binary32 and binary16 intermediate rounding from binary64.
- [x] All 941 type-checker tests pass, including 20 Default/ML negative controls for fixed-binary64 helpers; strict package clippy and formatting pass.
- [x] Current primary checkout transformer package retains all 395 GC assertions. It has no `toFloat32(0.0)` workaround anchors; the separate Phase 4/5 migration owner has been notified.
- [x] Verify the final Default/ML regression, aliases, signed zero, subnormals and local `Result` wrappers under GC, ARC and wasm32. All nine exact-golden runs pass, including the unchanged legacy pattern-matching golden; native ARC runs each report zero live objects. Receipt: `polymorphic-verified-receipts.json`.
- [x] Full Rust workspace tests and every Rust coverage floor pass on the final code: codegen 96.3% (floor 95%), types 98.3% (floor 98%); all 941 type tests pass under coverage. The declaration-documentation and exact rejection-golden gates also pass.
- [x] All 26 package coverage projects meet their floors. Direct IR inspection of the unannotated reported helper confirms native `fdiv float` for both widths, followed by binary16 rounding for `float16`; receipt: `polymorphic-native-width-receipt.json`.
- [x] All C runtime tests and per-library C coverage floors pass in the full run, including exhaustive binary16 conversion and binary32 spelling checks.
- [x] Full GC conformance passes: 318 programs, 318 exact goldens, 20 alternate GPU-lowering cases and all six doctests.
- [x] Full ARC conformance passes: 318 programs, 318 exact goldens, zero leaky programs, 20 alternate GPU-lowering cases and all six doctests.
- [x] Source VS Code extension suites pass (880 + 14 tests). Installed VSIX checks also pass (170 tests); all four extension coverage measures meet their 95% floors.
- [x] Full `make test` exits 0, including all host suites and coverage floors. Transformer package GC and ARC retain all 395 assertions (ARC: zero live objects). Receipt: `polymorphic-make-test-exit.json`.
- [x] Fix the separately identified WASI `tanhf` rounding discrepancy in the wasm-only runtime. A loaded ten-input regression is RED at ±0.5 before the fix and GREEN afterward against independent Python bit constants. Both flavors pass GC, ARC and wasm; all 234 supported wasm programs and goldens pass, with zero ARC leaks, 20 alternate GPU cases and six doctests. The 84 pinned unsupported programs are unchanged.
- [x] Recheck the wasm proof kernel after the runtime change: all 400 cases pass. IR parity and all four documentation/source-style gates pass. Final wasm artifact/source hashes: `wasm-tanh-cohort.json`.

Receipts are in `target/logs/plan-0050/polymorphic-*`. No codegen change was needed for the verified native arithmetic: existing specialization emits the sized operations once inference supplies the correct result type. Other arithmetic operators retain the existing one-overload-per-definition rule; the division relation preserves its existing ability to instantiate mixed integer/binary64 operands.

The first wide run exposed a division `Result` regression (`good elvis = 0.0` instead of `5.0`), fixed by keeping computed numeric values monomorphic and generalizing the relation only with all its operand variables. The original golden is unchanged. Wasm transformer math assertion 4 also fails with the older release compiler; the GPU owner has its receipts, and this remains an open whole-corpus gate.

The full test command caught an IR mismatch in the new ML regression, corrected by spelling its parameter and argument tuples like the Default twin. The unchanged cross-flavor IR gate now passes. A separate RED negative-control test caught an overly broad removal of the numeric guard; the guard is retained, and only the deferred division result relation changes. The report's minimal example also needs the existing `Arith.divideByZero` handler to execute; with that policy installed it prints `0.75`.

Final focused verification at `02d1c952`: all nine Default/ML/legacy golden comparisons pass again after restoring the fixed-width controls. Both code fixes and checklist updates are committed; the final `make test` receipt is `polymorphic-make-test-complete.log`.

The next full-suite pass reached the exact must-reject diagnostics gate and exposed ordering drift in the Default/ML `float_operand_constraint` cases. The checker now inserts late division diagnostics at their original obligation site; the complete rejection-golden gate passes with every golden unchanged.

The host `make test` pass precedes the wasm-only runtime follow-up. After that follow-up, the rebuilt wasm runtime passes the complete wasm corpus, the 400-case wasm proof suite and both flavors of the focused regression under all three execution lanes; the native `tanhf` path and compiler lowering are unchanged.

PR #339 cross-platform follow-up: the earlier `6612e263` Linux jobs also fail to link the binary32 libm symbols (`expf`, `logf`, `tanhf`) and differ in generated NaN signs. Those are distinct from the division bit regressions and remain with the CI owner; the local wasm follow-up is held for a coordinated push so it does not cancel the live run. Local passes above do not claim the PR CI is green.
