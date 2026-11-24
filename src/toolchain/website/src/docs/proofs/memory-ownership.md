---
layout: page
title: How Osprey proves its memory code is correct
description: "A plain-English tour of how Lean proofs check the code Osprey's compiler emits for compile-time memory ownership: what is proved, the real Lean, and the broken versions CI must reject."
permalink: /docs/proofs/memory-ownership/
---

**A program has to give back the memory it borrows from the computer.** If it gives memory back too early, the program reads garbage. If it gives the same memory back twice, the program can crash or be attacked. If it never gives it back, memory fills up. Osprey is safe, elegant and fast, and its `--memory=ownership` mode handles this at compile time: the compiler works out the single moment each piece of memory is no longer needed and adds the instruction that frees it.

Each piece of memory has exactly one **owner** at a time, the way each library book is checked out to exactly one person. When the owner is finished, the book goes back once, and nobody can still be reading it.

The compiler does this automatically, so you have to trust it got every case right. Tests can only try some cases. Osprey goes further for the most important pieces: it uses **Lean**, a proof checker, to prove that the code the compiler actually produced follows the rules **for every possible input**.

## Two kinds of proof

1. **The rules are sound.** A small model of the rules ("one owner", "you can't free a book someone borrowed") is proved never to go wrong. The model lives in [OwnershipLedger.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLedger.lean).
2. **The compiler's real output follows the rules.** The compiler compiles a real Osprey program, and the result is turned into Lean and proved correct. This is the part this page explains.

## Proof 1: the rules themselves

The model tracks every piece of memory as a `Cell`: whether it is alive, who owns it and who has borrowed it. Every action (create, move, lend, free, read) either succeeds or is refused. Here is one real theorem:

```lean
theorem lent_pins_owner {c lent : Cell} {field owner : Slot}
    (accepted : step c (.borrow field owner) = some lent) (destination : Slot) :
    step lent (.free owner) = none ∧ step lent (.move owner destination) = none
```

**In plain English:** once someone has borrowed a value, the owner can neither free it nor hand it to someone else. Both attempts are refused (`none`). This rules out the "freed while someone is still reading it" bug in every case.

The main theorem, `from_empty`, says that whatever sequence of allowed steps a program takes from an empty memory, every live value still has exactly one owner, and every borrow points at that owner. Eighteen more theorems cover moving, lending, taking a field out of a record and freeing.

To show the proofs are not true by accident, CI weakens the rules three ways: it allows a free while a value is lent, a move while it is lent, and a free by someone who is not the owner. Each weakened version must make a proof fail.

## Proof 2: the code the compiler really produced

### Step 1: compile a real program

The proof starts from ordinary Osprey in [single_owner.test.osp](https://github.com/Nimblesite/osprey/blob/main/src/osprey/tests/regressions/ownership/single_owner.test.osp):

```osprey
fn grow(xs, x) = listAppend(xs, x)
fn growWhen(xs, x, enabled) = if enabled { listAppend(xs, x) } else { xs }
```

`grow` builds a new list with one more item, so the old list must be freed exactly once. `growWhen` only grows the list when `enabled` is true, so the compiler must free the old list on one path and keep it on the other.

The compiler is run with `--memory=ownership --llvm` and prints the low-level code (LLVM IR) it would really ship. For `grow` on a list of numbers, the important lines are:

```llvm
%r0 = call i8* @osprey_list_append_of(i8* %$p0, i64 %$p1, i32 0)
call void @osp_ownership_free(i8* %$p0)
ret i8* %r0
```

That is: make the new list, free the old one, return the new one.

### Step 2: translate it into Lean, line by line

A translator, [ownership-lowering.mjs](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-lowering.mjs), reads that output and turns each instruction into a Lean instruction. If it meets an instruction it doesn't recognise, it **stops with an error**, so nothing the compiler did can be silently skipped. The `grow` function above becomes:

```lean
def emitted : List Instr :=
[.reserve 0, .store 0 none, .reserve 1, .store 1 none, .store 1 (some 0),
 .store 1 none, .append 0 1, .free (some 0), .store 0 (some 1), .ret 1]
```

Nobody types this list. It is generated from the compiler's output on every run, so if the compiler changes, the list changes too.

### Step 3: run it on a strict pretend computer

[OwnershipLowering.lean.in](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLowering.lean.in) defines a tiny pretend computer, `execute`, that runs those instructions and refuses (`none`) on any memory mistake:

```lean
  | .free (some reg) :: rest =>
    if (s.heap.lookup reg).isSome then
      execute element {s with heap := s.heap.filter (fun entry => entry.1 != reg)} rest
    else none
  | [.ret reg] =>
    match s.heap with
    | [(owner, result)] => if owner == reg then some result else none
    | _ => none
```

**In plain English:**

- You may only free memory that is still alive. Freeing something twice, or something that never existed, is refused.
- When the function returns, exactly one piece of memory may be left, and it must be the one returned. Anything else left over is a leak and is refused.
- Reading a slot must give the value the compiler expected. A stale read is refused.

### Step 4: prove it for every input

```lean
theorem compiledGrow (xs : List α) (x : α) :
    execute x ⟨[(0, xs)], []⟩ emitted = some (xs ++ [x]) := by rfl

theorem compiledGrowWhen (xs : List α) (x : α) (enabled : Bool) :
    execute x ⟨[(0, xs)], []⟩ (conditional enabled) =
      some (if enabled then xs ++ [x] else xs) := by cases enabled <;> rfl
```

**In plain English:** `compiledGrow` says that for **any** list `xs` and **any** item `x`, the code the compiler produced returns the right answer (`xs ++ [x]`) and never makes a memory mistake along the way. "Any" really means any: it covers lists of every length and every element type, which no test suite can try one by one. `compiledGrowWhen` proves the same for both values of `enabled`, so the old list is freed on the path that grows it and kept on the path that doesn't.

### Step 5: the C runtime as well

The compiled program calls helper functions written in C, such as the one that appends to a list. Those are compiled to the same low-level form and proved too:

| Theorem | What it checks |
| --- | --- |
| `runtimeRootOrigins` | The list append, list prepend and map set helpers return memory that is newly made or a value they were given, never a third pointer that might belong to someone else. |
| `renderedStringOrigin`, `stringResultOrigins` | Building a string and 15 string operations always return a string the caller owns, not one borrowed from somewhere else. |
| `compiledFreeNull` | Freeing "nothing" (a null pointer) is safely ignored. |
| `compiledPairFields` | A record with two fields stores both children, and the code that frees it visits field 1 and then field 2, once each. |
| `compiledChildDrop` | When a record is freed, each child is handled correctly in every state its header can be in. |

### Step 6: double-check with Osprey's own checker

Lean checks every proof first. Then each theorem is exported and checked a second time by **Osprey's own proof checker**, written in Osprey, built three ways: with garbage collection, with reference counting, and as WebAssembly. All three must print `axioms: none`, which means the proof uses no unproved assumptions.

### Step 7: break the code on purpose

A proof that still passes after the code breaks proves nothing. So CI deliberately breaks the compiler's output and the C runtime, and each broken version **must** fail its proof:

| Broken version | What was changed | Proof that must fail |
| --- | --- | --- |
| `leak` | the free is deleted | `compiledGrow` |
| `double-free` | the old list is freed twice | `compiledGrow` |
| `early-free` | the old list is freed before it is copied | `compiledGrow` |
| `stale-phi` | the `if` joins pick up the wrong list | `compiledGrowWhen` |
| `alias` | the list helper returns a pointer to someone else's list | `runtimeRootOrigins` |
| `borrowed-string` | the string builder returns a borrowed string | `renderedStringOrigin` |

For example, the `leak` version is the same list with `.free (some 0)` missing:

```lean
[.reserve 0, .store 0 none, .reserve 1, .store 1 none, .store 1 (some 0),
 .store 1 none, .append 0 1, .store 0 (some 1), .ret 1]
```

Two lists are now alive at the return, so the pretend computer refuses, and Lean reports:

```text
error: Tactic `rfl` failed: The left-hand side
  execute x { heap := [(0, xs)], slots := [] } emitted
is not definitionally equal to the right-hand side
  some (xs ++ [x])
```

The script also checks that the error is in the theorem that broken version should break, not somewhere else. More broken versions cover the record and child-freeing code: a missing child free, a child queued twice, a field left out of the free list and an inverted check.

## Every proof runs in CI

All of this is part of the required `CI` check on every pull request that touches code. The `Tests: proof kernel and Lean speed gate` job runs `make _test_proof_ci`, which runs `make _test_ownership_proof`. That runs both scripts:

- [ownership-ledger.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-ledger.sh): every rule theorem, checked by Lean and by Osprey's checker on all three builds, plus the three weakened rule sets.
- [ownership-lowering.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-lowering.sh): compiles `single_owner`, translates it, and proves every theorem above, plus every broken version.

`make ci` runs the same target locally. If the compiler's output changes so that a proof no longer holds, the pull request goes red.

## What is not proved yet

These proofs are real, but they cover specific functions, not the whole compiler:

- They prove the code produced for `grow`, `growWhen`, the record constructor and the runtime helpers listed above. They do not yet prove the compiler gets **every** program right.
- Nothing yet proves that the compiler's output, in general, follows the rules proved in the first part. That link is open work in [plan 0045](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0045-compile-time-ownership.md).
- Borrow lifetimes, the memory allocator itself and the meaning of lists and maps are not yet proved.

For every other program, the evidence comes from tests: the test programs run under `--memory=ownership` with freed memory scribbled over and under AddressSanitizer, and each must finish with zero objects left alive. The [memory management specification](/spec/0018-memorymanagement/) lists each proof and its assumptions.

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
