---
layout: page.njk
title: "Persistent Collections in Osprey: Immutable List and Map with Structural Sharing"
excerpt: "Osprey now ships persistent List<T> and Map<K,V> backed by a 32-way bitmapped vector trie and a HAMT — the same data structures that power Clojure and Scala. Append, set and remove are O(log32 n); old versions of a collection stay valid in O(1) extra space."
description: "Explore Osprey’s persistent List and Map, their vector-trie and HAMT internals, structural sharing, complexity, API, and current implementation tradeoffs."
tags: ["blog", "collections", "data-structures", "functional-programming", "language-design"]
author: "Christian Findlay"
readingTime: 8
image: /assets/images/blog/persistent-collections.png
---

Osprey now has first-class persistent collections. **`List<T>` and `Map<K, V>` are immutable**, but "modifying" them is cheap because the runtime shares structure between versions. This post walks through what landed, how it works under the hood, and which papers we leaned on.

## The user-facing surface

`List<T>` is backed by a 32-way bitmapped vector trie with a tail buffer — the same design as Clojure's `PersistentVector` and Scala's `immutable.Vector`. Every operation returns a *new* list that shares almost all of its nodes with the old one:

```osprey
let xs  = listAppend(listAppend(List(), 10), 20)
let xs1 = listAppend(xs, 30)
let xs2 = listAppend(xs, 99)

listLength(xs)   // 2  — xs is untouched
listLength(xs1)  // 3  — extended with 30
listLength(xs2)  // 3  — independent branch with 99
```

`Map<K, V>` uses a **Hash Array Mapped Trie** (HAMT) per Phil Bagwell's 2000 paper, [*Ideal Hash Trees*](https://lampwww.epfl.ch/papers/idealhashtrees.pdf). Each internal node holds a 32-bit bitmap of present slots plus a packed array of children — no wasted pointer slots — so lookup, insert and remove are O(log₃₂ n) expected:

```osprey
let m  = mapSet(mapSet(Map(), "alice", 25), "bob", 30)
let m1 = mapSet(m, "charlie", 35)
let m2 = mapRemove(m, "alice")

mapLength(m)   // 2  — m is untouched
mapLength(m1)  // 3
mapLength(m2)  // 1
```

Both collections are full participants in the operator algebra:

```osprey
xs + xs1     // O(n+m) list concat via listConcat
m + m1       // right-biased union via mapMerge
```

The `+` operator dispatches on the inferred type — list operands route to `osprey_list_concat`, map operands to `osprey_map_merge`, and integer/string operands stay on the existing fast paths.

## Why structural sharing matters

A naive immutable list copies the whole array on every append: O(n) per write, O(n²) to build a million-element list. That's the kind of complexity that pushes people back to mutable state.

The bitmapped trie does something cleverer. The list is a *tree* with 32-way branching. To append, we copy at most one *path* from the root down to the affected leaf — roughly **log₃₂ n nodes**, each holding 32 slots. For a million-element list, that's 4 levels of 32 pointer copies — ~128 pointer writes per append. The rest of the tree is shared with every previous version of the list.

The same idea drives the HAMT: an insert path-copies one branch down to where the new key lands. Old versions keep pointing to the unmodified subtrees, so they survive every "mutation" without paying for one.

For more background:

- Chris Okasaki, *Purely Functional Data Structures* (Cambridge UP, 1998) — the canonical reference.
- [Wikipedia: Persistent data structure](https://en.wikipedia.org/wiki/Persistent_data_structure) — covers path-copying.
- [Clojure's data structures reference](https://clojure.org/reference/data_structures) — Rich Hickey's adaptation of Bagwell's HAMT.

## Builders and the transient escape hatch

There's a tension in any persistent collection: when you're **building** a large collection from scratch, each step doesn't need to be observable as a persistent snapshot. Path-copying every push is wasted work.

Clojure solves this with [transients](https://clojure.org/reference/transients) — a temporary mutable window you can write into, then "seal" back into a persistent collection. Hickey's own benchmark shows ~34% speedup on a million-element build.

Osprey uses the same trick internally. The runtime exposes a `Builder` type that mutates in place, and the literal lowering (`[1, 2, 3]`) drives a builder behind the scenes. **The transient itself is never exposed to user code**, so the functional surface stays clean.

## Stream fusion still applies

Osprey's iterator family (`map`, `filter`, `fold`, `forEach`) already does compile-time stream fusion in the style of Coutts, Leshchinskiy & Stewart's [*Stream Fusion: From Lists to Streams to Nothing at All*](https://www.cs.tufts.edu/~nr/cs257/archive/duncan-coutts/stream-fusion.pdf) (ICFP 2007). With collections in the iterator family, a chain like:

```osprey
xs |> filter(isEven) |> map(double) |> forEach(print)
```

compiles to a single loop with no intermediate list materialised — the same zero-cost abstraction we already had for ranges, now applied to user-built collections.

## What's tested

Every behaviour above is locked in by tests at two levels:

- **C runtime**: 33 vanilla-C unit tests with `assert()` in [`compiler/runtime/list_tests.c`](https://github.com/Nimblesite/osprey/tree/main/compiler/runtime/list_tests.c) and [`compiler/runtime/map_tests.c`](https://github.com/Nimblesite/osprey/tree/main/compiler/runtime/map_tests.c). These cover empty edges, trie-level transitions at 32 / 33 / 1024 / 1025 elements, a 10 000-element stress, hash collisions, structural-sharing invariants ("mutate one version, the others stay intact"), and equivalence between builder construction and incremental `append`.

- **End-to-end Osprey programs**: 12 `.osp` files in [`examples/tested/basics/lists/`](https://github.com/Nimblesite/osprey/tree/main/examples/tested/basics/lists). Each one actually runs through the JIT and its `stdout` is byte-compared against a checked-in `.expectedoutput`. Any regression in length, value, iteration order or persistence breaks the build.

## What's deferred

A few items from the [collections plan](https://github.com/Nimblesite/osprey/tree/main/docs/plans/collections.md) are explicitly deferred:

- **`[head, ...tail]` and subset map patterns** in `match` — requires grammar additions and a parser regeneration cycle.
- **List comprehensions** (`[x * x for x in xs]`) — same grammar pipeline.
- **Names collapsing to `length` / `contains`** without a `list`/`map` prefix — waiting on universal function call syntax (UFCS), which is in flight.

The plan document tracks each of these with its specific blocker.

## Try it

The collections runtime ships in `0.2.0`. Install it on macOS or Linux:

```bash
brew install nimblesite/tap/osprey
```

…and the [playground](/playground/) supports the new builtins too. Build something stateful — a route table, an event log, an undo stack — and see how the persistent semantics change the shape of the code.
