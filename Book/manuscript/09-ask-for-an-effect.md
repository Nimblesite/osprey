# Chapter 9 — Ask for an effect

## Reader outcome

Declare a typed effect, perform an operation, and install a handler without passing service objects through pure functions.

## Flight Log state

The project wants to log progress. Its core asks to log; production and test handlers answer differently.

## Core sections

1. Keep decisions pure and outside work visible
2. Declare the operation's inputs and output
3. `perform` makes a typed request
4. A lexical handler supplies behavior
5. Missing handlers are rejected before program entry
6. Replace behavior in tests without changing the caller
7. Native-only resumption and current limits
8. Under the wing: first-class algebraic effects

## Compiler-feedback exercise

Remove the required handler and capture the pinned missing-handler diagnostic. Restore the narrowest handler rather than hide the effect.

## Flight Log checkpoint

Add a logging effect with a production print handler and a deterministic test handler.

## Planned visuals

- Pure request and handler boundary
- Direct-style call path
- Production/test handler swap
- Missing-handler gate

## Source map

`0001-Introduction`, `0017-AlgebraicEffects`, effect corpus and failure fixtures

