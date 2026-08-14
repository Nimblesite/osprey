# Chapter 5 — Move a collection through a pipeline

## Reader outcome

Build lists and maps, preserve earlier versions, and describe collection work with `map`, `filter`, `fold`, and `|>`.

## Flight Log state

One entry becomes a list of entries. The reader filters active work and folds it into a compact summary.

## Core sections

1. A list groups values of one type
2. Persistent updates keep the old value valid
3. A map connects string keys to values
4. A pipeline reads in transformation order
5. `map` transforms and `filter` keeps
6. `fold` combines and `forEach` performs
7. Named callbacks before lambdas

## Compiler-feedback exercise

Use a callback with the wrong return shape inside a pipeline. Follow the type relationship from the callback to the consumer.

## Flight Log checkpoint

Filter active entries, map them to display lines, and produce a deterministic summary without a mutable loop.

## Planned visuals

- Pipeline flow
- Persistent structural sharing
- Map/filter/fold job comparison

## Source map

`0004-TypeSystem`, `0010-LoopConstructsAndFunctionalIterators`, `0012-Built-InFunctions`

