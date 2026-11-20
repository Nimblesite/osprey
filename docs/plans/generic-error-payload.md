# Generic error payloads

Make `Result<T, E>` hold its error at type `E`, end to end, as [ERR-RESULT](../specs/language/0013-ErrorHandling.md#resultt-e--err-result) states. Before this work the checker pinned `Error { message }` to `string` and left `E` unconstrained, and the backend stored only a C string in the error slot (a number payload was silently dropped).

## Rules being implemented

- [ERR-RESULT]: `T` and `E` are both generic, inferred, exact and must agree.
- [ERR-PAYLOAD]: `Error { message }` binds the producer's value at type `E` on GC, ARC and wasm32.
- [ERR-RESULT-FORMAT]: `Error(<payload>)` renders the payload at its own type.
- [TESTING-BUILTIN-EXPECT], [TESTING-BUILTIN-CHECK], [TESTING-BUILTIN-EXPECT-ERROR], [TESTING-EQUALITY]: assertion operands share one inferred type; `expectError` takes the Result's own `E`.
- There is no built-in `Error` type; `Error` is only the `Result` constructor. Each fallible built-in states its own `E`, which is `string` for all of them today.

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
- [ ] `Error` is not a type: built-ins return `Result<T, string>`; `Error` in a type position is rejected
- [ ] Rejections in `src/osprey/examples/failscompilation/`: wrong `E` returned, wrong `expectError` payload type, `expect(5, "5")`

## Backend (`osprey-codegen`)

- [x] Error slot holds a string under discriminant 1, a typed payload box under discriminant 2
- [x] `match` binds the payload at `E`; unknown `E` binds its rendering, never a blind read
- [x] `toString`, `print`, interpolation render a typed payload
- [ ] `expectError` compares a record or union payload
- [ ] Structural equality of two `Result`s compares typed payloads

## Tests

- [ ] Corpus regression mixing record, union, int, bool, float, string and nested-`Result` errors through `match`, `?:`, `toString`, records and closures, under GC, ARC and wasm32
- [ ] Corpus programs that annotated `Result<_, string>` over a `string` payload state their real `E`
- [ ] Corpus assertions that compared values of different types compare one type
- [ ] `make ci` green
