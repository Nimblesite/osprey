# Chapter 3 — Let the compiler work out the types

## Reader outcome

Read common Osprey types, rely on inference for ordinary code, and treat a type mismatch as a precise disagreement rather than a mysterious crash.

## Flight Log state

The summary function works for one shape of data. The reader introduces a small record boundary and lets field use determine the surrounding function types.

## Core sections

1. A type describes possible values
2. Literals and operations leave an inference trail
3. Function inputs and outputs constrain one another
4. Add an annotation only when it adds information
5. Reuse a function at more than one inferred type
6. Polymorphism is not `any`
7. Read a mismatch from the source location outward

## Compiler-feedback exercise

Supply a boolean where the program builds text. Capture the pinned compiler's location and expected/observed types; do not paraphrase an invented diagnostic.

## Flight Log checkpoint

Introduce a `Learner` record and a function whose parameter and return types remain inferred.

## Planned visuals

- Inference trail from literal to call
- Useful versus redundant annotation
- Type-mismatch locator

## Source map

`0003-Syntax`, `0004-TypeSystem`, `0005-FunctionCalls`

