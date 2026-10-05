# Chapter 5 — Move a collection through a pipeline

**Chapter outline.** The teaching plan below has been checked against the current collection APIs; the full lesson and checkpoint are still to be written.

## Reader outcome

Build lists and maps, preserve earlier versions, and describe range-iterator work with `map`, `filter`, `fold`, and `|>`. Keep `Iterator<T>` distinct from a materialized `List<T>`.

## Flight Log state

One entry becomes a list of entries. The reader visits entry indices with a range iterator, handles checked list lookups, and builds a compact summary of active work.

## Core sections

1. A list groups values of one type; indexing returns `Result`
2. Persistent updates keep the old value valid
3. A map connects string keys to values; traversal order is unspecified
4. A pipeline reads in transformation order; `range` produces an iterator
5. Iterator `map` transforms and `filter` keeps
6. Iterator `fold` combines; `forEach` visits an iterator and `forEachList` visits a list
7. Named callbacks before lambdas

## Compiler-feedback exercise

Use a callback with the wrong return shape inside a pipeline. Follow the type relationship from the callback to the consumer.

## Flight Log checkpoint

Traverse entry indices in order, handle each checked lookup, filter active work, and produce a deterministic summary without a mutable loop. Do not pass a list directly to the iterator combinators or assume an order for map keys.

## Planned visuals

- Pipeline flow
- Persistent structural sharing
- Map/filter/fold job comparison

## Source map

`0004-TypeSystem`, `0010-LoopConstructsAndFunctionalIterators`, `0012-Built-InFunctions`
