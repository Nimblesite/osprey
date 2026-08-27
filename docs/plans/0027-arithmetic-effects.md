# Arithmetic as an Effect — retiring `Result` arithmetic and the `?:` fabrication tax

**Status:** design fixed by [spec 0037](../specs/0037-ArithmeticEffects.md) (normative target); implementation has not started. Driven by [#230](https://github.com/Nimblesite/osprey/issues/230). Interacts with [plan 0022](0022-arithmetic-totality-audit.md) (floats — stays there), [plan 0016](0016-algebraic-effects-and-handlers.md) (handler values gate prelude-named policies), and [plan 0024](0024-staged-effects.md) (`handle static Arith` as the eventual zero-cost policy form).

> Evidence line numbers may drift as code moves. Use the cited function and diagnostic-message names as stable anchors.

## 1. Why

Integer arithmetic returning `Result<int, MathError>` produced a corpus-wide idiom of discharging the wrapper with a fabricated literal. Measured on this tree: **6,408 `?:` sites across 231 `.osp`/`.ospml` files** (tests 6,204 in 177 files, examples 137 in 31, benchmarks 67 in 23); the dominant fallbacks are `?: 0` (3,233), `?: 0.0` (231), `?: 99` (156), `?: -1` (106). Exactly **7 files** ever inspect a `MathError`. A fabricated fallback is a silent wrong answer with exit code 0 — #230 demonstrates a hash whose accumulator resets to zero mid-string, a fold that discards its running total, and a ledger deposit that vanishes. The redesign moves the failure from a value every call site must discharge to an effect one region's policy handles: `+` returns `int`, overflow performs `Arith.overflow`, and the statically required handler substitutes the policy's value. No panic is introduced anywhere; halting is unrepresentable in the operation signatures.

This also aligns the Default flavor with the systems languages it is converging on — `h * 33 + b` is once again an `int` expression — without adopting their silent wraparound.

## 2. Phases

| Phase | Delivers | Depends on |
|---|---|---|
| 0 | Red tests pinning #230's silent wrong answers | — |
| 1 | Default `handle ... in` → `handle ... do` | — |
| 2 | Checker: plain arithmetic types, `Arith` seeding, whole-effect arm re-entry rule | 1 |
| 3 | Codegen: cold branch dispatches to the active `Arith` handler | 2 |
| 4 | Total helpers (`wrapAdd`…`satMul`), constant folding, file-scope rule | 2 |
| 5 | Corpus conversion (checklist at the bottom of this document) | 3, 4 |
| 6 | Spec/doc/website finalization; benchmark re-run | 5 |

Phases 2–5 land as one PR: the type change breaks every arithmetic `?:` site by design, so the tree is not green between them. Phase 1 lands first and separately — small, mechanical, reversible — so every handler the conversion writes uses `do` from birth. Phase 0 lands before everything, red.

## 3. Phase 0 — red tests first

Per CLAUDE.md, the failing tests outrank the fix. Expand `tests/core/arithmetic/effect_policies.test.osp{,ml}` (do not add files) with cases that are wrong today and must stay wrong-loudly until the redesign lands: the masked square (`x * x ?: 0` at `4e9`), the reset fold accumulator, the vanished ledger deposit. Under today's semantics these assert the *correct* mathematical answer and therefore fail. They go green only when Phase 5 rewrites them into `Arith` policies.

## 4. Phase 1 — `in` → `do` in the Default handle form

- `tree-sitter-osprey/grammar.js` — the `handle` rule (`grammar.js:408`): `'in'` → `'do'`. Regenerate the parser.
- `crates/osprey-syntax/src/default/` — the handle parse path and its error recovery text.
- `do` becomes reserved in the Default lexer; no `.osp` in the tree uses it as an identifier (verified — all grep hits are comment prose). ML keeps `in`; its lexer already reserves `do` (`crates/osprey-syntax/src/ml/token.rs:174`).
- Convert the **51 Default files** using `handle ... in` (`tests/regressions/effects` 11, `tests/effects/resume` 6, `tests/core/collections` 6, `tests/modules` 5, `tests/regressions/fiber` 4, remainder spread across http/basics/examples). Grammar-aware edit, not blind sed — `in` also appears as the variance marker and in comment prose.
- Default snippets in `docs/specs/*.md` (27 files mention `handle`; only Default-flavor blocks change), `docs/messaging.md` if any snippet shows it, website markdown snippets, `vscode-extension/syntaxes/osprey.tmLanguage.json` keyword list, and any `examples/failscompilation` fixture whose source or expected message spells the binder.
- Goldens are unaffected (surface-only change); the differential harness proves it byte-for-byte.

## 5. Phase 2 — checker

All in `crates/osprey-types/`:

- `expr.rs` — `infer_arith` (`expr.rs:1160`): `int_arithmetic_result` (`:1297`) returns `Type::int()` and seeds `Arith.overflow`; `"/"` (`:1179`) returns `Type::float()` and seeds `Arith.divideByZero`; the `%` arms seed `remainderByZero`/`divideByZero`; `infer_negation` (`:1306`) returns `Type::int()` and seeds `overflow`. The `propagates_error`/`unwrap_result`/`result_error` flattening preamble (`:1160-1167`) **deletes** — net negative LOC. `res_math` (`:33`) loses its arithmetic callers; `res_math_like` (`:1040`) and the generic-`Error` builtins keep theirs (indexing etc. are untouched).
- Deferred-overload machinery (`defers`/`deferred_arith`) survives unchanged; only the result types it resolves to change.
- `[ARITH-EFFECT-TOTAL-SITES]`: a `/`/`%` whose divisor is a nonzero literal (post neg-literal fold) seeds nothing.
- Declare `Arith` as a compiler-known effect visible in every scope (alongside the builtin signatures in `builtins.rs`; there is no existing compiler-declared effect — this is the one genuinely new mechanism). Reject user redeclaration.
- `effect_rows.rs` — arithmetic nodes become a second requirement-seeding site beside `Expr::Perform` (`effect_rows.rs:613`). The propagation, fixed-point call analysis, partial-handler discharge and entry check need **no change** — that is the point of reusing the effect system.
- Widen the arm re-entry rule (`effect_rows.rs:2396`) for `Arith` only: any checked arithmetic in any arm of an `Arith` handler is rejected (`[ARITH-EFFECT-ARMS-NO-REENTRY]`), with the diagnostic naming `wrapAdd`/`satAdd`/the `wrapped` payload. Reject `resume` in `Arith` arms.
- File-scope initializers: constant expressions fold (`[ARITH-EFFECT-CONST]`, overflow = compile error); a non-constant fallible operation at file scope is rejected.

## 6. Phase 3 — codegen

All in `crates/osprey-codegen/`:

- The overflow intrinsics and `gen_zero_checked` (`expr.rs:453`) keep their branch structure. The cold branch stops constructing `Error("integer overflow")`/`DIVIDE_BY_ZERO` (`expr.rs:471`) and instead dispatches the `Arith` operation to the active handler — the existing direct handler-call path used by every substituting arm, on native and wasm32 alike. The wrapped value for `overflow`'s fourth payload is already in the intrinsic's result pair.
- A fault inside a fiber travels the shipped `[EFFECTS-FIBER-PERFORM]` serialized round trip; cold path only, no new runtime surface.
- `result.rs`'s arithmetic-Result construction paths go dead and are deleted.
- GPU kernels under the host backend dispatch like any lambda. Device backends (unstarted, plan 0023) will need compile-time policies — recorded there, not here.

## 7. Phase 4 — total helpers and folding

- Builtins `wrapAdd`, `wrapSub`, `wrapMul`, `satAdd`, `satSub`, `satMul`: total `(int, int) -> int`, seed nothing, documented in spec 0012. `checkedAdd`/`checkedSub`/`checkedMul` (`builtins.rs:180`, `codegen/expr.rs:910`) remain unchanged as the explicit `Result` spelling.
- Constant folding of integer arithmetic over literal operands, everywhere (not only file scope), with the compile-time overflow error.

## 8. Phase 6 — finalization

- Rewrite `[ARITH-CHECKED]`, `[ARITH-NEG-LITERAL]` context, "Chaining Arithmetic" and "Choosing and preserving a policy" in spec 0013 to the shipped model; delete the Result Preservation arithmetic carve-out in spec 0004; update the 0017 handle EBNF and 0003's precedence/keyword prose; fold spec 0037's content into the mainline specs and retire its normative-target status header.
- `docs/messaging.md`: "Arithmetic does not silently wrap" stands; add the one-region-policy sentence; update Current qualifications.
- Re-run `make bench` and regenerate `website/src/benchmarks.md` from fresh `results.json` — removing per-op Result wrappers may move numbers, and benchmark claims must stay attached to reproduced data.
- Audit whether anything still produces `MathError`; if nothing does, retire the type and its `names::MATH_ERROR` seat.
- Retire this plan per `docs/plans/README.md` once every box below is checked and the named tests prove it.

## 9. Risks and open decisions

- **Recorded decision — float zero divisors keep detection.** `/` and float `%` seed `divideByZero` rather than going pure-IEEE (`inf`/`NaN`); revisiting that belongs to plan 0022's float-totality decision, not this plan.
- **Recorded decision — no implicit default policy.** A program with fallible arithmetic and no handler is rejected. Ambient authority is what the capability model refuses; hello-world with arithmetic pays one `handle Arith ... do` line until handler values (plan 0016 Phase B) enable `handle Arith.saturating do`.
- **Diagnostic noise.** `!Arith` will appear in inferred rows in LSP hover and error messages. Acceptable; a suppression heuristic ("elide `Arith` when it is the only row entry") is a possible follow-up, not scope.
- **`result_chain_unary_stress` and `boundary_error_stress`** exist to exercise the flattening rule this plan deletes. They are rewritten to pin the new semantics (fault dispatch order, nested policies), not weakened — the behaviours they pin must have successors before the old assertions go.
- **Cold-path cost.** Dispatch replaces Result construction on the fault path only; the hot path is byte-identical branch layout. Verified by the benchmark re-run.

---

## Conversion checklist — migrating the corpus to the new style

Ground rules for every box below. **The compiler is the site classifier**: after Phase 2, every arithmetic `?:` site fails with `` `?:` needs a Result on its left, found int `` (or `float`) — fix exactly those; a `?:` whose scrutinee is a genuine `Result` (indexing, map lookup, HTTP, parsing, user functions — the `?: "none"`/`?: "missing"` population) still compiles and **stays**. Every edit is in place — no parallel files, no `_v2` copies. Each `.ospml` twin gets the identical semantic edit and must stay byte-identical to its Default twin's golden. A golden may change **only** where it printed an arithmetic `Success(...)`/`Error(...)` wrapper or a value a fabricated fallback produced; any other golden diff is a defect in the conversion, stop and investigate. Annotation rules apply throughout: inferred `!Arith` rows are never written out, and any `Result<int, MathError>` annotation the conversion orphans is deleted, not rewritten.

### R. Mechanical recipes (apply in this order at each site)

- [ ] **R1 — dead fallback, total site.** `x % 2 ?: 1`, `x / 4 ?: 0.0` (literal nonzero divisor): delete the `?: fallback`. No handler needed (`[ARITH-EFFECT-TOTAL-SITES]`).
- [ ] **R2 — fabricating fallback, fallible site.** `(a + b) ?: 0`, `x * x ?: 0`, fold lambdas `fn(a, b) => a + b ?: 0`: delete the `?: fallback`, then discharge at the nearest region that states the file's actual policy — one `handle Arith ... do` around the test body or `main`, not one per expression. Choose the policy the test *means*: wrapping for hash/checksum-shaped code, fault-sticky for everything asserting exact values.
- [ ] **R3 — self-fallback in handler-owned state.** `count = (count + 1) ?: count` inside non-`Arith` handler arms: delete the `?:`; the seeded requirement propagates out of the arm to an enclosing `handle Arith`. Inside `Arith` arms only, use `wrapAdd`/`satAdd` (R7).
- [ ] **R4 — sites that inspect the error.** `match a + b { Success ... Error ... }` and the `?:`-with-diagnostic forms: rewrite as an `Arith` handler policy, or as `checkedAdd`/`checkedSub`/`checkedMul` where the test genuinely wants a value-level `Result`. Applies to the 7 `MathError`-inspecting files: `tests/regressions/basics/strings/string_edge_cases.test.osp{,ml}`, `tests/regressions/basics/types/type_equality_comprehensive.test.osp{,ml}`, `tests/regressions/effects/result_and_effects.test.osp{,ml}`, `examples/tui/api_browser.osp`.
- [ ] **R5 — printed wrappers.** `print(toString(15 / 3))` printing `Success(5.0)`: the expression now prints `5.0`; update the assertion/golden deliberately (see G).
- [ ] **R6 — annotations.** Delete every orphaned `-> Result<int, MathError>` / `Result<float, MathError>` annotation; per the inference rule, do not replace it with `-> int`.
- [ ] **R7 — intentional wraparound.** Sites whose comment or name says wrap/checksum/hash: use `wrapAdd`/`wrapMul` or a wrapping region, whichever reads better; never leave a fault-policy region wrapping by accident.
- [ ] **R8 — `handle ... in` → `handle ... do`** (Phase 1, already done before this checklist starts; re-verify no `in` binder survives in any file this pass touches).

### C. Corpus passes (Default file and its ML twin together; counts are `?:` sites at audit time)

- [ ] `tests/core/` — 3,510 sites. Start with `tests/core/arithmetic/` (all six suites): `effect_policies` absorbs Phase 0's red tests and becomes the flagship `Arith` policy suite; `result_chain_unary_stress` and `boundary_error_stress` are rewritten to pin dispatch semantics (see Risks); `calculator`, `deep_integer_expressions`, `precedence_associativity_stress` convert by R1/R2. Then `tests/core/collections`, `gpu` (kernel lambdas — host backend, R2), and the rest.
- [ ] `tests/regressions/` — 1,702 sites. `basics/osprey_mega_showcase.test.osp{,ml}` first — it is the flagship teaching example and #230's exhibit; its 7 sites convert to one fault-sticky region and the file must read as the advertisement for the new model. Then `basics/`, `effects/` (R3-heavy), `fiber/`, `http/`, `types/`.
- [ ] `tests/effects/` — 654 sites; R3 dominates; verify every `Arith` region nests correctly around existing handlers (innermost-arm-wins is load-bearing here).
- [ ] `tests/flavors/` — 138 sites; these files exist to prove Default/ML equivalence, so twin-parity discipline is the test.
- [ ] `tests/modules/` — 98 sites; watch file-scope bindings: constant initializers fold (R1/const), fallible ones move into handled regions per `[ARITH-EFFECT-CONST]`.
- [ ] `tests/framework/` — 84 sites (`verdict` suites); the testing framework's own arithmetic converts like any other.
- [ ] `tests/workflows/` — 18 sites.
- [ ] `examples/` — 137 sites in 31 files, including `examples/tui/api_browser.osp` (R4), `examples/wasm` (proves the wasm32 dispatch path in a shipped example), `examples/statefulhttp`, `examples/projects/` (multi-file: one policy at each program entry, not per module).
- [ ] `benchmarks/` — 67 sites in 23 files. Hot loops lose per-op `?:`; hash/checksum benchmarks use wrapping regions (R7). Do not hand-edit reported numbers anywhere — Phase 6 re-runs `make bench`.

### F. Reject fixtures (`examples/failscompilation/`)

- [ ] `result_default_on_plain_value.ospo` — still rejects, but its rationale comment describes checked unary negation of a variable as a `Result`, which stops being true; update comment and, if the message's found-type changes, the `.expectedoutput`.
- [ ] The four fixtures whose expected messages embed `Result<int, MathError>` in inferred types — `closure_captures_unbound`, `ffi_capturing_callback`, `function_typed_record_field`, `variance_invariant_arg_mismatch` — get regenerated `.expectedoutput` text (the type becomes `int`). The *defect each pins* must survive; only the type spelling in the message changes.
- [ ] New rejection fixtures (this is the sanctioned place for new files): missing-handler-at-entry for `Arith.overflow`; checked arithmetic inside an `Arith` arm; `resume` in an `Arith` arm; user redeclaration of `Arith`; fallible file-scope initializer; constant-fold overflow. Each with exact `.expectedoutput` text, each with an ML twin where the surface exists.
- [ ] Sweep every remaining `.ospo` for `?:`-on-arithmetic in *setup code* — a fixture must fail for its own reason, not for a stale arithmetic `?:`.

### G. Goldens (`.expectedoutput`)

- [ ] The 6 goldens containing `integer overflow`/`division by zero` and the ≤15 containing `Success(`/`Error(`: reclassify each line as arithmetic-produced (changes: bare value or policy output) or genuine-Result-produced (indexing/HTTP/user — unchanged). Hand-verify every changed line against the new program by running it; never bulk-regenerate, a regenerated golden that merely records new behavior is how #230's class survives.
- [ ] ML twins share the Default golden — zero new `.expectedoutput` files for twins.

### D. Docs, specs, website, tooling

- [ ] Spec snippets: 0013 (rewritten in Phase 6), 0003 (`handle` example at the bindings section, precedence notes for `?:` prose), 0004 (Result Preservation carve-out), 0008/0010/0012/0017/0034/0035 wherever a snippet does arithmetic with `?:` — every Default snippet uses `do`, every snippet type-checks under the new rules (spot-compile them; snippets are code).
- [ ] `docs/messaging.md` snippet accuracy pass (Phase 6 items).
- [ ] Website: prose pages and playground samples that show arithmetic; regenerate `website/src/spec/*.md` via `npm run build`, never by hand (gitignored build output).
- [ ] `vscode-extension/`: tmLanguage keyword lists (`do`), any bundled sample code, `npm test`.
- [ ] `webcompiler/` sample programs, if any embed arithmetic `?:`.
- [ ] README code samples.

### V. Verification (after every pass above, and gating retirement)

- [ ] `make ci` green; clippy auto-fixes taken, none hand-suppressed.
- [ ] `crates/run_test_corpus.sh` byte-exact under the default backend, `--memory=arc` (zero leaks), `--memory=gc`, and `OSPREY_TARGET=wasm32` — the wasm run is what proves fault dispatch never touches the native-only continuation runtime.
- [ ] Phase 0's red tests are green, asserting the *correct* mathematical answers through declared policies.
- [ ] `grep -rn '?: 0' tests examples benchmarks --include='*.osp*'` returns only sites whose scrutinee is a genuine `Result` — each surviving hit individually justified.
- [ ] No `Result<int, MathError>` remains in any `.osp`/`.ospml`/`.ospo`, spec snippet, or diagnostic golden; `MathError` audit resolved (retire or document the survivor).
- [ ] Coverage thresholds in `coverage-thresholds.json` did not go down; raise where the new checker/codegen paths allow.
- [ ] deslop `top-offenders` run over the touched Rust; no new duplication.
- [ ] `make bench` re-run; `website/src/benchmarks.md` regenerated from the fresh tracked `results.json`.
- [ ] Plan retired in `docs/plans/README.md` with named tests as evidence; spec 0037's status header rewritten from normative target to shipped, or its content folded into 0013/0017 and the file retired.
