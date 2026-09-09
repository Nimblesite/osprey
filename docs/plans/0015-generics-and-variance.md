# Plan 0015 — Generics with Variance and Generic Effects

**Subsystem:** tree-sitter-osprey, crates/osprey-syntax (both flavors), osprey-ast,
osprey-types, osprey-codegen, osprey-lsp
**Status:** The generics implementation is present in both flavors, including
explicit call-site type arguments, declaration variance, generic effects,
handler inference through helpers and aliases, and specialization metadata for
fibers and nested records. Validation is not green: the completed field-call and generic callback fixes
pass the focused integration matrices, but frozen fixtures conflict
with the settled specification, and pinned Deslop 0.27.0 reports 6.6%
duplication against a 5% gate. Native Windows validation has no local Windows
runner. No CI gate or test is waived.
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
  `Result<T, E> -> T` is forbidden. Constructor variance has exact leaves;
  it never lifts the representation-changing Result promotion through a container.
- Codegen specializes generic fns by inlining (genfn.rs), erases `Type::Var`
  to `i64` (types.rs), and generic effects use instantiation-mangled runtime
  keys. Resolved call substitutions specialize body metadata before lowering.

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

## Follow-up implementation and boundaries

The declared-generics, variance, generic-effects, and explicit-application
surfaces are implemented. The following records their final behavior and limits:

### 1. Call-site type application (turbofish) — `identity<int>(5)`

**State:** implemented. Default writes `identity<int>(5)`; ML writes
`identity<int> 5`. The angle run is recognized only when its full type-shaped
contents close before a valid call argument, preserving comparisons such as
`x<3` and `x<y`. Both frontends lower to `Expr::TypeApply` around the existing
call's callee. Declaration binders and signature variables share one fresh
substitution. Arity, nested type arity, unknown types, binder shadowing, and
argument contradictions are checked. Applying arguments to a bound type
variable, such as `T<int>`, is rejected before conversion can discard them.

`applications.rs` carries implicit and explicit call substitutions across the
backend AST copy. Alias origins compose into that substitution, and generic
body metadata for effect operations, handlers, lists, lambdas, and nested
records specializes at each call. Source positions remain available for
compiler diagnostics. An unconstrained immutable `let` can still generalize;
explicit type arguments are a way to pin its type, never a requirement.

### 2. Generic functions as first-class values — ✅ landed (plan 0002)

**State (2026-07): works.** `let g = identity` binds as a call alias; a
generic function flowing into a concrete function-typed slot specialises to
the slot's ABI (emitted like a capture-free lambda); lambda arguments to
generic HOFs dispatch indirectly (`fn also(x, f) = f(x)` applied at two
instantiations works). The enabling checker fix: builtin scheme binder ids
(`Var(0)`/`Var(1)`) no longer collide with live inference variables
(`RESERVED_SCHEME_VARS`), which had been silently blocking let-generalization
of `-> T`-annotated functions. A returned generic lambda can be bound and specialized when called. A still-generic
closure used as a bare value without a concrete consuming slot is rejected.

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

2. **Map type notation and constructible values are distinct.** The type
   system retains `Map<K, out V>` with invariant `K`; shipped constructors,
   literals, lookup, and updates constrain `K` to `string` ([TYPE-MAP],
   [BUILTIN-MAP-GET]). A type argument can name `Map<int, V>`, but no shipped
   map constructor builds such a value. The invariant key parameter does not
   imply support for integer-keyed maps.

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

   **Replacement diagnostic** (same rule, true reason; the frozen fixture
   golden remains stale and is a validation blocker):

   > ``perform Signal<Count>`` names an instantiation of dynamic effect
   > ``Signal``; a written instantiation is the identity of a ``static effect``,
   > while a dynamic effect takes its instantiation from inference
   > (docs/plans/0024-staged-effects.md). Declare ``static effect Signal``, or
   > write ``perform Signal`` and let inference instantiate it

4. **Handler body spelling is explicit.** Default accepts `handle ... in`
   and the compatibility spelling `handle ... do`. ML accepts `in`. The
   `the_spec_writes_a_handled_body_after_do` assertion now passes.

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
- [ ] make ci green — blocked; see validation and frozen-test conflicts

Follow-ups:

- [x] **Call-site type application** `identity<int>(5)` — grammar +
      `Expr::TypeApply` + both-flavor lowering + checker unification
      (§Follow-up 1). The contract is
      [spec 0004](../specs/0004-TypeSystem.md) `[TYPE-GENERICS-APPLY]` and
      [spec 0024](../specs/0024-MLFlavorSyntax.md) `[FLAVOR-ML-GENERICS]`; the
      assertions are `crates/osprey-types/src/generics_apply_tests.rs`
      (Default), `generics_apply_ml_tests.rs` (ML), four CST cases in
      `tree-sitter-osprey/test/corpus/osprey.txt`, and the corpus twins
      `tests/regressions/basics/types/type_equality_comprehensive.test.osp{,ml}`
      with the golden line `applied int=5 text=os nested=2 empty=0 pair=7`.
- [x] **Generic functions as values** — landed via plan 0002, now retired; the
      shipped contract is [spec 0004](../specs/0004-TypeSystem.md) `[TYPE-GENERICS-FN]`:
      slot-driven specialization + let-alias + inline fn-typed arg
      registration; `let g = identity` passed to a HOF compiles and runs
      (§Follow-up 2). Bare generic closure values still require a concrete
      consuming slot.
- [x] **Static handler/operation seam** — resolved operation summaries make an
      instantiation mismatch a compile error; explicit rows are contracts, and
      the runtime null guard is only a backstop (§Follow-up 3 and
      [plan 0016](0016-algebraic-effects-and-handlers.md)).
- [x] Explicit-application rejection checks are implemented. Frozen cases include: `turbofish_type_arg_arity`, `turbofish_type_arg_too_few`,
      `turbofish_no_declared_binder`, `turbofish_argument_contradiction`,
      `turbofish_variance_marker`, `ml_turbofish_type_arg_arity` and
      `ml_turbofish_no_declared_binder` in `examples/failscompilation/`.
- [x] The adjacent declaration and variance surface is implemented and checked: `generics_decl_tests.rs`
      ([TYPE-GENERICS-DECL], [GENERICS-CTOR-ARITY], [TYPE-GENERICS-FN]),
      `generics_variance_tests.rs` ([TYPE-VARIANCE-*], including the built-in
      variance table checked through position composition) and
      `generic_effects_tests.rs` ([EFFECTS-GENERIC-*], plus the
      the accepted Default `handle … do` compatibility spelling).

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

The Default comparison regression is also fixed. Its external scanner checks
balanced, type-shaped arguments and the following call delimiter before
committing to the angle application. `x<3`, `x<y`, and `identity<int>(5)`
compile together. The frozen nested-application CST expectation still has
one mismatched closing parenthesis; parser behavior is correct.

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

## Final review and validation (2026-09-09)

The implementation review corrected call-site specialization through aliases,
generic effects under helper-installed handlers, metadata for nested generic
records, and Default glued comparisons. A mixed executable probe combines
covariant records, a contravariant effect, polymorphic helpers, inferred and
written applications, fibers, dynamic handlers, resume, Results, lists, maps,
closures, and aliases. Its Default and ML forms both pass under default, GC,
and ARC memory management. Temporary probes live outside the repository; no
frozen assertions were weakened to obtain these results.

The warning review found a real false positive on the inbox's
`Markdown::smaller`. One written ML signature had been copied onto several
canonical slots, and erasing one copy left the others supplying the constraint.
Lowering now preserves their shared source position; the detector erases all
copies together. The actual inbox project produces no `smaller` warnings.
Curried types retain their curried shape in diagnostics; `(int) -> (int) -> int`
and `(int, int) -> int` are different types. Existing Default annotations stay
separate source annotations. The warning pass chooses one jointly removable set
for the entire assembled program before filtering by open file. Per-file
selection cannot prove that annotations in different files are removable
together. The latest local LSP library run took 44.32 seconds; the earlier
5.95-second result used the now-replaced per-file selection shortcut.

The following table records the earlier CI snapshot. The completed instrumented
workspace rerun and corpus matrix are recorded after the integration notes below.

| Check | Result |
| --- | --- |
| Full instrumented workspace run | 1515 passed, 21 failed across 7 targets |
| Release CLI build | Passed |
| Production Clippy (`--workspace --lib --bins -- -D warnings`) | Passed |
| `make hawk` | Passed, zero findings |
| Native Default corpus | 209 passed, 2 frozen invalid fixtures failed |
| Native ARC corpus | 209 passed, same 2 failed; zero reported leaks |
| Wasm corpus | 144 passed, same 2 failed; 65 pre-existing declared capability exclusions |
| Alternative GPU lowering | 18 passed in each corpus run |
| Type checker unit tests | 511 passed, 7 frozen expectations failed |
| LSP unit tests | 159 passed, 6 frozen expectations failed |
| Codegen unit tests | 124 passed |
| Tree-sitter corpus | 17 passed, 1 malformed frozen CST expectation failed |
| Bank native suites | 3 passed |
| Mobile inbox domain tests | 32 passed |
| Formatting | Passed on current working tree; pre-existing test formatting edits preserved |
| All-target Clippy | Blocked by five lints in frozen tests/test helpers |
| `make ci` | Pinned Deslop 0.27.0 runs and rejects approximately 6.6% duplication against the unchanged 5% ceiling |

The seven type-checker failures comprise the five semantic/phase conflicts
listed above and two warning fixtures that flatten a curried type in their
expected text. The six LSP failures expect no diagnostics for programs with
redundant written annotations; the returned diagnostics are Warnings, not
syntax/type errors. The tests remain validation blockers. The final workspace
run and rejection goldens must not be described as green, and no PR may be
submitted while `make ci` fails.

### CI preparation follow-through (2026-09-09)

The delegated audit found and corrected additional production defects: explicit
constructor and effect-row arguments now use the same validation as function
applications; rows reject wrong written arity even when their bodies perform no
operations. Parameters, returns, local bindings, and lambda annotations reject
applications of an enclosing type variable such as `T<int>`. Local and lambda
annotations resolve the enclosing function's declared binders. The native
mixed-type local/lambda probe prints `7 ok`.

ML standalone signatures and inline parameter annotations both constrain the
checker. A disagreement is rejected. A whole signature retains its curried
shape in warning text, and headers declaring generic binders or effect rows are
conservatively retained because type-only erasure does not model deleting those
declarations. LSP analysis traverses explicit type applications while recovering
from invalid callees, preserving hover information inside those expressions.

The delegated local CI runs additionally establish:

| Job or gate | Result |
| --- | --- |
| Branch protection | Two active rulesets; all 14 required contexts match |
| VS Code extension | 316 tests passed; all four coverage metrics exceed 95% |
| Extension lint, manifest, VSIX packaging | Passed |
| Website Chromium E2E | 107 passed, including both Wasm flavors and smoke hosts |
| Bank Chromium E2E | 17 passed against rebuilt compiler |
| Docker web compiler | Image built; actual container API assertion passed |
| C runtime | 24 suites passed; 29 library coverage gates passed |
| iOS | Device/simulator builds, ABI, Counter and Inbox smoke passed; corpus 128/130 with the same two frozen invalid fixtures |
| Android | App build, ABI/domain tests, smoke and Gradle lint passed; corpus 128/130 with the same two frozen invalid fixtures |
| Additional checks | Runtime incremental build, profiler, benchmark tooling, and node dependency guard passed |
| Windows | Not executable on this host: native Windows/MSYS2 UCRT64 runner or VM is absent |

The exact CI Rust command fails on frozen CLI diagnostic expectations. The earlier
instrumented run with `--no-fail-fast` exposed seven failed targets; it is
not a green coverage run. Existing Rust coverage report thresholds passing is
not evidence that this failed run completed successfully.

The independent pinned duplication audit classifies 3,357 counted lines as
frozen tests and 2,510 as production. This corrects an earlier classification
that missed attributes between `#[cfg(test)]` and test modules. Even the
unachievable optimistic case of removing all 919 strong production clone lines
from the numerator alone leaves 5.574%. The reviewed production consolidation
families do not establish safe sufficient savings. The gate remains unchanged;
no tests, exclusions, or thresholds were modified to obtain a pass. Exact spans,
overlap accounting, candidate remedies, and validation are recorded in
`/tmp/osprey-final-deslop-findings.md`.

### Final field, callback, and fiber integration

Generic field constraints now retain the relationship between a receiver and
its field through HM generalization, aliases, and call-site instantiation.
Hidden callback variables travel with that relation. The checker rejects
wrong callback result types and nonrecord receivers before code generation.

Dotted calls preserve field precedence until the receiver is resolved. A
single generic helper can select a record field at one call and a free
function at another, including distinct result types. Explicit type arguments
apply only to the selected callable's declared binders. Named UFCS arguments
retain the implicit receiver. Effect summaries defer the same choice and
preserve callback effects, returned callables, handler exclusions, and exact
generic effect identity. Unknown provenance is not proof that a field is
absent. The warning oracle compares generalized constraints and dispatch
choices as well as the previously published types.

Closure parameters, captured callbacks, and returned function values carry
full semantic types across their ABI slots. This fixes a float getter that
printed integer bits, an invoked callback emitted as an unresolved free
function, and generic effectful functions stored in fields. Inlined list
literals are materialized before crossing a fiber result boundary, fixing a
segfault when awaiting a boxed generic fiber returning nested lists.

The final focused evidence includes 47 dispatch/corpus cases, 58 check/LLVM
outcomes across 29 effect/dispatch probes, 39 additional runtime checks and
five rejection probes, and the 36-case fiber matrix. These are overlapping
validation sets, not a count of distinct regression tests. The stable release
SHA256 is `2807bb3a92e19423737d7a5313e1330261f2ed568ef98a15134a545823e8f46e`.
Existing codegen tests pass 124/124 and effect tests pass 93/93. No repository
tests were changed by this delegation.

Both large type-equality twins pass all 33 assertions and match their existing
golden after the malformed inputs are repaired only in temporary copies.
Their helper contracts also differ: Default annotates `keepSource` and
`keepSink` concretely, while ML generalizes them. Removing those two Default
annotations in the temporary copy preserves generic coverage and yields
byte-identical IR. This is a further frozen fixture correction, recorded in
`/tmp/osprey-ci-frozen-corrections.md`; the repository copies remain unchanged.

The user requested deleting this plan **once done**. Its CI acceptance condition
is still unmet, so the plan remains until the frozen-test conflicts, duplication
gate, and Windows validation are resolved. No PR has been opened.

### Completed verification after backend integration

The full instrumented Rust workspace run reports **1,514 passed and 22 failed
across eight targets** (`/tmp/osprey-ci-final-rust.log`). The additional failure
is the frozen Default AST expectation that rewrites `o.m(1)` before receiver
typing; the implementation now correctly preserves `MethodCall`. This failed
run is not a green coverage result.

The sequential corpus matrix completed with compiler SHA256
`73137670037536be5137972efa68916ebf6b234b6c29f8ebf8c4d221184af0f7`:

| Target | Result |
| --- | --- |
| Native assertions and stdout goldens | 209 passed, the same two frozen invalid twins failed |
| GC stdout goldens | 209 passed, the same two failed |
| ARC stdout goldens | 209 passed, the same two failed; zero reported leaks |
| Wasm stdout goldens | 144 passed, the same two failed; 65 existing declared capability exclusions |
| Alternative GPU lowering | 18 passed in each golden matrix |

Exact commands, logs, and the stable compiler hash are recorded in
`/tmp/osprey-ci-final-corpora.json`. These runs precede the final checker review
of builtin shadowing and callback-return effect provenance; they are evidence
for the integrated backend, not a claim that every subsequent checker revision
has passed a complete CI run.

The proposed frozen-fixture correction set is concrete and unapplied at
`/tmp/osprey-frozen-corrections.patch`. Its applicability check passes, and all
769 protected file hashes match the test-freeze baseline. Applying it still
requires an explicit exception to the user's test freeze. Production formatting
and Clippy pass; all-target Clippy still reports five frozen test/helper lints.

### Final effect-provenance review

The review additionally fixed three ways effect information could be lost:
builtin result shapes applied to shadowing user bindings; a callback application
was represented by its callee rather than its returned value; and a failed field
projection retained a callee parameter index that could later resolve through an
unrelated caller argument. Supplied opaque values now stay unknown; only truly
absent arguments retain symbolic binders. Generic field calls cannot use an
unrelated pure callback to discharge an effectful field.

Callback return projections and higher-order invocations retain positional and
named argument values. Scope shifting, substitution, fixed-point widening, and
handler exclusion traverse that retained provenance. Named and curried calls
therefore preserve the effects of callbacks actually invoked by the callee.
Active handler return values are keyed by operation and complete generic effect
instantiation. Closures capture values without capturing handler bindings, so
escaping a handler does not erase a returned callable's remaining requirements.

On frozen release SHA256
`8ef9a57c7d23af2e7a472861d7110ecf6e235e47364e1e5d7e17037cd03a2dc6`,
the existing effect suite passes **93/93**, the final checker/LLVM matrix passes
**142/142** decisions, and ten accepted cases pass **30/30** native runs across
default, GC and ARC with exact expected output. Cases include `Factory<T>`
returning a record, exact and wrong `Probe<T>` handlers, escaped captured
records, lexical shadowing, ignored/invoked callbacks, and named/curried calls.
The matrix overlaps earlier focused checks; it is not a count of new repository
tests. Evidence is in `/tmp/osprey-failed-projection-report.md`.

The final full instrumented workspace run against this source completed with
**1,514 passed and the same 22 failed across eight targets**
(`/tmp/osprey-ci-verified-rust.log`). Totals exclude a nested one-test subprocess
already counted by its parent suite; earlier totals that included it were one
too high. Codegen passes 124/124, LSP reports 159 passed and six frozen failures,
and types reports 511 passed and seven frozen failures. The run is not a green
coverage result.

Final formatting, production workspace Clippy, and the Hawk dead-code gate pass.
The assembled inbox checks successfully with 175 statements and no
`Markdown::smaller` warnings; its one remaining warning names the whole curried
`Text::boundary` signature. The pinned `make ci` run still fails at **6.6%
duplication (5,867 / 88,916 LOC)** against the unchanged 5% ceiling. The logs are
`/tmp/osprey-ci-verified-{format,production-clippy,hawk,inbox,gate}.log`.

The correction patch still passes `git apply --check`, all 769 protected files
remain unchanged, and the user has not authorized an exception to the test
freeze. No native Windows runner is available. These blockers keep the CI
checkbox open; the plan is retained and no PR has been created.
