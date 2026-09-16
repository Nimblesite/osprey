# Plan 0028 — Resumption Multiplicity: how many times an effect may be answered

**Subsystem:** `tree-sitter-osprey` + `crates/osprey-syntax` (surface) +
`crates/osprey-ast` (declaration) + `crates/osprey-types` (the four checks) +
`compiler/runtime` (the `abort` and `many` continuation shapes)
**Status:** design fixed by
[spec 0035](../specs/0035-StagedEffects.md#multiplicity--multi-axis)
(`[MULTI-AXIS]` through `[MULTI-STAGE-TURN]`). Phases 0–3 and 6 are implemented:
both-flavor declarations, static checks, replay restrictions and named WASM
capability diagnostics. Runtime `abort` and `many` handlers remain rejected;
phases 4, 5 and 7 require safe unwinding, reusable continuations and tracing.
**Depends on:** [plan 0026](0026-structured-concurrency.md) for `[CANCEL-FINALLY]`
unwinding (phase 4 only), [plan 0016](0016-algebraic-effects-and-handlers.md)
for a multi-shot-capable continuation (phase 5 only),
[plan 0024](0024-staged-effects.md) for ML frontend parity on effect
declarations (phase 1 shares that work).

> Line numbers drift as code moves. The cited function, test and
> diagnostic-message names are the stable anchors.

## 1. Why

An effect row states what a function needs and, since
[spec 0035](../specs/0035-StagedEffects.md), when the request is answered. It
does not state how many times. Every dynamic effect therefore reads as "the
handler may do anything with the continuation", which merges three different
programs:

- a handler that never resumes — the computation ends where it stands;
- a handler that resumes once — retry, async, state;
- a handler that resumes repeatedly — backtracking, search, nondeterminism.

Three consequences, each a defect the compiler cannot currently name:

1. **Replay is unreported.** A multi-shot handler re-runs the remainder of the
   handled computation, so a body that sends an email sends it twice. Nothing
   in the row says so, and no check exists to say it.
2. **Every dynamic effect pays the pessimistic price.** The perform site
   allocates a suspended continuation because the compiler cannot see that the
   arm will never resume. An operation known not to resume needs a non-local
   exit and nothing else (`[MULTI-COST]`).
3. **The one-shot rule is enforced at runtime, not by the checker.** A second
   resume aborts in `__osprey_coro_resume`
   (`compiler/runtime/effects_coro.c:311`) with `fatal: continuation already
   resumed`. The arm is visible at compile time; the rejection should be too.

Multiplicity puts `abort` / `once` / `many` on the operation declaration, with
`replayable` marking an operation safe to re-perform, and derives all three
answers from it.

## 2. Phases

| Phase | Delivers | Depends on |
|---|---|---|
| 0 | `[MULTI-FALSIFY]`'s four programs, written and their outcome recorded | — |
| 1 | `[MULTI-DECL]` surface: grammar, AST field, both lowerers | 0 |
| 2 | `[MULTI-HANDLE-ONCE]` + `[MULTI-AXIS-STATIC]` checks | 1 |
| 3 | `[MULTI-REPLAY]` family: the handle-site replay check | 2 |
| 4 | `[MULTI-HANDLE-ABORT]` + the no-suspension lowering | 3, plan 0026 |
| 5 | `[MULTI-HANDLE-MANY]` + `[MULTI-HANDLE-MANY-LEXICAL]` | 3, plan 0016 |
| 6 | `[MULTI-WASM]` row-level target rejection | 2 |
| 7 | `[MULTI-TRACE]`, `[DEBUGGER-EFFECT-TRACE]`, `[LSP-EFFECT-MULTIPLICITY]` | 5 |

Phases 1–3 are buildable against the tree as it stands and are pure narrowing:
`once` is the default, is what the runtime already enforces, and changes no
running program's meaning (`[MULTI-COMPAT]`). Phases 4 and 5 each carry one
prerequisite outside this plan and each changes an existing meaning for
declarations that opt in — they do not land before their prerequisite does.

## 3. Phase 0 — the falsification gate, first

`[MULTI-FALSIFY]` is normative: the gate is written before any implementation
work, and its outcome is recorded here whether or not it passes. Four programs
under `tests/effects/multiplicity/` (Default and ML twins sharing one golden):

1. **Single-op retry.** A `Charge.charge` handler retrying on failure over a
   body that also performs `Email.send`. Accepted; the email is sent exactly
   once when the retry succeeds.
2. **Backtracking over impure code.** A `Choice.pick` handler over that same
   body. Rejected at the handle site, naming `Email.send`.
3. **Backtracking over pure code.** The same handler over a body whose only
   other effects are `Random.next` (`replayable`) and static entries. Accepted,
   producing every alternative. This is the program that exercises phase 5 end
   to end; if it cannot be written, phases 4–5 are the work and this plan says
   so rather than shipping a keyword with no program behind it.
4. **Shared `map`.** One unannotated `fn map(xs, f)` applied to a `once`
   callback under a `once` handler and a `many` callback under a `many` handler
   in one program, compiling with no multiplicity annotation on `map` — the
   multiplicity twin of `tests/regressions/effects/staged_shared.test.osp`.

Programs 2–4 do not compile before their phases land. They go in red, as
CLAUDE.md requires, and are never weakened to go green.

### Recorded outcomes

`[MULTI-FALSIFY]` is normative about recording, not only about writing, so each
measurement lands here as it is taken. Baselines below were measured with the
pre-multiplicity compiler, so they say what the axis changes rather than what it
is hoped to change.

| Observation | Measured | Verdict |
|---|---|---|
| Two `resume` sites in one block, undecorated operation | `--check` exits 0; `--run` prints `fatal: continuation already resumed` and exits 0 | The runtime guard is the ONLY thing standing between this program and a wrong answer, and it does not fail the process |
| One `resume` per `match` branch, undecorated operation | `--check` exits 0; `--run` prints `total=30` | The affine rule is per PATH; reading it as `contains_resume` would reject this |
| `abort`/`once`/`many`/`replayable` as ordinary identifiers — a function name, a parameter, two `let` bindings — beside an `effect` declaration | Runs, prints `outcome=44`, both flavors | Contextual, as `[MULTI-DECL]` requires |
| The same four words as operation NAMES (`abort: fn() -> Unit`, …), performed and handled | Runs, prints `total=7`, both flavors | The keyword reading is a parse decision, not a lexical one; nothing is reserved |
| The same double-resume program after `[MULTI-HANDLE-ONCE]` landed | `--check` exits 1 with ``handler arm `Choose.pick` may resume more than once; `Choose.pick` is declared `once` `` | [MULTI-COMPAT]'s narrowing, measured: the runtime abort became a compile error and the branchwise control stayed green |

Rows 1–4 describe the compiler before this plan; row 5 was measured on a working
tree carrying the phase 1–2 surface and check, so it records that the narrowing
behaves as [MULTI-COMPAT] specifies, not that it has shipped. The durable
evidence is the test names, which fail if it ever stops holding.

#### The four gate programs, as delivered

| Gate | Outcome | Where it lives |
|---|---|---|
| 1. Single-op retry | **Accepted.** The arm retries the gateway inside one answer, so `Charge.charge` is answered once and the body's `Email.deliver` runs exactly once. Its two `resume` sites are on different `match` branches, which the affine rule permits | `tests/effects/multiplicity/multiplicity.test.osp`, both flavors, one golden |
| 2. Backtracking over impure code | **Rejected at the handle site, naming `Email.send`**, exactly as `[MULTI-REPLAY-CHECK]` specifies | `examples/failscompilation/multi_many_over_nonreplayable.ospo` |
| 3. Backtracking over pure code | **Cannot be written.** The replay checks all pass — the body's only other effect is `replayable` — and the program is then rejected because no re-entrant continuation exists: native resume is one suspended pthread stack, and a live stack cannot be cloned. This is the gate's real finding: `many` is a declaration surface and four checks, with no representation behind it until [plan 0016](0016-algebraic-effects-and-handlers.md) supplies one | `a_many_arm_is_rejected_because_no_re_entrant_continuation_exists` (cli) |
| 4. Shared `map` | **Accepted for the multiplicities that have a representation.** One unannotated `applyTwice(f, x)` serves a `replayable` callback and a plain `once` one in one program, with no multiplicity annotation and no multiplicity variable inferred — `[MULTI-STAGE-POLY]` holds. The `many` half is blocked with gate 3 | `sharedAcrossMultiplicitiesCase`, both flavors |

Gate 2 is answered without control flow finer than the row, so
`[MULTI-REPLAY-COARSE]` has **not** hit the wall its own gate describes and
spec 0035 stands. Gate 3 says what the axis costs: the declaration, the four
checks and the cost model are separable from the runtime, and shipping the first
three without the fourth is honest only because a `many` handle site is rejected
rather than mis-compiled.

**A defect the gate found on the way.** Writing gate 1 in its natural spelling —
a handler arm reading a promoted `mut` into an inferred `Result` helper — was
rejected at lowering with ``  `attempts` has no resolved signature ``.
`genfn::alias_target` classified any identifier absent from `lookup` as a bare
callee name, and two ordinary bindings are deliberately absent from it: a
handler-promoted `mut` ([EFFECTS-HANDLER-STATE]) and a file-scope binding read
inside a function body ([`globals`]). Both were aliased as functions, and codegen
then demanded a signature the author never wrote. Fixed at the classification
site, with `a_handler_arm_reads_a_mut_cell_into_an_inferred_result_helper`
(`crates/osprey-cli/tests/effect_installer_defects.rs`) pinning it.

**A silent wrong answer the ML surface was giving.** `static effect E` parsed in
ML as the bare identifier `static` followed by a **dynamic** `effect E`, and
`handle static E` read `static` as the effect's name. The stage marker was
dropped without a diagnostic, so an ML program asking for compile-time discharge
got runtime dispatch. `handle static` now parses in ML and both markers are
contextual, which also turns `tests/regressions/effects/staged_shared.test.ospml`
— red in the tree before this work — green. Implements [STAGE-DECL] for the ML
surface, the `[FLAVOR-ML-EFFECT-ANNOTATIONS]` work this plan shares with
[plan 0024](0024-staged-effects.md).

The second row is the positive control the check must not swallow, pinned at the
CLI by `two_resume_sites_on_different_branches_stay_legal` and end to end by
`tests/regressions/effects/abort_vs_resume.test.osp`.

**The plan's own claim about the runtime guard was wrong.** The TODO below used
to require that `a_second_resume_aborts_the_program_at_runtime` (cli_e2e) keep
passing, reasoning that the checker "cannot see through a dynamically selected
handler". Osprey installs handlers lexically and rejects `resume` outside an arm
([EFFECTS-RESUME]), so every arm that could reach the guard is visible at the
handle site, and no source program reaches it once `[MULTI-HANDLE-ONCE]` lands.
That test now asserts the compile-time rejection instead — which is exactly what
[MULTI-COMPAT] demands of it ("the same program rejected earlier, not a program
that stops working") — and the guard in `effects_coro.c` stays as a backstop for
invalid compiler output, in the same sense as the generic handler-key null
lookup ([EFFECTS-GENERIC-RUNTIME]). A backstop with no source-level path to it
is not uncovered work; it is a guard that has been made unreachable, which is
the point of moving the verdict to compile time.

## 4. Phase 1 — the declaration surface

- `tree-sitter-osprey/grammar.js` — add a `multiplicity` rule beside
  `static_stage` (`grammar.js:300`) and an optional `replayable` marker, both as
  optional leading fields of `operation_declaration` (`grammar.js:315`).
  Regenerate `src/parser.c`. Precedent and shape are `static_stage`'s: a bare
  keyword node, not a general modifier list.
- `crates/osprey-ast/src/lib.rs` — `EffectOperation` (`lib.rs:337`) gains
  `multiplicity: Multiplicity` (defaulting to `Once`) and `replayable: bool`.
  Both `Stmt::Effect` sites (`lib.rs:413`, `lib.rs:545`) carry them through.
- `crates/osprey-syntax/src/default/lower.rs` and `ml/lower.rs` read the fields.
  ML surface is `[FLAVOR-ML-EFFECT-ANNOTATIONS]`; it lands with plan 0024's
  stage 2 or shares its work, since both flavors are lowering the same node.
- `crates/osprey-ast/src/stage.rs` — the operation summary (`stage.rs:70`)
  currently keeps names only; carrying multiplicity here is what lets
  `[MULTI-AXIS-STATIC]` reject a multiplicity on a static operation during the
  same pass that discharges it.
- Keywords are contextual: `abort`, `once`, `many` and `replayable` are legal
  identifiers everywhere else and MUST stay so. Verify by grep before
  reserving anything.

## 5. Phase 2 — the affine check

`crates/osprey-types/`:

- `[MULTI-HANDLE-ONCE]` — a syntactic pass over each arm's `resume` sites,
  rejecting two on one control path. Osprey has no loop construct, so the
  analysis is over `match` branches and calls only. Two `resume` sites on
  *different* branches stay legal; `tests/regressions/effects/abort_vs_resume.test.osp`
  is the existing program that must keep compiling unchanged.
- `[MULTI-AXIS-STATIC]` — a multiplicity written on a `static effect` operation
  is rejected with the message in the spec.
- The runtime guard at `effects_coro.c:311` stays as a backstop. Its
  `cli_e2e` coverage (`a_second_resume_aborts_the_program_at_runtime`) must
  keep passing: the checker cannot see through a dynamically selected handler,
  so the guard remains reachable and must remain proven.

## 6. Phase 3 — the replay check

At the handle site, in `crates/osprey-types/src/effect_rows.rs`, reusing the
row the checker already builds:

- `[MULTI-REPLAY-CHECK]` — for a region whose arm handles a `many` operation,
  every entry of the handled expression's row other than `E`'s own MUST be
  replayable. Static entries count as replayable
  (`[GPU-KERNEL-PURE]` already assumes re-running erased code is harmless).
  Name the first offending operation.
- `[MULTI-REPLAY-COARSE]` — the check reads the whole handled expression's row,
  not per-`perform` control flow. This is deliberate; the diagnostic should say
  so, because the remedy is to move the non-replayable work out of the region.
- `[MULTI-REPLAY-STATE]` — reject a `mut` capture in a `many` arm. The existing
  handler-state promotion (`[EFFECTS-HANDLER-STATE]`) is a single shared cell,
  so a second resumption would observe the first's writes.
- `[MULTI-REPLAY-FIBER]` — reject a `many` operation answered across a fiber
  boundary, naming the fiber's perform site. `[EFFECTS-FIBER-PERFORM]`
  serializes one suspend-to-resume round trip; a second resumption of a
  continuation spanning a spawned fiber has no order to belong to.

Both fail-closed rules land with the check, not after it. Relaxing either later
is additive; shipping `many` without them is not.

## 7. Phase 4 — `abort` (needs plan 0026)

- `[MULTI-HANDLE-ABORT]` — reject `resume` in an arm for an `abort` operation.
- `[MULTI-HANDLE-ABORT-MODE]` — the declaration selects the arm's mode.
  `crates/osprey-types/src/expr.rs:457` currently decides mode with
  `osprey_ast::contains_resume`; for an `abort` operation that reading inverts,
  so a `resume`-free arm abandons instead of substituting. This is the one
  place multiplicity changes an existing meaning, and it applies only to a
  declaration that opts in.
- `[MULTI-COST]` — a perform of an `abort` operation MUST NOT allocate a
  continuation. Codegen emits a non-local exit to the handler frame; no
  `__osprey_coro_*` call is emitted for that site, asserted against the emitted
  IR the way `crates/osprey-cli/tests/staged_effects.rs` asserts zero staged
  residue.
- **Gate:** `[MULTI-COST-ABORT]` requires a dropped continuation to run
  `finally` arms and release its frames' owned operands. Neither happens today
  — the killed thread runs no epilogue
  ([0017 known limits](../specs/0017-AlgebraicEffects.md#known-limits-of-abandoning-a-region)).
  Shipping `abort` first would route programs onto that path that are on the
  substituting path now, so this phase does not land before
  [plan 0026](0026-structured-concurrency.md)'s unwinding.

## 8. Phase 5 — `many` (needs plan 0016)

Two separable pieces:

- `[MULTI-HANDLE-MANY-LEXICAL]` — `crates/osprey-types/src/expr.rs:881-907`
  clears `resume_ctx` when entering any lambda, which is what makes
  `resume_inside_an_arm_lambda_is_a_type_error` (`check.rs:1827`) fire. For a
  `many` arm the justification does not hold: a lambda the arm passes to a
  higher-order function and invokes before returning DOES have a live
  continuation. Narrow the rule to escape analysis over arm-local lambdas —
  invoked-before-return keeps `resume_ctx`, escaping (stored, returned, or
  captured by anything outliving the arm) keeps the rejection. `check.rs:1827`
  gains a sibling asserting the escaping case still fails, and the existing
  test narrows rather than disappears.
- The representation. `many` needs a continuation that can be re-entered; a
  live pthread stack cannot be cloned, which is
  [plan 0016](0016-algebraic-effects-and-handlers.md)'s multi-shot-capable
  runtime. Not this plan's work, and `many` is not accepted by the checker
  until it exists — a declared `many` with no representation behind it is a
  silent wrong answer, which the broken-code process forbids.

## 9. Phase 6 — wasm32

`[MULTI-WASM]` — decide at compile time from the row instead of at link time
from a missing symbol. A `once` operation is rejected naming the operation
(replacing the `__osprey_coro_*` undefined-symbol failure that
`crates/run_test_corpus.sh` currently classifies as `SKIP`); a `many` operation
is rejected permanently with a diagnostic that says the proposal will not
provide it. Static and `abort` operations compile.

Each program the golden harness reclassifies from `SKIP` to a named rejection
must be individually verified — a `SKIP` that becomes a rejection is progress
only if the rejection is the right one.

## 10. Phase 7 — the trace

`[MULTI-TRACE]` — a continuation carries the chain of perform sites it has
passed through, one word per hop. `[DEBUGGER-EFFECT-TRACE]` presents it beside
the physical stack, resolving each entry through `[DEBUGGER-SOURCE-MAP]`;
static perform sites MUST NOT appear. `[LSP-EFFECT-MULTIPLICITY]` reads
`textDocument/implementation` in reverse — from an arm to every perform site it
answers — which for a `many` arm is the replay set its author is responsible
for, plus the tail-resumptive hint of `[MULTI-STAGE]`.

## TODO

### P0. Falsification gate (before anything else)

- [ ] Write `[MULTI-FALSIFY]` 1–4 under `tests/effects/multiplicity/` with ML
      twins sharing one golden; commit them red where they cannot yet compile
- [ ] Record each program's outcome in this document — including "cannot be
      written", which is the finding the gate exists to force
- [ ] If (2) cannot be rejected without control flow finer than the row, stop:
      `[MULTI-REPLAY-COARSE]` has hit its wall and spec 0035 is wrong

### S. Surface (phase 1)

- [x] `multiplicity` + `replayable` rules in `tree-sitter-osprey/grammar.js`
      beside `static_stage`; `src/parser.c` regenerated. The modifier words are
      also legal operation NAMES, which GLR resolves — no external scanner
- [x] `Multiplicity` enum (`osprey-ast/src/multiplicity.rs`) + `declared_multiplicity`
      / `replayable` on `EffectOperation`. The annotation is kept AS WRITTEN
      (`Option<Multiplicity>`) because `[MULTI-AXIS-STATIC]` rejects one that was
      written and `once` — the default — is a legal thing to write
- [x] Default lowerer reads both fields; ML lowerer likewise, contextually
      (`[FLAVOR-ML-EFFECT-ANNOTATIONS]`). The shared plan 0024 work — ML
      `static effect` and `handle static` — landed with it, since ML was
      silently dropping the stage marker
- [x] Multiplicity in `stage.rs`'s operation summary; `[MULTI-AXIS-STATIC]`
      rejects during discharge, fixture
      `multi_multiplicity_on_static_effect.ospo`
- [x] Verify `abort` / `once` / `many` / `replayable` remain legal identifiers
      everywhere else — both as ordinary bindings and as operation NAMES, in
      both flavors; recorded in [Recorded outcomes](#recorded-outcomes)

### C. Checks (phases 2–3)

- [x] `[MULTI-HANDLE-ONCE]` affine pass — `resumes_on_one_path` is branch-AWARE
      (max over `match` arms, sum in sequence), which is why it is not
      `contains_resume`
- [x] `[MULTI-AXIS-STATIC]` rejection with the spec's message
- [x] `[MULTI-REPLAY-CHECK]` at the handle site, naming the first offender in
      sorted row order so the diagnostic is deterministic
- [x] `[MULTI-REPLAY-STATE]` — `mut` capture in a `many` arm rejected
- [x] `[MULTI-REPLAY-FIBER]` — `many` across a fiber boundary rejected, following
      the call graph out of the handled expression, not only its syntax
- [x] `tests/regressions/effects/abort_vs_resume.test.osp` compiles unchanged
- [x] `a_second_resume_on_one_path_is_rejected_at_compile_time` (cli_e2e)
      replaces `a_second_resume_aborts_the_program_at_runtime`: the arm is
      visible, so the verdict moves to the checker ([MULTI-COMPAT]) and the
      runtime guard becomes a backstop with no source-level path to it. The
      positive control `two_resume_sites_on_different_branches_stay_legal` is
      green
- [x] The backstop is proven where it now lives — at the C level, by
      `death_second_resume_of_a_finished_continuation`
      (`compiler/runtime/effects_runtime_tests.c`). A guard whose only path is
      invalid compiler output cannot be reached from a source program by
      construction, and an unproven backstop is indistinguishable from a removed
      one. It `exit`s rather than `abort`s, so `test_death.h` grew
      `osp_death_exit` to read the status

### A. `abort` (phase 4 — blocked on plan 0026)

Landed now: `[MULTI-HANDLE-ABORT]` rejects a `resume` in an `abort` arm, and a
`resume`-free `abort` arm is rejected too, naming plan 0026. That second
rejection is the honest answer, not a placeholder: `[MULTI-HANDLE-ABORT-MODE]`
requires such an arm to ABANDON the region, compiling it under today's
substituting rule would make the `perform` return, and routing it onto the
abandon path leaks the discarded frames' owned operands. `MIXED_UNDECLARED`'s
two suspension sites are pinned by
`the_undeclared_control_pays_for_a_continuation_at_every_perform_site`, so
phase 4's saving is measured against a recorded number.


- [ ] `[CANCEL-FINALLY]` unwinding exists and releases the abandoned frames'
      owned operands; the ARC exit audit in `crates/run_test_corpus.sh` reports
      zero live objects for the abandoning program in
      [0017 known limits](../specs/0017-AlgebraicEffects.md#known-limits-of-abandoning-a-region)
- [ ] `[MULTI-HANDLE-ABORT]` rejection fixture
- [ ] `[MULTI-HANDLE-ABORT-MODE]` — mode read from the declaration at
      `osprey-types/src/expr.rs:457`, with a test pinning that an undeclared
      operation's `resume`-free arm still substitutes
- [ ] No `__osprey_coro_*` symbol in the emitted IR for an `abort` perform
      site, asserted like `crates/osprey-cli/tests/staged_effects.rs`

### M. `many` (phase 5 — blocked on plan 0016)

Landed now: the declaration surface, all four replay checks, and — after they
pass — a rejection naming this plan's prerequisite. A `many` handle site is
never compiled, because a re-entrant continuation does not exist and answering a
re-entrant request with one that cannot be re-entered is a silently wrong
answer. The checks run BEFORE that rejection so each stays live and tested: a
`many` region over non-replayable code reports `Email.send`, not the missing
runtime.


- [ ] A multi-shot-capable continuation exists (plan 0016)
- [ ] `[MULTI-HANDLE-MANY-LEXICAL]` — escape analysis over arm-local lambdas at
      `osprey-types/src/expr.rs:881-907`; `resume_inside_an_arm_lambda_is_a_type_error`
      (`check.rs:1827`) narrowed, with a sibling test for the escaping case
- [ ] `[MULTI-FALSIFY]` case 3 goes green and produces every alternative
- [ ] `[MULTI-STAGE-TURN]` — a test pinning that a `many` arm holds its turn
      across every resumption

### W. Targets and tools (phases 6–7)

- [x] `[MULTI-WASM]` — the rejection names the OPERATION (demangled for a
      module-scoped effect) instead of the `resume` keyword; all 28 manifest
      reasons in `tests/WASM_UNPORTABLE.txt` re-verified. `many`'s PERMANENT
      wording is not reachable: the checker rejects `many` on every target
      before the per-target gate, so it lands with phase 5
- [ ] `[MULTI-TRACE]` perform-site chain on the continuation
- [ ] `[DEBUGGER-EFFECT-TRACE]` view, static sites absent
- [ ] `[LSP-EFFECT-MULTIPLICITY]` reverse implementation query + the
      tail-resumptive hint

### V. Verification (gating retirement)

- [x] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
      and `cargo test --workspace` clean; nothing hand-suppressed. `make ci`'s
      `_deslop` gate needs the `deslop` binary, which is absent on this machine
- [x] `crates/run_test_corpus.sh` byte-exact at 209/209 under the default
      backend, `--memory=gc`, and `--memory=arc` with `TEST_CORPUS_ARC_LEAKY=0`;
      144/144 under `OSPREY_TARGET=wasm32`. The golden floors were ratcheted
      203 → 209 and 142 → 144
- [x] Every landed `[MULTI-*]` section cited by a comment in the code
      implementing it, and removed from the exemption list in
      [`docs/specs/README.md`](../specs/README.md). The seven still blocked on
      plan 0016 or plan 0026 stay listed there, each naming its blocker
- [x] Rejection fixtures under `examples/failscompilation/` for each check,
      with exact `.expectedoutput` text: `multi_multiplicity_on_static_effect`,
      `multi_once_arm_resumes_twice`, `multi_undeclared_resumes_twice`,
      `multi_abort_arm_resumes` (+ `ml_` twin), `multi_many_over_nonreplayable`,
      `multi_many_arm_captures_mut`, `multi_many_across_fiber`,
      `multi_many_resume_in_escaping_lambda`
- [ ] Coverage thresholds in `coverage-thresholds.json` did not go down
- [ ] deslop `top-offenders` over the touched Rust; no new duplication
- [ ] Plan retired in `docs/plans/README.md` with named tests as evidence
