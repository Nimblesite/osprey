# Generic error payloads

Make `Result<T, E>` hold its error at type `E`, end to end, as [ERR-RESULT](../specs/language/0013-ErrorHandling.md#resultt-e--err-result) states. Before this work the checker pinned `Error { message }` to `string` and left `E` unconstrained, and the backend stored only a C string in the error slot (a number payload was silently dropped).

## 🚨 Top priority: gaps that block top tier

These come before every other open item in `docs/plans/`. Each is a defect against a top-tier language, seen with a concrete program on this branch. None may be dropped or deferred behind new features; tick only with a corpus regression under GC, ARC and wasm32. Work them in this order: language first, since built-in constraints remove the ad hoc checks the error-system items patch around.

### Language

- [x] **Built-in constraints carried in the type scheme ([TYPE-CONSTRAINTS](../specs/language/0004-TypeSystem.md#built-in-constraints--type-constraints)).** A generic function like `fn show(x) = toString(x)` lost the "x must be printable" obligation, so it was only checked where a concrete type happened to reach it. Elm-style built-in constraints (`printable`, `comparable`, `sortable`, `sized`, `number`) now travel in the function's scheme; effects, module signatures and records of functions cover everything user-extensible. No type classes.
  - [x] Spec: [TYPE-CONSTRAINTS] written
  - [x] Spec: "not implemented" status removed; table matches the checker (`comparable` is scalars plus payload-free variants, `sortable` is `listSort`'s own)
  - [x] Checker: the constraint is an obligation in `Scheme.obligations`; generalization keeps it, instantiation re-locates it at each call (`located_obligations`), and validation after unification discharges it. No separate `Type::Var` field: the obligation already names the variable, so one mechanism serves every constraint
  - [x] `is_printable`, `is_sortable_list`, `is_sized` and the order checks run on those carried obligations; `validate_builtin_uses` is the discharge step, not a post-hoc patch
  - [x] Error at the call that decides the type, naming the function and constraint: "cannot convert value to string: (int) -> int; `show` needs a printable argument"; a direct built-in use names its own line
  - [x] Hover, inferred signatures and `--docs` show the constraint: `fn show(x: printable) -> string`, `sorted : List<sortable> -> List<sortable>`
  - [x] `failscompilation/` rejections for each constraint through one, two and three levels (`print_record_unsupported`, `ml_interpolation_unprintable`), hover and `--docs` tests in both flavors
  - [x] `number` does not travel, by design: a numeric definition takes ONE overload chosen by its uses (GPU kernels rely on it); stated in [TYPE-CONSTRAINTS]
- [x] **Nested patterns ([PATTERN-NESTED](../specs/language/0007-PatternMatching.md#nested-patterns--pattern-nested)).** `Node(Node(l, _), r)` and `Error { message: Overflow { by } }` were rejected; each slot was a binding or `_`, forcing a `match` per level.
  - [x] Spec: [PATTERN-NESTED] written
  - [x] Grammar and both parsers accept a pattern in every slot: Default `field: pattern` brace slots, tuple and row slots (`slot_pattern`); ML `Ctor p q` by juxtaposition as ML builds the value, `Ctor(field = p)` by label, tuple and `{ f = p }` row slots. Both lower through `desugar::{positional,labelled}_ctor_pattern`, and `Pattern::Structural` slots are patterns
  - [x] Checker: each nested pattern is typed at its slot's type (`Result` payloads at `T`/`E`, rows at the field's type); a name bound twice in one arm is rejected; a slot testing an `any` row's field is rejected
  - [x] Exhaustiveness and reachability over the whole nested pattern by usefulness (`pattern/usefulness.rs`), with a witness: "no arm matches `Node(Node(_, _), Node(_, _))`"
  - [x] Backend: an arm that tests below its top runs `nested::capture` and falls through to the next arm on any failed test, on union, row and `Result` matches; bindings install only once the whole pattern matched. GC, ARC and wasm32 corpus green
  - [x] Corpus regression (both flavors, `core/foundations/checks`) mixing trees, `Result` in `Result`, union payloads, literals and lists three levels deep; `failscompilation/` rejections for a non-exhaustive nest, unreachable nested arms and a name bound twice (`match_not_exhaustive`, `match_unreachable_after_catch_all`)
  - [x] Spec: "not implemented" status and the [TYPE-UNION-POSITIONAL] note removed; ML spelling specified in [PATTERN-NESTED] and [FLAVOR-ML-MATCH]

### Error system

- [x] `expectError` on an inline `if` needs an annotation: `expectError(if c { Error { message: DivZero } } else { Success { value: 1 } }, DivZero)` fails with "expectError on a Result whose error type is not decided here; annotate it". The checker knows `E`; the backend must take it from the checked type instead of the operand's syntax.
- [x] Vague assertion messages: a failed `expectError` on a record or union payload prints "expected Error(<the given MathError>), got a Success or an Error holding another MathError". Records and unions now render ([BUILTIN-PRINT-FORM]), so it must print both values: `expected Error(DivZero), got Error(Overflow { by: 3 })`. Same for `expect` and `check`. Done: assertions compare renderings of any printable operand ([TESTING-EQUALITY]); an operand with no rendering is a compile error.
- [x] A union variant holding an unprintable value is accepted by `print`: `print(Held { m: someMap })` compiles and shows `Held { m: <map> }`, while the same value in a record field is rejected. User types reach the constraint as a name, so the check must read the declaration and reject it at compile time.
- [x] A quoted string inside a record, variant or list is not escaped: `{ why: "a"b" }` is ambiguous. Escape `"`, `\` and control characters as the string literal syntax does.
- [x] `Map` has no rendering. Decided: keep the rejection, reason in [BUILTIN-PRINT-FORM]. Decide and specify one (`{ "k": 1 }`, key order defined) or keep the rejection and say why in [BUILTIN-PRINT].

## Rules being implemented

- [ERR-RESULT]: `T` and `E` are both generic, inferred, exact and must agree.
- [ERR-PAYLOAD]: `Error { message }` binds the producer's value at type `E` on GC, ARC and wasm32.
- [ERR-RESULT-FORMAT]: `Error(<payload>)` renders the payload at its own type.
- [TESTING-BUILTIN-EXPECT], [TESTING-BUILTIN-CHECK], [TESTING-BUILTIN-EXPECT-ERROR], [TESTING-EQUALITY]: assertion operands share one inferred type; `expectError` takes the Result's own `E`.
- There is no built-in `Error` type; `Error` is only the `Result` constructor. Each fallible built-in states its own `E`: `IndexOutOfBounds { index, length }` for a list read ([BUILTIN-LIST-GET]), `string` for the rest.

## Specs

- [x] 0013-ErrorHandling: `Result<T, E>` is the pillar; six rules; formatting
- [x] 0004-TypeSystem: remove the "phantom error channel"; holes; no `Error` type; block shape
- [x] 0007-PatternMatching: `Error` binder is the payload at `E`
- [x] 0027-TestingFramework: generic `expect`, `check`, `expectError`; one-type equality

## Checker (`osprey-types`)

- [x] `Error { message: E }` constructor field is `E`, not `string`
- [x] `Error { message }` pattern binds `E`
- [x] `expectError : Result<T, E>, E`; `expect : T, T`; `check : string, T, T`
- [x] A `Result` is printable when both sides are
- [x] `Error` is not a type: built-ins return `Result<T, string>`; `Error` in a type position is rejected
- [x] Rejections in `src/osprey/examples/failscompilation/`: wrong `E` returned, wrong `expectError` payload type, `expect(5, "5")`

## Specific error types

- [x] [BUILTIN-LIST-GET] spec: `listGet` and `list[i]` fail with `IndexOutOfBounds { index, length }`
- [x] Prelude declares `IndexOutOfBounds`; checker types list reads with it
- [x] Backend builds the record at the bounds check on GC, ARC and wasm32
- [x] Corpus and rejection programs cover it
- [x] `xs[i]` on a parameter no use has typed yet is a deferred relation, resolved to its container's result at each call

## Backend (`osprey-codegen`)

- [x] Error slot holds a string under discriminant 1, a typed payload box under discriminant 2
- [x] `match` binds the payload at `E`; unknown `E` binds its rendering, never a blind read
- [x] `toString`, `print`, interpolation render a typed payload
- [x] `expectError` compares a record or union payload
- [x] Two `Result`s compare through assertions, by their rendering; `==` on a `Result` is rejected ([ERR-RESULT] rule 5)
- [x] Records, unions and lists print by their literal form ([BUILTIN-PRINT-FORM]), so a typed payload renders at any depth

## Tests

- [x] Corpus regression mixing record, union, int, bool, float, string and nested-`Result` errors through `match`, `?:`, `toString`, records and closures, under GC, ARC and wasm32
- [x] Corpus programs that annotated `Result<_, string>` over a `string` payload state their real `E`
- [x] Corpus assertions that compared values of different types compare one type
- [ ] `make ci` green
