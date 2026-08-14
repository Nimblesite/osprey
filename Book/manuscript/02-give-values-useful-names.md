# Chapter 2 — Give values useful names

## Reader outcome

Use strings, booleans, numbers, parameters, interpolation, and immutable bindings to explain a small calculation without mutable bookkeeping.

## Flight Log state

Chapter 1 prints one launch line. This chapter derives that line from a learner profile and a project summary, with names that describe the problem rather than the storage type.

## Core sections

1. A value is something the program can use
2. A binding gives a stable value a useful name
3. Parameters let one function work with different inputs
4. Interpolation turns values into a readable boundary
5. Checked integer arithmetic introduces a visible `Result`
6. Expressions keep the transformation small
7. Under the wing: immutability and referential transparency

## Compiler-feedback exercise

Attempt to reassign an ordinary immutable binding. Use the actual checker output to distinguish “create a new value” from handler-owned mutation, without introducing effects early.

## Flight Log checkpoint

Add a pure `summary` function that receives project data and produces one string. Verify two inputs without adding global state.

## Planned visuals

- Named-value graph
- Expression in → value out

## Source map

`0002-LexicalStructure`, `0003-Syntax`, `0004-TypeSystem`, `0013-ErrorHandling`

