# Plan 0015 — Generics with Variance and Generic Effects

**Subsystem:** tree-sitter-osprey, crates/osprey-syntax (both flavors), osprey-ast,
osprey-types, osprey-codegen, osprey-lsp
**Status:** The core is implemented and passing tests. Declared type parameters
(fn/type/effect),
declaration-site `in`/`out` variance with position checking and
variance-directed assignability, generic effects with per-site instantiation,
explicit construction-site type arguments, and static proof of the
handler/operation-instantiation seam all work in BOTH flavors. Call-site type
application and the already-documented returned-generic-lambda limitation
remain — see
[§What is left](#what-is-left-detailed).
**Spec:** 0004 §Generics/§Variance ([TYPE-GENERICS-*], [TYPE-VARIANCE-*]),
0017 §Generic Effects ([EFFECTS-GENERIC-*]), 0003 §typeParamList/§effectSet,
0024 [FLAVOR-ML-GENERICS]

## Summary

User-declared generics land in both flavors: explicit type parameters on
functions, variance-annotated (`out`/`in`) type parameters on type and effect
declarations, generic effects (`effect State<T>`), and effect rows carrying
type arguments (`![State<int>]`). Variance is declaration-site (C#/Kotlin
style): `out T` restricts `T` to covariant positions, `in T` to contravariant
positions, and use-site subsumption is variance-directed *assignability* —
plain HM unification is untouched, so principal types survive.

## What works today (file:line evidence)

- `type Box<T>` / ML `type Box T` parse and check end-to-end
  (grammar.js:135-146, ml/parser.rs:230, check.rs `collect_type`).
- HM let-polymorphism: implicit generalization of top-level fns
  (check.rs `check_function`, env.rs `generalize`/`instantiate`).
- The assignability relation `unify_assignable` models the safe one-way
  promotion `T -> Result<T, E>` plus function param-contra/ret-co. The inverse
  `Result<T, E> -> T` is forbidden; declared variance uses this relation.
- Codegen specializes generic fns by inlining (genfn.rs), erases `Type::Var`
  to `i64` (types.rs:19), and effects run on a name-keyed handler stack
  (effects_runtime.c) — fully type-erased.

## Previous gaps

- No syntax: fn type params, effect type params, effect-row type args,
  variance keywords (`ERROR` nodes in both flavors).
- `Stmt::Function`/`Stmt::Effect` had no `type_params`; effect rows were bare
  `Vec<String>`; `Expr::TypeConstructor.type_args` was parsed then discarded.
- `infer_perform` (expr.rs:183) never unified arguments against operation
  parameters and returned the *shared* global op signature — two
  instantiations of one effect could not coexist.
- No variance representation or checking anywhere (grep-verified).

## Chosen design

1. **AST** (`osprey-ast/src/generics.rs`): `Variance`
   {Invariant/Covariant/Contravariant}, `TypeParam { name, variance }`,
   `EffectRef { name, type_args }`. `Stmt::Type.type_params` and new
   `Stmt::Function.type_params`/`Stmt::Effect.type_params` are
   `Vec<TypeParam>`; `Stmt::Function.effects` is `Vec<EffectRef>`;
   `Expr::Perform`/`Expr::Handler` gain `position` (per-site instantiation
   keys, mirroring `Expr::Lambda`).
2. **Surfaces** (identical canonical lowering per [FLAVOR-BOUNDARY]):
   - Default: `type Source<out T>`, `fn map<T, U>(...)`,
     `effect State<T> { ... }`, `![State<int>, Log]`.
   - ML: `type Source out T =`, `map<T, U> : ...` signature binder,
     `effect State T`, `! State<int>`. `out` is contextual; `in` (a hard
     keyword) is accepted inside type-parameter position only.
3. **Checker**:
   - `InferCtx` carries a constructor→variance table; `unify_assignable`
     matches same-name `Con` args variance-directed (co: expected←actual,
     contra: flipped, invariant: plain `unify`), with EXACT unification at
     the leaves. A `Result<T, E>` never coerces to `T`, under a container or at
     a direct value site, because that would erase failure and change the
     representation. Builtins: `Result<out, out>`, `List<out>`, `Fiber<out>`,
     `Map<inv, out>`.
   - Declaration-site position validation walks variant-field and
     effect-op types with a polarity that function parameters flip and
     nested constructors multiply; violations and variance on fn type
     params are errors.
   - Effects are stored generically (`type_params` + raw op sigs) and
     instantiated per handle site and per effect-row entry; a handler-scope
     stack resolves `perform` sites innermost-first (matching the runtime's
     innermost-wins dynamic semantics); `perform` arguments now unify
     against instantiated op parameters.
   - Inference publishes per-position resolved op signatures
     (`ProgramTypes::performs`, `ProgramTypes::handler_ops`).
4. **Codegen**: generic effect ops keep one erased ABI program-wide (every
   type-var-mentioning slot is a boxed `i64`), so the C runtime is untouched.
   Perform sites box erased arguments (bitcast for floats — never `fptosi`)
   and unbox erased results to the site-resolved type; handler arms unbox
   erased params at entry and box erased returns/resumes. Static operation
   summaries retain each resolved instantiation across calls and handlers, so
   a `Stash<string>` handler does not discharge a `Stash<int>` operation and
   compilation fails while the requirement remains at entry. Handlers still
   register and performs still look up under instantiation-mangled keys
   (`Stash$int`); the null-guard is defense in depth, not normal rejection.
   Monomorphic effects keep bare names and identical behavior.
5. **Runtime**: zero C changes (keys are opaque strings).

## Testing

- Expand `tests/regressions/basics/types/pure_hindley_milner_test.{osp,ospml}`
  (fn type params), `type_equality_comprehensive.{osp,ospml}` (variance
  assignability), `effects/algebraic_effects_comprehensive.{osp,ospml}`
  (generic effect, two instantiations, rows with args) — shared goldens,
  byte-identical IR across flavors.
- New must-reject cases in `examples/failscompilation/`: covariant param in
  input position, contravariant param in output position, variance on a fn
  type param, effect type-argument mismatch.
- Unit tests: variance polarity walk, variance-directed assignability,
  effect instantiation, ML parse paths (`ml_coverage.rs`).

## Risks / considerations

- The tree-sitter parser (`src/parser.c`) is checked in and regenerated
  manually (`npm run generate`) — grammar edits are inert until regenerated.
- Cross-flavor Debug-string AST equality compares every new field — both
  lowerers must fill them identically.
- `perform` argument unification tightens checking; latent mismatches in
  examples surface as (genuine) type errors.
- Float payloads crossing erased effect slots must use bitcast boxing
  (`box_to_i64`), never `coerce_to`'s numeric `fptosi`.

## What is left (detailed)

The declared-generics + variance + generic-effects core is done. Three
follow-ups are known-incomplete, each with a concrete failing repro today:

### 1. Call-site type application (turbofish) — `identity<int>(5)`

**State:** unsupported. `identity<int>(5)` parses `int` as a *value*
identifier and errors `unknown identifier int`; the spec only ever shows
`identity<int>` in comments, never as callable syntax. Declaration-site
binders (`fn map<T,U>`) and construction-site args (`Box<int> { … }`) work;
the call-site form does not.

**Why it matters:** the only way to pin an otherwise-unconstrained
polymorphic return today is an annotated `let` (`let x: int = identity(5)`);
turbofish is the direct spelling the docs imply.

**Scope:** grammar (`call_expression` needs a `< typeList >` postfix that
does not collide with `<` comparison — the same GLR/lookahead hazard the
construction-site form already solved), a `type_args` field on `Expr::Call`,
lowering in both flavors, and a checker step that unifies the call's
instantiation variables against the written arguments (reuse
`current_fn_typarams` threading from construction sites). ML spelling TBD
(angle-bracket `f<int>(x)` vs a signature-only story).

### 2. Generic functions as first-class values — ✅ landed (plan 0002)

**State (2026-07): works.** `let g = identity` binds as a call alias; a
generic function flowing into a concrete function-typed slot specialises to
the slot's ABI (emitted like a capture-free lambda); lambda arguments to
generic HOFs dispatch indirectly (`fn also(x, f) = f(x)` applied at two
instantiations works). The enabling checker fix: builtin scheme binder ids
(`Var(0)`/`Var(1)`) no longer collide with live inference variables
(`RESERVED_SCHEME_VARS`), which had been silently blocking let-generalization
of `-> T`-annotated functions. Plan 0002 still rejects a generic lambda returned
from a generic function.

### 3. Static proof of the handler/operation instantiation seam — ✅ landed

**State:** statically enforced. `crates/osprey-types/src/effect_rows.rs`
computes a closed-program summary of the operations required by each function
and callback. Every requirement includes its resolved generic arguments. Calls
propagate those requirements, and a handler removes only an operation arm with
the same effect name and instantiation. A `Stash<string>` handler therefore
cannot discharge `Stash<int>.take`; the remaining requirement is a compile
error at program entry. The runtime's instantiation-mangled null guard remains
a defensive backstop.

Explicit rows such as `!Stash<int>` are checked contracts and instantiation
hints, not handlers. They constrain operations in a function body but do not
grant authority when that function is called. Unannotated functions infer the
same requirements without needless return or effect annotations.

**Representation boundary:** this safety pass does not add a general open-row
variable to `Type::Fun`; it uses fixed-point operation summaries for the
current closed-program language surface. If future syntax exposes independently
quantified row variables in public higher-order types, that representation and
its HM generalization rules remain separate work in
[plan 0016](0016-algebraic-effects-and-handlers.md).

### 4. Unspecified extensions (no work planned)

- **Bounded polymorphism** (`fn f<T: Ord>`): the spec mentions an implicit
  `Iterable` constraint once ([0004] §Collection Types) but defines no
  constraint syntax; `TypeParam` has no `bounds` field. Out of scope until a
  concrete use case is specified.
- **Higher-kinded type parameters** (`F<_>`): not represented, not planned.

## Conflicts found while pinning the spec (2026-09-09)

Four disagreements surfaced when the generics surface was written down as
executable assertions. Each is recorded with the evidence that settles it.

1. **Variance was inert at assignment sites, and the spec implied otherwise.**
   `[TYPE-VARIANCE-ASSIGN]` opened with a directional rule and closed with
   "bottoms out in exact unification". `unify_variant_arg`
   (`crates/osprey-types/src/unify.rs`) recurses only through **same-name**
   variance-declared constructors and then calls plain `unify`, so `int` versus
   `Result<int, MathError>` is an exact mismatch at any depth. The one coercion
   in the language (`T -> Result<T, E>`) is representation-changing and applies
   at direct value sites only. **Resolution:** exact leaves win; spec 0004 now
   says so in `[TYPE-VARIANCE-COERCION]`, and states the consequence outright —
   `out`, `in` and unannotated accept and refuse the same programs today, so a
   marker's observable effect is position checking alone. Pinned by
   `the_three_markers_agree_on_every_assignment_outcome` and
   `the_covariant_and_invariant_builtins_agree`, which go red the day a
   representation-preserving subtype relation lands without a spec update.
   The two shipped fixtures could not see this: both assert the one direction
   all three markers already agree on.

2. **`Map<K, out V>`'s "(keys invariant)" describes an uninstantiable
   parameter.** The shipped map surface fixes keys to `string`
   ([BUILTIN-MAP-GET], spec 0012), so no program can write `Map<int, int>` and
   the key's variance is unobservable. Spec 0004's built-in table and spec 0012
   disagree about whether `K` exists.

3. **The dynamic-instantiation diagnostic contradicts the runtime it cites.**
   `examples/failscompilation/stage_signal_instantiated_dynamic_effect.ospo`
   rejects `perform Signal<Count>` on a dynamic effect because "a dynamic
   handler is keyed by effect name at runtime, so instantiations share one
   key" — but `crates/osprey-codegen/src/effects.rs` registers each handler
   "under its instantiation-mangled key, so only same-instantiation performs
   resolve to it", which is also what spec 0017 `[EFFECTS-GENERIC-RUNTIME]`
   promises. The REJECTION is a shipped contract and stands; its stated REASON
   is stale and would mislead anyone implementing against it. The surface rule
   is now pinned as: `handle`/`perform` infer a dynamic effect's instantiation,
   angles are legal on a ROW (`!Stash<int>`), and the written mention belongs
   to `static effect`, whose identity IS the instantiation.

   **Replacement diagnostic** (same rule, true reason — the fixture golden is
   updated in the same commit that emits it):

   > ``perform Signal<Count>`` names an instantiation of dynamic effect
   > ``Signal``; a written instantiation is the identity of a ``static effect``,
   > while a dynamic effect takes its instantiation from inference
   > (docs/plans/0024-staged-effects.md). Declare ``static effect Signal``, or
   > write ``perform Signal`` and let inference instantiate it

4. **Spec 0017 writes `handle … do`; the language accepts `in`.** Intended:
   the Default flavor is moving to `do` ([plan 0027](0027-arithmetic-effects.md)
   phase 0). `the_spec_writes_a_handled_body_after_do` is a deliberate red pin
   that turns green when that rename lands.

## TODO

Core (done):

- [x] AST: `Variance`/`TypeParam`/`EffectRef` + new fields
- [x] Default grammar + parser regen + lowering
- [x] ML lexer/parser/CST/lowering (incl. `Box<int>(item = …)` ctor args)
- [x] Checker: variance table, position checks (incl. effect ops), generic
      effects, per-site publishing, ctor type-arg resolution against the
      enclosing fn binder, arity errors
- [x] Codegen: erased-slot box/unbox at perform/handler boundaries;
      instantiation-mangled runtime keys + null-guard diagnostic;
      `has_type_var`-based erasure (nested `Result<T,…>` slots); Result-slot
      resume boxing
- [x] LSP symbol/hover rendering of type params
- [x] Examples expanded in both flavors + 7 failscompilation cases
- [x] Specs 0002/0003/0004/0017/0023/0024 updated
- [x] make ci green

Remaining:

- [ ] **Call-site type application** `identity<int>(5)` — grammar +
      `Expr::Call.type_args` + both-flavor lowering + checker unification
      (§What-is-left 1). **Specified and pinned red first**: the contract is
      [spec 0004](../specs/0004-TypeSystem.md) `[TYPE-GENERICS-APPLY]` and
      [spec 0024](../specs/0024-MLFlavorSyntax.md) `[FLAVOR-ML-GENERICS]`; the
      failing assertions are `crates/osprey-types/src/generics_apply_tests.rs`
      (Default), `generics_apply_ml_tests.rs` (ML), four CST cases in
      `tree-sitter-osprey/test/corpus/osprey.txt`, and the corpus twins
      `tests/regressions/basics/types/type_equality_comprehensive.test.osp{,ml}`
      with the golden line `applied int=5 text=os nested=2 empty=0 pair=7`.
- [x] **Generic functions as values** — landed via plan 0002, now retired; the
      shipped contract is [spec 0004](../specs/0004-TypeSystem.md) `[TYPE-GENERICS-FN]`:
      slot-driven specialization + let-alias + inline fn-typed arg
      registration; `let g = identity` passed to a HOF compiles and runs
      (§What-is-left 2). Only the returned still-generic lambda remains, in
      plan 0002.
- [x] **Static handler/operation seam** — resolved operation summaries make an
      instantiation mismatch a compile error; explicit rows are contracts, and
      the runtime null guard is only a backstop (§What-is-left 3 and
      [plan 0016](0016-algebraic-effects-and-handlers.md)).
- [ ] failscompilation cases for turbofish once it lands — already written and
      red: `turbofish_type_arg_arity`, `turbofish_type_arg_too_few`,
      `turbofish_no_declared_binder`, `turbofish_argument_contradiction`,
      `turbofish_variance_marker`, `ml_turbofish_type_arg_arity` and
      `ml_turbofish_no_declared_binder` in `examples/failscompilation/`.
- [ ] The adjacent spec surface is pinned by the same sweep and may expose
      defects of its own: `generics_decl_tests.rs`
      ([TYPE-GENERICS-DECL], [GENERICS-CTOR-ARITY], [TYPE-GENERICS-FN]),
      `generics_variance_tests.rs` ([TYPE-VARIANCE-*], including the built-in
      variance table checked through position composition) and
      `generic_effects_tests.rs` ([EFFECTS-GENERIC-*], plus the
      `handle … do` spelling spec 0017 writes and the language does not accept —
      [plan 0027](0027-arithmetic-effects.md) phase 0 owns that rename).

## Frozen-test conflicts (2026-09-09, second sweep)

The assertion sweep above was then frozen: no agent may edit, flip or reformat
any test in this tree, so the following are recorded rather than repaired. Each
names the side that is **right**, so whoever lifts the freeze knows which half
to change. Rulings are OspreyAstra1's.

1. **The compiler is right; the test is wrong — HM generalization.**
   `fn empty<T>() -> List<T> = []` then `let held = empty()` and
   `length(held)` compiles and prints `0`, and so does a bare `let xs = []`.
   An unconstrained binder at an immutable `let` generalizes; nothing observes
   `T`'s identity, so there is nothing to reject. Written type arguments *pin*
   an instantiation, they are never *required*.
   `generics_apply_tests.rs::the_unpinnable_binder_is_the_control_for_that_case`
   demands a rejection and must become an acceptance.

2. **The compiler is right; the test is wrong — ML tuple heads are flat.**
   `pick<T, U> : (T, U) -> T` with `pick (first, second) = first` declares a
   flat two-parameter head, so `pick<int, string> 1 "two"` is not a curried
   application of it. Now stated outright in
   [spec 0024](../specs/0024-MLFlavorSyntax.md) `[FLAVOR-ML-CURRY]` with both
   the accepted and the rejected spelling.
   `generics_apply_ml_tests.rs::ml_type_application_survives_curried_application`
   must call `pick<int, string> (1, "two")`.

3. **The compiler is right; the test is wrong — dynamic effects infer their
   instantiation.** A written instantiation *is* the identity, so only a
   `static effect` may carry one at a `perform` or a `handle`; a row may pin in
   either stage. That a runtime key happens to be mangled per instantiation
   does not license the written form. Now stated with accepted/rejected
   examples in [spec 0035](../specs/0035-StagedEffects.md)
   `[STAGE-SIGNALS-EXACT]`.
   `generic_effects_tests.rs::a_bracketed_row_carries_several_generic_entries`
   writes `handle Read<int>` on a dynamic effect and must drop the arguments.

4. **The compiler is right; the test is wrong — handler identity is exact.**
   A `Stash<string>` handler does not discharge a `Stash<int>` request, so
   `unhandled effect operations` is the truthful diagnostic; effect names
   matching is not effect identities matching.
   `generic_effects_tests.rs::a_handler_arm_disagreeing_with_the_body_is_rejected`
   expects `cannot unify` and must instead put the disagreeing arm on a
   *direct* `perform` under the handler, where the arm and the operation's
   return really do meet.

5. **The compiler is right; the corpus program is wrong — double flip is an
   output position.** `type Sink<in T>` carries
   `hof: ((T) -> int) -> int`. A field is read out of the record (+), the
   outer function's argument flips it (−) and the inner function's argument
   flips it back (+), so `T` lands in output position and `in T` forbids it.
   `type_equality_comprehensive.test.osp` line 124 must give `hof` a shape
   that keeps `T` negative.

6. **Three ML fixtures never reach the ML parser.**
   `ml_turbofish_type_arg_arity.ospo`, `ml_turbofish_no_declared_binder.ospo`
   and `ml_variance_covariant_no_inward_coercion.ospo` lack the
   `// osprey: flavor=ml` marker that every other `ml_*.ospo` carries. The
   `.ospo` extension resolves to Default ([`resolve_flavor`]), so they are
   parsed as Default and emit a page of syntax errors instead of the
   diagnostic they assert. The `ml_` prefix is a naming convention only — it
   selects nothing.

7. **The stage diagnostic's phase is asserted twice, incompatibly.**
   `crates/osprey-syntax/src/lib.rs`'s
   `an_instantiated_dynamic_effect_is_rejected_rather_than_shared` requires the
   rejection in `parse_program_with_flavor(...).errors`, while
   `generic_effects_tests.rs::a_written_instantiation_on_a_dynamic_perform_is_rejected`
   requires it from the checker. Both cannot hold. Until the freeze lifts the
   diagnostic stays where it is — a stage rule reading as a `SyntaxError` is a
   layering wart, but moving it changes a published phase contract.

8. **A stale golden.** `stage_signal_instantiated_dynamic_effect.ospo.expectedoutput`
   still carries the superseded "keyed by effect name at runtime, so
   instantiations share one key" rationale; `crates/osprey-ast/src/stage.rs`
   already emits the replacement prose. The golden is the stale side.

9. **The compiler is right; the ML twin is wrong — ML has no brace expression.**
   `type_equality_comprehensive.test.ospml` line 233 writes a Default-flavor
   map literal, `identityOf<Map<string, List<int>>> { "a": [1], "b": [2] }`.
   ML builds a map with `[k => v]`; `{` lexes only so a structural PATTERN can
   spell `{ heading, .. }` and has no expression form at all — the rule
   `examples/failscompilation/ml_brace_record_and_question_sigil.ospo` exists to
   pin. The line must become `[ "a" => [1], "b" => [2] ]` — verified: with that
   spelling the whole application parses, type-checks and runs, so the triple
   `>` and the call-site type arguments are not implicated.

Fixed in this sweep rather than recorded, because no test asserted the broken
behaviour: a glued `<` after a name committed to call-site type arguments with
no lookahead, so `x<3` and `x<y` stopped parsing as comparisons. The commit is
now gated on the whole shape — a balanced angle run of type tokens followed by
the argument the application applies to — mirroring `at_generic_record`.
`send` likewise stopped being a hard keyword in the three positions that hold an
effect operation name ([FLAVOR-ML-EFFECT-OP-NAME]).

**The same regression is still live in the Default flavor** and is the one
outstanding defect this sweep found in shipped behaviour: `fn lit(x) = x<3` is a
syntax error, while the spaced `x < 3` compiles. The fix belongs in
`tree-sitter-osprey/grammar.js`, and it is the rule the ML side now follows — a
glued `<` may not commit to type arguments on its own; the whole shape must be
there, a balanced angle run of type-only tokens followed by the argument the
application applies to.

Two failures in this sweep are **not** generics work:

- `recursive_generic_needs_annotation.ospo` is now *accepted*, so 1 of 144
  must-reject programs compiles. Ruled correct: a recursive generic's return is
  now specialized from its arguments, so the program really is well-formed and
  the **fixture** is the stale side — it must move to the corpus or be replaced
  by a program that still needs the annotation.
- `cli_e2e`'s `llvm_reports_a_codegen_error` and `run_reports_a_codegen_error`
  no longer reach codegen: `GENERIC_AS_VALUE` is caught by the checker, so their
  `stderr` never says `codegen` and **no program exercises the CLI's
  codegen-error path**. That path needs a program that still fails there.
