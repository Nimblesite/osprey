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
