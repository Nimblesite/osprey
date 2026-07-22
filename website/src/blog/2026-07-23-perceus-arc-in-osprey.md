---
layout: page.njk
title: "Perceus ARC in Osprey: Functional Memory Management"
excerpt: "Osprey ships opt-in Perceus automatic reference counting. The compiler inserts ownership operations, and the runtime reclaims values as owner counts reach zero."
description: "Osprey ships opt-in Perceus automatic reference counting. The compiler inserts ownership operations, and the runtime reclaims values as owner counts reach zero."
date: 2026-07-23
tags: ["blog", "perceus", "automatic-reference-counting", "memory-management", "functional-programming", "compiler"]
author: "Christian Findlay"
readingTime: 10
image: /assets/images/blog/perceus-arc-in-osprey.png
imageAlt: "Perceus automatic reference counting shown as shared memory blocks, ownership links, and reclaimed blocks entering a reuse pool"
---

# Perceus ARC in Osprey: Functional Memory Management

*By Christian Findlay · 23 July 2026*

I want Osprey to make immutable functional programming feel cheap. That promise gets harder when lists, maps, records, unions and closures all live on the heap. If the compiler allocates every new value but never knows when the last owner disappears, a long-running program eventually pays for every value it has ever created.

Osprey now ships an opt-in `--memory=arc` backend based on **Perceus automatic reference counting**. The compiler tracks ownership, inserts the required duplicate and drop operations, and transfers existing owners instead of creating unnecessary reference-count traffic. When the final owner disappears, the runtime reclaims the value and walks supported child layouts using layout metadata.

This first release implements the Perceus ownership core and several useful move optimisations. It does not yet implement the paper's compiler-guided reuse analysis or its full functional-but-in-place story. I also added a conservative tracing collector recently behind the same `--memory` boundary. That deserves its own post. This one is about why Perceus fits Osprey and how the ARC implementation actually works.

## Why memory management belongs below the language

I made one decision early: an Osprey program should never observe memory reclamation.

You cannot attach a finalizer to a value. You cannot inspect its address. You cannot write code that depends on destruction order or timing. A conforming implementation can reclaim a value at its last use, later in the run, or at process exit without changing what your program does. The [memory management specification](/spec/0018-memorymanagement/) makes that contract explicit.

That gives the compiler room to choose the right mechanism. Osprey emits one backend-neutral LLVM IR with calls to a small allocation, retain and release interface. The linker selects the runtime archive. Under ARC, those retain and release hooks update exact counts. The default and tracing collector archives leave those two hooks as no-ops.

You write the same source either way. Memory management stays an implementation concern.

## The academic foundation: Perceus

The design comes from Alex Reinking, Ningning Xie, Leonardo de Moura and Daan Leijen's PLDI 2021 paper, [*Perceus: Garbage Free Reference Counting with Reuse*](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/06/perceus-pldi21.pdf). The authors developed Perceus for Koka and grounded it in a linear resource calculus called `λ₁`. Osprey adapts the ownership and `dup`/`drop` insertion discipline from their [extended technical report](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf).

The central idea is simple enough to state without the calculus:

- A produced heap value starts with one owner.
- If code needs another owner, the compiler inserts `dup`.
- A final use consumes or transfers its owner. If a path does not use that owner, the compiler inserts `drop` as soon as it becomes dead.
- A function can transfer an owner to its caller instead of duplicating and then dropping it.

The formal system treats an owned resource as something the program must consume exactly once. Perceus then gives the compiler syntax-directed rules that insert duplicates late and drops early. That placement matters. Basic reference counting can spend a lot of time incrementing and decrementing counts that cancel each other. Precise ownership information lets the compiler avoid much of that work.

Perceus builds on earlier work from Sebastian Ullrich and Leonardo de Moura, [*Counting Immutable Beans: Reference Counting Optimized for Purely Functional Programming*](https://www.microsoft.com/en-us/research/publication/counting-immutable-beans-reference-counting-optimized-for-purely-functional-programming/). That Lean research explored exact counts, borrowed references and destructive updates when a value has one owner. Perceus turned those ideas into a systematic translation with a proof for its cycle-free core.

The paper uses **garbage-free** in a precise academic sense. In its cycle-free calculus, at each intermediate state after any immediately pending duplicate or drop operation, every heap object is reachable from the current computation. It does not mean zero allocation or zero bookkeeping.

The second half of the Perceus story uses dynamic uniqueness. If a pattern match consumes a uniquely owned constructor, the program can reuse that storage for a new constructor of the same size. Shared inputs keep persistent copy semantics. Unique inputs can update in place. The authors call this **functional but in-place**, or FBIP.

That reuse work matters, but I want to draw a hard line around the current implementation: Osprey ships the ownership and `dup`/`drop` foundation today. Compiler-guided constructor reuse remains future work.

## How automatic reference counting works in Osprey

Consider a small value that stores the same string twice:

```osprey
type Names = Names { left: string, right: string }

fn duplicateName(input: string) -> Names = {
    let name = trim(input)
    Names { left: name, right: name }
}
```

You never annotate `name` as owned or borrowed. The compiler sees a produced string, two owning fields and a returned aggregate. It maintains the ownership ledger for you.

The ARC rules work like this:

1. `trim(input)` produces a string with one owner.
2. Each heap field that keeps a shared value acquires its own owner.
3. The local `name` owner drops as the function exits, after its final use.
4. The `Names` value transfers its existing owner to the caller on return.
5. When the final owner of `Names` disappears, the runtime releases both child fields. The string disappears when its own final count reaches zero.

The important part is what does not happen. The return path does not retain the `Names` value and immediately release the function's copy. The compiler moves that existing `+1` out of the function.

## What the compiler actually emits

The implementation lives in Osprey's [Perceus ownership ledger](https://github.com/Nimblesite/osprey/blob/main/crates/osprey-codegen/src/arc.rs). It threads ownership information through the normal AST-to-LLVM walk rather than adding a second ARC-only compiler pipeline.

The compiler assigns every tracked managed-pointer owner a null-initialised slot in the function entry block. If a branch never produces the value, the compiler leaves that slot null. Region cleanup can therefore load and release the slot safely without solving a separate dominance problem for every match arm and loop edge.

On top of the basic producer, duplicate and drop rules, the compiler already removes several common pairs:

- **Fresh constructor moves:** a fresh unnamed owner can move straight into a constructor field.
- **Match join moves:** when every possible incoming arm produces a fresh owner, that owner can move through the LLVM `phi` at the join.
- **Return transfers:** a function moves an owner it already holds to its caller.
- **Last-use drops:** region cleanup and function-level liveness release a named top-level value after its final syntactic use instead of always waiting for the whole function to end.
- **LLVM allocation elimination:** LLVM can remove some fresh, non-escaping allocation and unique-release pairs, especially scalar `Result` blocks.

The generated IR always names the same hooks. `--memory=arc` simply links the ARC runtime behind them. That boundary keeps the language, type checker and optimiser independent from the selected memory backend.

## Inside the ARC runtime

The [ARC runtime](https://github.com/Nimblesite/osprey/blob/main/compiler/runtime/memory_arc.c) puts a 16-byte header before every managed allocation:

```c
{ int64_t meta; int32_t rc; uint32_t size; }
```

`rc` stores a positive owner count, while negative values mark immortal allocations. `size` records the body size. `meta` encodes a layout kind and, for masked layouts, which body words may hold managed pointers. Code generation and the collection runtimes supply those layouts, while the allocation registry distinguishes ARC objects from static and foreign pointers.

Retain and release first probe a registry of Osprey allocations. A null pointer, static string or foreign pointer misses that registry and becomes a safe no-op. When a count reaches zero, an iterative worklist releases the object's managed children. Deep trees therefore do not consume the C call stack while they collapse.

Freed blocks enter an exact size-class pool when they fit. A later allocation can recycle one of those blocks. This is useful allocator engineering, but it is not Perceus reuse analysis. The pool recycles storage after an object has died. The paper's reuse pass pairs a specific dying match scrutinee with a same-sized constructor and uses dynamic uniqueness to reuse it directly.

## Why reference counting fits functional programming

Reference counting has one famous weakness: cycles. Osprey avoids that problem by construction.

Heap values are immutable, so an older value cannot later gain a pointer to a younger value. `mut` rebinds a name to a new value; it does not rewrite an existing heap object. The value graph stays acyclic, which means a count reaching zero can reclaim dead subgraphs for supported precise layouts without a cycle collector.

Persistent collections make the ownership model especially concrete. Two versions of a list or map can share most of their internal nodes. Exact counts record that sharing. When one version dies, the runtime releases only the spine that version stopped owning. Nodes shared with another live version keep a positive count.

This is the part I like most. Immutability gives you simple source semantics, and the compiler turns the resulting ownership structure into ownership-aware runtime behaviour. You get persistent values without asking every object to survive until the end of the process.

## What the current measurements say

I do not want to turn one benchmark into a universal performance claim. Still, the committed [Osprey benchmark run](/benchmarks/) shows why this work matters.

On `binarytrees`, the default allocator reached **603.8 MiB** of peak memory and completed in **248.7 ms**. The ARC build reached **2.8 MiB** and completed in **216.4 ms**. ARC reclaimed the short-lived tree nodes throughout the run, and its bookkeeping did not slow this particular result.

Different programs create different ownership patterns, so you should read those numbers as one reproducible data point. The useful result is that the implementation now gives us a real way to measure reference-counted reclamation across the benchmark suite instead of reasoning from a design document.

## Status, honestly

Osprey currently ships:

- The Perceus producer, duplicate and drop discipline.
- Region-end and function-level last-use drops.
- Fresh constructor, match join and return-owner moves.
- Compiler-supplied layouts for generated records, unions, `Result` blocks, closures and flat list headers, plus runtime-supplied layouts for persistent collection nodes. Runtime-created strings use raw managed allocations.
- A non-recursive drop walk, allocation registry, leak diagnostics and size-classed block pool.

The inline pointer mask currently covers the first 56 body words. If an aggregate has managed fields beyond that range, the compiler falls back to a raw layout and retains those children rather than risking an invalid release.

Osprey does not yet ship:

- Selective borrow inference.
- Perceus drop specialization.
- Compiler-guided reuse, including drop-guided reuse and reuse specialization.
- An FBIP guarantee.

The distinction matters. The [Perceus paper](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse-2/) proves soundness and garbage-freedom for the syntax-directed duplicate-and-drop translation of its cycle-free core. It does not formally verify the Koka implementation or every optimisation and reuse transformation. Osprey adapts the discipline to a real LLVM compiler, persistent containers, effects and foreign pointers. Our tests give strong implementation evidence, but they do not inherit the paper's proof automatically.

At the time of writing, the code-generation suite passes 74 tests, and the C ARC test binary executes 600,975 assertions across allocation, layout, ownership, pooling and failure paths. The differential suite also runs the same Osprey programs through each backend and compares their output. ARC remains opt-in while I build the independent precise oracle I want before making ownership-based reclamation the default.

The next academic step is also clear. Anton Lorenzen and Daan Leijen's [*Reference Counting with Frame Limited Reuse*](https://www.microsoft.com/en-us/research/publication/reference-counting-with-frame-limited-reuse/) introduces drop-guided reuse and a frame-limited bound. Earlier reuse could react poorly to small program transformations and increase peak heap use without a bound. That is the direction I want to investigate when Osprey moves beyond the current `dup`/`drop` tier.

## Try it

Build and run the same Osprey program with Perceus ARC:

```bash
osprey app.osp --run --memory=arc
```

You do not add lifetime syntax. You do not call `free`. You do not change the program's types. The compiler accounts for ownership, and the runtime reclaims a value when its last owner goes away.

That is the memory model I wanted for Osprey: functional at the surface, explicit inside the compiler and governed by ownership counts at runtime. Try it on a program that builds persistent data, then watch what happens to its peak memory.
