# Generic error payloads

Make `Result<T, E>` hold its error at type `E`, end to end, as [ERR-RESULT](../specs/language/0013-ErrorHandling.md#resultt-e--err-result) states. Before this work the checker pinned `Error { message }` to `string` and left `E` unconstrained, and the backend stored only a C string in the error slot (a number payload was silently dropped).

## 🚨 Top priority: gaps that block top tier

These come before every other open item in `docs/plans/`. Each is a defect against a top-tier language, seen with a concrete program on this branch. None may be dropped or deferred behind new features; tick only with a corpus regression under GC, ARC and wasm32. Work them in this order: language first, since built-in constraints remove the ad hoc checks the error-system items patch around.

### Language

- [ ] **Built-in constraints carried in the type scheme ([TYPE-CONSTRAINTS](../specs/language/0004-TypeSystem.md#built-in-constraints--type-constraints)).** A generic function like `fn show(x) = toString(x)` loses the "x must be printable" obligation, so it is only checked where a concrete type happens to reach it. The fix is Elm-style built-in constraints carried in the function's type (`printable`, `comparable`, `number`, `sized`), with effects, module signatures and records of functions covering everything user-extensible. No type classes: effects pick an implementation by where the code runs, constraints by the type of the value, and Osprey derives the latter structurally.
  - [x] Spec: [TYPE-CONSTRAINTS] written, marked not implemented
  - [ ] Spec: remove the "not implemented" status when the rest lands
  - [ ] Checker: a constrained type variable in `Type::Var`'s scheme; generalization keeps it, instantiation copies it, unification with a concrete type discharges it
  - [ ] Move `is_printable`, `is_sortable_list`, `is_sized` and the numeric operand checks in `builtin_constraints.rs` onto it; delete the post-hoc `validate_builtin_uses` pass for them
  - [ ] Error at the call that decides the type, naming the constraint: "`show` needs a printable argument; a function is not printable"
  - [ ] Hover, inferred signatures and `--docs` show the constraint
  - [ ] Corpus regression (both flavors) and `failscompilation/` rejections for each constraint through one, two and three levels of generic call
- [ ] **Nested patterns ([PATTERN-NESTED](../specs/language/0007-PatternMatching.md#nested-patterns--pattern-nested)).** `Node(Node(l, _), r)` and `Error { message: Overflow { by } }` are rejected today; each slot is a binding or `_`, forcing a `match` per level.
  - [x] Spec: [PATTERN-NESTED] written, marked not implemented
  - [ ] Grammar and both parsers accept a pattern in every slot (`tree-sitter-osprey`, Default and ML lowering to one AST)
  - [ ] Checker: type each nested pattern at its slot's type; one binding per name per arm
  - [ ] Exhaustiveness and reachability over the whole nested pattern, with a witness value in the diagnostic
  - [ ] Backend: decision tree for nested tests on GC, ARC and wasm32; no binding leaks from a failed arm; ARC releases balanced
  - [ ] Corpus regression (both flavors) mixing constructors, `Result`, tuples, lists and rows three levels deep; `failscompilation/` rejections for a non-exhaustive nest, an unreachable arm and a name bound twice
  - [ ] Spec: remove the "not implemented" status and the note in [TYPE-UNION-POSITIONAL]

### Error system

- [x] `expectError` on an inline `if` needs an annotation: `expectError(if c { Error { message: DivZero } } else { Success { value: 1 } }, DivZero)` fails with "expectError on a Result whose error type is not decided here; annotate it". The checker knows `E`; the backend must take it from the checked type instead of the operand's syntax.
- [x] Vague assertion messages: a failed `expectError` on a record or union payload prints "expected Error(<the given MathError>), got a Success or an Error holding another MathError". Records and unions now render ([BUILTIN-PRINT-FORM]), so it must print both values: `expected Error(DivZero), got Error(Overflow { by: 3 })`. Same for `expect` and `check`. Done: assertions compare renderings of any printable operand ([TESTING-EQUALITY]); an operand with no rendering is a compile error.
- [x] A union variant holding an unprintable value is accepted by `print`: `print(Held { m: someMap })` compiles and shows `Held { m: <map> }`, while the same value in a record field is rejected. User types reach the constraint as a name, so the check must read the declaration and reject it at compile time.
- [ ] A quoted string inside a record, variant or list is not escaped: `{ why: "a"b" }` is ambiguous. Escape `"`, `\` and control characters as the string literal syntax does.
- [ ] `Map` has no rendering. Decide and specify one (`{ "k": 1 }`, key order defined) or keep the rejection and say why in [BUILTIN-PRINT].

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
