# Generic error payloads

Make `Result<T, E>` hold its error at type `E`, end to end, as [ERR-RESULT](../specs/language/0013-ErrorHandling.md#resultt-e--err-result) states. Before this work the checker pinned `Error { message }` to `string` and left `E` unconstrained, and the backend stored only a C string in the error slot (a number payload was silently dropped).

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

## Gaps found while testing

Each is a defect against a top-tier language, seen with a concrete program on this branch. None may be dropped; tick only with a corpus regression under GC, ARC and wasm32.

### Error system

- [ ] `expectError` on an inline `if` needs an annotation: `expectError(if c { Error { message: DivZero } } else { Success { value: 1 } }, DivZero)` fails with "expectError on a Result whose error type is not decided here; annotate it". The checker knows `E`; the backend must take it from the checked type instead of the operand's syntax.
- [ ] Vague assertion messages: a failed `expectError` on a record or union payload prints "expected Error(<the given MathError>), got a Success or an Error holding another MathError". Records and unions now render ([BUILTIN-PRINT-FORM]), so it must print both values: `expected Error(DivZero), got Error(Overflow { by: 3 })`. Same for `expect` and `check`.
- [ ] A union variant holding an unprintable value is accepted by `print`: `print(Held { m: someMap })` compiles and shows `Held { m: <map> }`, while the same value in a record field is rejected. User types reach the constraint as a name, so the check must read the declaration and reject it at compile time.
- [ ] A quoted string inside a record, variant or list is not escaped: `{ why: "a"b" }` is ambiguous. Escape `"`, `\` and control characters as the string literal syntax does.
- [ ] `Map` has no rendering. Decide and specify one (`{ "k": 1 }`, key order defined) or keep the rejection and say why in [BUILTIN-PRINT].

### Language

- [ ] No type classes (constrained polymorphism). "Printable", "comparable", "numeric" and "sized" are ad hoc checks on resolved types in `builtin_constraints.rs`; a generic function loses the obligation, so `fn show(x) = toString(x)` is checked only where a concrete type reaches it, and a user cannot state or extend a constraint. Design constraint-carrying type schemes ([TYPE-GENERICS]) so the obligation travels with the function's type, and move the built-in constraints onto them.
- [ ] No nested constructor patterns ([TYPE-UNION-POSITIONAL]): `Node(Node(l, _), r)` and `Success { value: Some { v } }` are rejected; each slot is a binding or `_`, forcing nested `match`. Implement nested patterns with exhaustiveness checking across the nesting.
