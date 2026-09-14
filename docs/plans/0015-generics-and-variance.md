# Plan 0015 — Generics with Variance and Generic Effects

**Subsystem:** tree-sitter-osprey, crates/osprey-syntax (both flavors), osprey-ast,
osprey-types, osprey-codegen, osprey-lsp
**Status:** Implemented and audited in both flavors. Callback provenance through Result defaulting, symbolic record fields and indirect named calls, returned-callback metadata, generic alias argument ordering, direct named lambdas and extern-call ordering are verified. All 129 runtime follow-up checks pass across default, GC and ARC. Local CI, mobile, Wasm and browser validation pass; exact osprey-types coverage is 10028/10231 lines (98.0%) against 98%, and pinned Deslop 0.27.0 reports 4.2% against 5%. No known implementation gap remains within this plan's defined scope. Hosted acceptance of the final submitted revision remains open; no gate is waived.
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

## Implemented behavior

- `type Box<T>` / ML `type Box T` parse and check end-to-end
  (`grammar.js` `type_declaration`, `ml/parser.rs` `type_decl_after_keyword`,
  `check.rs` `collect_type`).
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
- `infer_perform` (`expr.rs`) never unified arguments against operation
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

   **Replacement diagnostic** (same rule, accurate reason, now asserted by the corrected golden):

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
- [x] Complete local CI, coverage, duplication, Mac/Linux native, Wasm, mobile and browser validation — see current validation below
- [ ] Verify every hosted required check on the final submitted revision

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
- [x] Explicit-application rejection checks are implemented. Cases include: `turbofish_type_arg_arity`, `turbofish_type_arg_too_few`,
      `turbofish_no_declared_binder`, `turbofish_argument_contradiction`,
      `turbofish_variance_marker`, `ml_turbofish_type_arg_arity` and
      `ml_turbofish_no_declared_binder` in `examples/failscompilation/`.
- [x] The adjacent declaration and variance surface is implemented and checked: `generics_decl_tests.rs`
      ([TYPE-GENERICS-DECL], [GENERICS-CTOR-ARITY], [TYPE-GENERICS-FN]),
      `generics_variance_tests.rs` ([TYPE-VARIANCE-*], including the built-in
      variance table checked through position composition) and
      `generic_effects_tests.rs` ([EFFECTS-GENERIC-*], plus the
      accepted Default `handle … do` compatibility spelling).

## Current validation (2026-09-10)

Existing assertions were corrected individually where the specification and executable evidence showed the old expectation was wrong. Diagnostic checks now assert exact warnings, messages, ranges and owners; curried signatures retain their curried types. The inbox's `Markdown::smaller` produces no redundant-annotation warning. Parser-stage rejection tests assert the parser's actual rejection. No assertions were removed to obtain a passing test.

The two former rejection fixtures for a channel inside a generic field and recursive generic specialization now run as positive corpus regressions. Their original programs and outputs are retained, with additional assertions for nested payload contents and specializations. The type-equality twins use matching generic contracts and flavor-correct map literals. Storage twins now use the same tupled contract and filename-generating expression, retaining all seven assertion blocks and independent runtime files. Cross-flavor IR comparison passes.

The deduplication audit preserves all 201 generics, variance and generic-effect tests, including their assertion expressions and literal expectations. The original `DebugBuild` assertions are retained; the CLI uses its build-kind mapping, and both the debugger and CLI regression tests pass.

| Check | Completed result |
| --- | --- |
| Local pipeline | `make ci` passes, including native integration, bank browser tests and shared mobile domain tests |
| Instrumented Rust workspace | 1,563 tests pass; osprey-types has 543 tests |
| Exact Rust coverage | All nine configured crate gates pass on Mac and Linux; osprey-types is 10028/10231 lines, above the unchanged 98% threshold |
| Effect transport regressions | All 17 focused tests pass, preserving exact rejection diagnostics and handled controls |
| Callback dispatch and argument order | All 129 runtime checks pass across default, GC and ARC, including direct/bound named lambdas and extern declarations; ARC reports zero live objects |
| Native default, GC and ARC corpus | 213 goldens and 18 alternate GPU checks pass in each allocator on Mac and Linux; zero ARC leaks |
| iOS and mobile iOS | 132 goldens, 18 alternate GPU checks, C ABI imports/exports, global lifetime, 32 domain tests and app smokes pass |
| Android | 132 goldens, 18 alternate GPU checks, C ABI tests, 32 domain tests, fresh/restore persistence smokes and Gradle lint pass |
| Wasm | 147 goldens, 18 alternate GPU checks, all 66 pinned capability rejections, and hello/Studio Node and browser smokes pass |
| Syntax, LSP and codegen | 115, 165 and 124 unit tests pass respectively |
| Tree-sitter corpus | All 18 tests pass |
| C runtime and coverage | All 24 suites and 29 library coverage gates pass on Mac and Linux; C production code is unchanged |
| VS Code extension | 316 tests pass, all four coverage measures exceed 95%, and VSIX packaging passes |
| Website and bank browser tests | 107 website and 17 bank tests pass |
| Native integration and Docker API | Bank, profiler, build-tool and rebuilt Docker API checks pass |
| Lint and dead code | Format, all-target Clippy, extension lint, Hawk and the product-reference dead-code gate pass |
| Installer | Large response, API failure, missing tag and pinned idempotence checks pass |
| Pinned Deslop 0.27.0 | 4.2% duplication against the unchanged 5% ceiling |
| Compiler output comparison | All 1,278 comparisons are identical across 213 corpus programs in native release/debug/profile, Wasm, iOS and Android, including rejection diagnostics |
| Branch protection | Two active rulesets and all 14 required check contexts verified |
| Hosted acceptance | Required checks still need verification on the final submitted revision |

Coverage uses the Makefile's LCOV `LH`/`LF` totals. Counting unique `DA` source lines gives a different result and is not the gate. Platform results are attached to their compiler revision; an earlier passing run does not establish that later changes passed.

The settled specification retains exact leaves under constructor variance: representation-changing `T` to `Result<T,E>` promotion is available only at direct value sites. Expected return context can infer a result-only binder (`let xs: List<int> = empty()`); only a phantom binder absent from both parameters and result needs explicit arguments to select its type. Dynamic handler and operation instantiations come from inference; written generic rows constrain the contract without granting handler authority.

The local implementation and regression audit is complete. Retire this plan only after every hosted required check passes on the final submitted revision; the local results do not substitute for that acceptance.
