# Chapter 10 — Read, write, and call the web

## Reader outcome

Complete one native boundary workflow while decoding outside data early and keeping every expected failure visible.

## Flight Log state

The in-memory project saves to or loads from a deterministic fixture. A web variant is shown only where the current native runtime supports it.

## Core sections

1. The outside world is allowed to be unreliable
2. Keep a pure model behind a narrow boundary
3. Read or request data through a `Result`
4. Decode before the data reaches the core
5. Log presence and shape, never secrets
6. State native and WebAssembly support separately
7. Treat C calls as an explicit safety boundary

## Compiler-feedback exercise

Try to use a fallible boundary result as decoded data. Add the missing match and retain the original error value.

## Flight Log checkpoint

Persist or fetch one fixed fixture and prove the pure summary is unchanged after decoding.

## Planned visuals

- Pure core and impure shell
- Boundary validation path
- Platform support map

## Source map

`0013-ErrorHandling`, `0014-HTTP`, `0019-ForeignFunctionInterface`, `0022-WebAssemblyTarget`

