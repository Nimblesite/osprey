# Chapter 13 — Choose another flavor when it helps

## Reader outcome

Translate one understood program between Default and ML, identify the differences that affect function shape, and prove unchanged behavior.

## Flight Log state

The complete Default source remains canonical for the book. One pure file gains an optional `.ospml` twin and shares the same output and tests.

## Core sections

1. Flavor changes the source surface, not the language you have learned
2. Default remains the safe starting point
3. ML replaces braces with layout and defaults to currying
4. Flat and curried calls are not punctuation twins
5. One file selects one surface
6. Use an agent for translation, then check and test
7. The architecture remains open to future flavors

## Compiler-feedback exercise

Translate a flat two-argument function as though it were curried, observe the real mismatch, and repair the source while preserving the function's intended shape.

## Flight Log checkpoint

Create an ML twin of one pure module, run both checks, and compare stdout or test evidence byte-for-byte.

## Planned visuals

- Several source feathers converging on one checked core
- Agent translation and verification loop
- Flat versus curried application

## Source map

`0023-LanguageFlavors`, `0024-MLFlavorSyntax`, cross-flavor AST and IR equivalence tests

## Edition note

Never say Osprey is permanently limited to two flavors. Name Default and ML as the currently available surfaces and keep future additions structurally possible.

