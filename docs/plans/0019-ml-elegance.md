# Plan 0019 — ML Flavor Elegance

**Status:** **Specified, not implemented.** Spec
[0024 — ML Flavor Syntax](../specs/0024-MLFlavorSyntax.md) now carries the target
surface as normative text; the current frontend rejects all of it. Spec
[0013 — Error Handling](../specs/0013-ErrorHandling.md) and
[0004 — Type System](../specs/0004-TypeSystem.md) carry `[ARITH-PLAIN]`; spec
[0003 — Syntax](../specs/0003-Syntax.md) carries the Default-flavor spelling of
positional payloads. Nothing in `crates/` has changed. See
[§What is left](#what-is-left).

## Summary

The ML flavor is more verbose than the language it takes its name from.
`benchmarks/cases/binarytrees/binarytrees.ospml` is 22 code lines; its Haskell
twin is 9, of which 3 are optional signatures. Five of Osprey's decisions
account for the gap, and the largest of them defends nothing.

This plan removes the ceremony in four phases, ordered so that every phase ships
independently and the two cheapest phases carry the whole line-count win. The
target:

```osprey-ml
type Tree = Leaf | Node Tree Tree

make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

check Leaf = 0
check (Node l r) = 1 + check l + check r

print "${range 0 1200 |> fold 0 (\acc _ => acc + check (make 13))}"
```

6 code lines / ~73 tokens, against signature-free Haskell's 6 / ~79 — and Osprey
pays for none of `main`, `IO ()`, `import`, `show`, or the `:: Int` defaulting
pin, while keeping enforced exhaustiveness and a checked `/`.

## Evidence

Measured against the 102 hand-written `.ospml` files (7,513 LOC) under
`examples/` and `benchmarks/`, excluding the generated bundle.

| Root cause | Where | Corpus cost |
|---|---|---|
| `\|` is not a lexeme | `crates/osprey-syntax/src/ml/lexer.rs` (`single_char_operator`) — `unexpected character '|'` | 48 of 54 type declarations exceed two lines; 236 LOC |
| Payload fields must be named | `crates/osprey-ast/src/lib.rs:230-248`, `ml/parser.rs:394` | 124 sites re-type 298 `field = value` pairs |
| Arithmetic is `Result`-typed | `crates/osprey-types/src/expr.rs:736-742` vs strict `push_unify` in `osprey-types/src/pattern.rs:65` | 11 operator wrappers; 112 of 204 signature lines exist only to force auto-unwrap |
| Definition heads take identifiers only | `crates/osprey-syntax/src/ml/parser.rs:878` (`is_binding_head`) | 88 functions shaped `name arg = match arg`, ~600 arm lines |
| `(` is a parse error in pattern position | `crates/osprey-syntax/src/ml/parser.rs:1459` | blocks the fix above |
| ML has no `?:` | `crates/osprey-syntax/src/ml/lexer.rs` rejects `?` outright, though Default's `?:` works | 207 `Success v => v` / `Error m => fallback` blocks in 38 files, ~620 LOC (15–20% of all hand-written ML) |

### The arithmetic wrapper is a fiction

`gen_arith` (`crates/osprey-codegen/src/expr.rs:217-228`) emits a bare
`add i64` / `sub i64` / `mul i64` / `srem i64` and unconditionally `make_ok`s the
result. No overflow intrinsic, no zero check. Reproduced:

```
let big = 9223372036854775807
print("overflow: ${big + 1}")   // -9223372036854775808  — silent two's-complement wrap
print("modzero: ${10 % 0}")     // garbage — undefined `srem` by zero
```

`MathError::Overflow` is unreachable and `%`-by-zero is undefined behaviour. The
corpus pays 11 wrapper functions and 112 signature lines for a guarantee that
has never existed.

**And it is not only a source-level cost — it is a heap allocation per
arithmetic operation.** `make_ok` → `make_result`
(`crates/osprey-codegen/src/result.rs:22-40`) calls `malloc_struct`, so
`fn addup(a: int, b: int) -> int = a + b` emits:

```llvm
%r0 = add i64 %a, %b
%r3 = call i8* @osp_alloc_tagged(i64 %r2, i64 1025)   ; heap-allocate the Result
%r5 = getelementptr ... i32 0, i32 0
store i64 %r0, i64* %r5                                ; store payload
store i8 0, i8* %r6                                    ; store discriminant
```

— one `add`, one runtime allocation, three stores, then an immediate unwrap back
to `i64`. Every `+`, `-`, `*` and `%` in every Osprey program pays this. That
makes `[ARITH-PLAIN]` the largest single performance change available in the
compiler, not merely a syntax cleanup: the arithmetic-dominated benchmark cases
(`fib`, `ackermann`, `nestedloop`, `collatz`, `binarytrees`) allocate once per
operation today. `website/src/benchmarks.md` previously attributed the resulting
gap to "the cost of that safety"; it has been corrected, and the suite must be
re-measured after phase 2.

### Two live defects to fix along the way

- **`bind_variant_fields`** (`crates/osprey-codegen/src/pattern.rs:341-360`)
  resolves each pattern binder against the variant layout **by name**, so
  today's `Node left right` works only because the binders happen to equal the
  declared field names. Rename them and the program typechecks, then dies at
  `codegen: unknown name`. Default-flavor `Ctor(x)` has the same hole:
  `osprey-types/src/pattern.rs:262-267` binds `sub_patterns` positionally while
  `pattern_ctor` (`osprey-codegen/src/pattern.rs:178-183`) discards them. Fixing
  this to bind by index is owed regardless of this plan.
- **`finish_phi`** (`crates/osprey-codegen/src/pattern.rs:461-468`) returns
  `Value::unit()` when arm LLVM types disagree instead of erroring, converting a
  class of type-system mistakes into silently-unit expressions.
- **Arithmetic swallows operand errors.** An arithmetic expression with a
  `Result`-typed operand unwraps it instead of propagating, and fabricates a
  success payload on the error path: `toString((10 / 0) + 1.0)` evaluates to
  `Success(1.0)`, not `Error(division by zero)`. The specs said "if any
  sub-expression errors, the chain errors"; that has never been true. This is
  independent of `[ARITH-PLAIN]` — a division-by-zero is silently converted into
  a wrong number today — and should be fixed first, since phase 2 narrows the
  wrapper to exactly the operators (`/`, `%`) whose errors this loses.

## Phase 1 — pure sugar

Zero AST change, zero type-checker change. **Every file touched is under
`crates/osprey-syntax/src/ml/`.** Ship in this order; each item is
independently mergeable.

1. **`[FLAVOR-ML-PATTERN-GROUP]`** — one `TokKind::LParen` arm in `fn pattern()`
   (`ml/parser.rs:1459`). Grouping erases at parse time. `(a, b)` is an error,
   not a tuple; nested constructor patterns stay rejected until the
   `sub_patterns` fix lands in phase 3. ~6 LOC.
2. **`[FLAVOR-ML-UNION-INLINE]`**, named payloads only — `TokKind::Pipe` in
   `ml/token.rs`; `|` in `single_char_operator` (`ml/lexer.rs:410`) **after**
   `two_char_operator` so `||` and `|>` keep maximal munch; a union arm in
   `type_decl_after_keyword` (`ml/parser.rs:277-281`) before the manifest-alias
   fallback, with a forward-progress guard. Keep `|` out of `infix_bp`
   (`ml/parser.rs:74`). Commit to the union branch only on an uppercase head so
   `type Id = int` still aliases. Ship the "or-patterns are not supported"
   diagnostic with it — once `|` lexes, users will try `Leaf | Node l r =>` and
   deserve better than a generic parse error. ~80 LOC.
3. **`[PARAM-WILDCARD]` — shared core, both flavors.** ML: `_` in `one_param`
   (`ml/parser.rs:837`); `flat_params` / `curry_params` (`ml/lower.rs:795`) emit a
   `Parameter` with a generated unspellable name so repeated `_`s cannot collide.
   Default: `parameter` (`tree-sitter-osprey/grammar.js:196`) admits `_` beside
   `identifier`, and the Default lowerer emits the *same* generated name — without
   it `\acc _ => …` has no [FLAVOR-IR-EQUIV](../specs/0023-LanguageFlavors.md#cross-flavor-equivalence-tests)
   twin, since `|acc, _| => …` would not parse. `Expr::Lambda.position` must stay
   unique — inference publishes the resolved lambda type under it
   (`osprey-ast/src/lib.rs:664-666`). ~20 LOC.
4. **`[FLAVOR-ML-CLAUSES]`** — `is_binding_head` (`ml/parser.rs:878`) accepts
   literal / constructor / `_` / `(` before the `=`, still requiring a top-level
   `Eq` before the newline; `MlItem::Binding`'s head becomes `Vec<MlPattern>` in
   `ml/cst.rs`; `ItemLower::lower_binding_item` (`ml/lower.rs:261`) merges
   consecutive same-name same-arity bindings into one `Stmt::Function` over
   `Expr::Match`; `collect_bound_names` (`ml/lower.rs:68`) still registers the
   name once so call sites curry. The scrutinee parameter takes its name from
   the first bare-identifier head — load-bearing, so the Default twin
   `fn make(d) = match d { … }` stays byte-identical under
   [FLAVOR-IR-EQUIV](../specs/0023-LanguageFlavors.md). Each arm carries its own
   clause `Position` per [FLAVOR-LOWER-CONTRACT] rule 2 or exhaustiveness errors
   land on the wrong line. ~120 LOC.
5. **ML `?:`** — the Result default the Default flavor already has. `?:` in
   `two_char_operator` (`ml/lexer.rs`, matched before `?` and `:` so maximal
   munch holds against the type-signature colon), in `infix_bp`
   (`ml/parser.rs:74`), lowered in `ml/lower.rs` to the **same shape Default
   emits** — `lower_ternary` (`default/expr.rs:191-205`) reuses the scrutinee as
   the `then` branch of an `Expr::Match` carrying two `Pattern::Literal(Bool)`
   arms. Not a `Success`/`Wildcard` pair: that is a different node and breaks
   [FLAVOR-IR-EQUIV](../specs/0023-LanguageFlavors.md#cross-flavor-equivalence-tests)
   against the Default twin
   ([PATTERN-RESULT-DEFAULT](../specs/0007-PatternMatching.md#result-default---pattern-result-default)).
   The scrutinee must therefore be a `Result` or a `bool` — `5 ?: -1` is
   `cannot unify int with bool`, in both flavors alike. **No new operator, no
   AST change, no tree-sitter change, no Default-flavor work** —
   `let v = intDiv(a: 10, b: 2) ?: -1` already prints `5`, while the same line
   in a `.ospml` dies at `unexpected character '?'`. ~40 LOC.

**Result: binarytrees 22 → 14 lines**, and ~620 LOC removed corpus-wide by `?:`
alone.

### Corpus sweep required by phase 1

Today two same-name bindings compile clean and last-wins silently:
`f x = 1` / `f x = 2` prints `2` with no diagnostic. After the merge that is
either two irrefutable arms (first wins) or — preferred — an unreachable-clause
error. Turning undiagnosed nonsense into an error is right, but sweep the corpus
before merging. Non-adjacent same-name bindings must be a hard
duplicate-definition error, never a merge: a function whose definition is
scattered through a file is worse than the ceremony it removes.

## Phase 2 — `[ARITH-PLAIN]`

Type system only; still no AST change.

- `int_arithmetic_result` (`crates/osprey-types/src/expr.rs:736-742`) returns
  `Type::int()` for `+ - *`; `gen_arith` drops the `make_ok` wrap.
- `/` is unchanged: `Result<float, MathError>`, divide-by-zero checked.
- `%` keeps `Result<int, MathError>` and **gains a real zero check**, replacing
  today's undefined `srem`.
- New builtins `checkedAdd` / `checkedSub` / `checkedMul` in
  `crates/osprey-types/src/builtins.rs` + codegen, returning
  `Result<int, MathError>` via `llvm.s{add,sub,mul}.with.overflow`, following the
  existing `intDiv` precedent. This is the first time the overflow guarantee is
  real; it is opt-in at the call site that wants it.
- Specs 0004 and 0013 already reflect this. Auto-unwrap context 1 ("nested
  arithmetic") ceases to exist — five contexts become four.
- Expected churn: every `.expectedoutput` showing `Success(n)` from `toString`
  over an arithmetic expression. Large but mechanical.

The line to defend in review: operators whose only failure mode is overflow
return the plain type — overflow wraps, and already does so today; operators
that can fail on a value with no representable result (`/` and `%` by zero) keep
`Result`.

**Result: binarytrees 14 → 6 lines.** Corpus-wide, 11 operator wrappers deleted
and 112 signature lines become removable. Separately and more importantly, one
heap allocation per arithmetic operation disappears — re-run `make bench` and
update `website/src/benchmarks.md` with the new numbers as part of this phase.

### Rejected: the match-arm `Result` join

Making match arms join `Result<T, E>` with `T` looks like the smaller fix and is
not. Three independent reasons:

1. **Non-principal.** The checker is an eager unifier with no deferred
   constraints. When an arm's pruned type is still `Type::Var` the join must
   guess between unwrapping and widening, and the guess propagates. Keeping
   auto-unwrap out of plain `unify` is what prevents this today.
2. **It silently miscompiles.** `gen_arith` returns a `Result` struct, so a
   joined match mixes `i64` and Result-shaped arms — and `finish_phi` returns
   `Value::unit()` rather than erroring on mismatched arm types. Accepting the
   join at the type level without a checker-published per-arm unwrap table
   yields a match that silently evaluates to unit.
3. **It does not fix the target program.** `fold 0 (\acc _ => acc + check (make 13))`
   needs the lambda body to unify under a `Con` argument, where
   `try_unwrap_result` (`crates/osprey-types/src/unify.rs:82-99`) deliberately
   does not fire.

## Phase 3 — `[TYPE-UNION-POSITIONAL]`, shared core

The one AST change, and the correctness fix.

- Add positional discrimination to `osprey_ast::TypeField`
  (`crates/osprey-ast/src/lib.rs:230-248`).
- Add the positional alternative to the `variant` rule at
  `tree-sitter-osprey/grammar.js:220` so Default spells `Node(Tree, Tree)`.
- Constructor-arity pre-pass in `ml/lower.rs` mapping ctor name → declared fields
  in order, precedented by the existing `BOUND_NAMES` collector
  (`ml/lower.rs:40-68`), so it does not breach [FLAVOR-BOUNDARY].
  `lower_application` (`ml/lower.rs:1170`) folds a saturated
  `App(Ident(Ctor), args)` into `Expr::TypeConstructor`.
- Fix `pattern_ctor` / `bind_variant_fields` to bind `sub_patterns` **by index**.

This must be shared core, not ML sugar. [FLAVOR-LOWER-CONTRACT] rule 5 forbids a
flavor-only node shape, and [FLAVOR-IR-EQUIV] requires a Default twin for every
`.ospml` — so the sugar-only route would force twins to spell
`type Tree = Leaf | Node { _0: Tree, _1: Tree }`, putting synthesized
identifiers into checked-in fixtures, making `t._0` legal and undocumented, and
leaking `_0` into every type-error message.

Two rules the spec already states and the implementation must enforce:
constructors do not curry (`Node Leaf` is an arity error, since
`Expr::TypeConstructor` has no partial form); and positional patterns bind
positionally **only** against positionally-declared variants, so no existing
`.ospml` changes meaning. The pre-pass sees only ctors declared in the
compilation unit — an imported ctor falls back to the named form or is
diagnosed, never silently mis-lowered as a curried call.

**Result: the 6-line target at ~73 tokens.** Corpus-wide, 298 `field = value`
pairs deleted.

## Phase 4 — `[ARITH-EFFECT]`

The end state, and the thing no comparable language can do: `+ - *` return `int`
and *perform* `Arith.overflow`; `/` and `%` perform `Arith.divideByZero`. The
operation result type is `int`, so a handler may `resume` with a saturating,
wrapping, or sentinel value, and the *same* expression is trapping, saturating,
or wrapping depending on an enclosing handler.

```osprey-ml
handle Arith
    overflow op => resume 9223372036854775807     // saturate
in
    check (make 13)
```

**Blocker:** `crates/osprey-types/src/check.rs:582-592` takes effect rows from
declarations only. There is no effect-row inference, so the options today are
`! Arith` on every arithmetic-bearing signature — resurrecting the 112 signature
lines this plan exists to delete — or exempting `Arith` from the unhandled-effect
check.

**Reject the exemption.** An effect that can never appear in a row and is never
reported unhandled is not typed by the effect system; it is an abort with
handler syntax attached. It punches a permanent hole in the guarantee Osprey
markets as world-first, and the moment there is a second ambient effect the claim
becomes "compile-time effect safety, except…".

Therefore: build effect-row inference first — the natural completion of the
effect system, owed anyway (see
[plan 0016](0016-algebraic-effects-and-handlers.md)) — then land
`[ARITH-EFFECT]` with a genuinely inferred row. `[ARITH-PLAIN]` is
forward-compatible: the surface syntax is identical either way, so nothing
written in phases 1–3 changes. Only the row and the codegen path do. Measure the
branch cost of the overflow intrinsics before merge and keep the unchecked
lowering behind a release flag if it shows.

## Compatibility budget

**Constraint: slight breaking changes are acceptable; a reassembly of the flavor
is not. Most existing samples must keep compiling.** Measured against the 208
corpus files (103 `.ospml`, 105 `.osp`):

| Phase | Breaking? | Corpus impact |
|---|---|---|
| 1.1 `(P)` patterns | No | `(` is a parse error in pattern position today |
| 1.2 inline `\|` unions | No | `\|` does not lex; the layout form stays valid |
| 1.3 `_` params | No | `_` is a parse error in parameter position today |
| 1.4 clauses | **Yes, narrow** | see below |
| 1.5 ML `?:` | No | `?` does not lex |
| 2 `[ARITH-PLAIN]` | **Yes** | 23 files, 8 `.expectedoutput` |
| 3 positional payloads | No | new declaration form; named variants keep by-name binding |

Phases 1.1–1.3, 1.5 and 3 are **purely additive by construction**: every token
they introduce is a lexical or parse error today, so no existing program can be
using it. Verified — the only `|` and `?` characters in the `.ospml` corpus are
inside string literals and comments, which the shared `crate::strings` scanner
handles and neither lexer change touches. Positional payloads are additive
because a bare `C a b` against a **named**-payload variant keeps its existing
by-name meaning; that asymmetry is the compatibility guarantee, not an oversight.

**Phase 2 is the only broad break**, and it is mechanical in the shrinking
direction. 23 of 208 files match `Success`/`Error` over a `+ - *` expression
(76 arm sites), e.g. `comprehensive_math.osp:10`:

```osprey
2 => match base * base { Success { value } => value  Error { message } => 0 }
2 => base * base                                     // migrated
```

No logic changes; each site loses a wrapper. 8 `.expectedoutput` files carry
`Success(n)` from `toString` over arithmetic and are regenerated. 89% of the
corpus is untouched.

**Phase 1.4 is the one behavioural change to watch.** Two same-name bindings
compile clean today and last-wins silently (`f x = 1` / `f x = 2` prints `2`);
after the merge they are an unreachable-clause error. That is undiagnosed
nonsense becoming a diagnostic, but it must be swept before merging, and
non-adjacent same-name bindings must stay a hard duplicate-definition error.

Nothing in phases 1–3 removes an existing form. The ML layout union, layout
record construction, named-field payloads, and the explicit `match` all remain
normative and valid; the new forms sit beside them.

## What is left

Everything. No `crates/` change has been made. The specs describe the target
surface and mark it unimplemented; `benchmarks/cases/binarytrees/binarytrees.ospml`
and the rest of the corpus still use the current syntax and must keep compiling
untouched through phase 1, then migrate per phase.

## TODO

- [ ] Phase 1.1 — `[FLAVOR-ML-PATTERN-GROUP]`: `LParen` arm in `fn pattern()`
- [ ] Phase 1.2 — `[FLAVOR-ML-UNION-INLINE]`: `TokKind::Pipe`, lexer order, union arm, or-pattern diagnostic
- [ ] Phase 1.3 — `[PARAM-WILDCARD]`: `_` param with generated name, both flavors
- [ ] Phase 1.4 — `[FLAVOR-ML-CLAUSES]`: pattern heads, CST change, consecutive-clause merge, per-clause spans
- [ ] Phase 1.5 — ML `?:`: lex and parse the Result default the Default flavor already implements
- [ ] Phase 1.x — corpus sweep for silently-shadowed same-name bindings
- [ ] Phase 2 — `[ARITH-PLAIN]`: `int_arithmetic_result`, `gen_arith`, `%` zero check, `checked*` builtins, `.expectedoutput` regeneration
- [ ] Phase 2 — re-run `make bench` and update `website/src/benchmarks.md`: the per-operation `osp_alloc_tagged` disappears
- [ ] Phase 3 — `[TYPE-UNION-POSITIONAL]`: `TypeField`, `grammar.js:220`, ctor pre-pass, `sub_patterns`-by-index fix
- [ ] Defect — `finish_phi` must error on mismatched arm LLVM types, not return `Value::unit()`
- [ ] Defect — arithmetic must propagate a `Result` operand's error, not unwrap it and fabricate `Success` (`(10 / 0) + 1.0` → `Success(1.0)`)
- [ ] Phase 4 — effect-row inference, then `[ARITH-EFFECT]`
- [ ] Migrate the `.ospml` corpus per phase; `osprey-fmt` must round-trip every new form and never convert between clauses and `match`
