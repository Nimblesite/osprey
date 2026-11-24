---
layout: page
title: What is a proof, and how does it check Osprey code?
description: "A plain-English guide to proofs: what a proof is, how it differs from a test, and how a Lean proof checks the exact code Osprey's compiler produced, step by step, using memory ownership as the example."
permalink: /docs/proofs/how-proofs-check-code/
---

## A test checks some cases. A proof checks all of them.

Say you write a function that adds one item to a list. A **test** runs it on a few lists you pick, like `[1, 2]` and `[]`, and checks the answers. If those pass, you hope the others work too. But there are endless lists, and you only tried a few.

A **proof** is an argument that the code is right for **every** list, of every length, holding anything. A person could write that argument on paper, but people make mistakes. So we write it for a program called **Lean**, a proof checker. Lean checks every step of the argument. If a single step is wrong, Lean refuses the whole proof.

So a proof is like a test that has tried every possible input at once.

## The example: giving memory back

When an Osprey program makes a list, it borrows memory from the computer. Later it has to give that memory back. This is called **freeing** it. Three things can go wrong:

- **Leak:** the memory is never given back, so the program slowly fills up memory.
- **Double free:** the same memory is given back twice, which can crash the program or let an attacker in.
- **Use after free:** the program gives the memory back and then reads it anyway, and gets garbage.

In Osprey's `--memory=ownership` mode, the **compiler** decides where every free goes. You never write one. That is convenient, but it means you have to trust the compiler got it right. The proof is how we check.

Here is the Osprey code we will check, from [single_owner.test.osp](https://github.com/Nimblesite/osprey/blob/main/src/osprey/tests/regressions/ownership/single_owner.test.osp):

```osprey
fn grow(xs, x) = listAppend(xs, x)
```

`grow` makes a **new** list that is the old list plus one item. The old list is no longer needed, so it must be freed **exactly once**, **after** the new list is made. Only the new list may still be alive when `grow` returns.

## How the proof checks it, step by step

### 1. Compile the real code

The compiler turns `grow` into machine-level instructions, the code that really runs. These are the important lines it produces:

```llvm
%r0 = call i8* @osprey_list_append_of(i8* %$p0, i64 %$p1, i32 0)
call void @osp_ownership_free(i8* %$p0)
ret i8* %r0
```

In words: make the new list, free the old list, return the new list.

### 2. Turn that code into Lean

A small translator, [ownership-lowering.mjs](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-lowering.mjs), reads those instructions and writes the same steps in Lean:

```lean
def emitted : List Instr :=
[.reserve 0, .store 0 none, .reserve 1, .store 1 none, .store 1 (some 0),
 .store 1 none, .append 0 1, .free (some 0), .store 0 (some 1), .ret 1]
```

`.append 0 1` means "make list 1 from list 0". `.free (some 0)` means "free list 0". `.ret 1` means "return list 1".

Two things matter here:

- **Nobody types this list.** It is generated from the compiler's real output every time. If the compiler changes, the list changes.
- **Nothing is skipped.** If the translator meets an instruction it doesn't understand, it stops with an error instead of guessing.

### 3. Write down the rules

Next we tell Lean the rules of memory, as a tiny pretend computer called `execute` (in [OwnershipLowering.lean.in](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLowering.lean.in)). It runs the steps one by one and answers `none`, meaning "broken", whenever a rule is broken:

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

The first rule: **you may only free memory that is still alive.** Freeing it twice gives `none`.

The second rule: **when the function returns, exactly one list may be left, and it must be the one being returned.** A leftover list is a leak, and it gives `none`.

### 4. State the claim and prove it

The **theorem** is the claim we want Lean to check:

```lean
theorem compiledGrow (xs : List α) (x : α) :
    execute x ⟨[(0, xs)], []⟩ emitted = some (xs ++ [x]) := by rfl
```

Read it like this: "For **any** list `xs` and **any** item `x`, running the compiler's code by the rules gives back `xs` with `x` on the end, and never breaks a rule."

The part after `by` is the **proof**. Here it is just `rfl`, which tells Lean: "run the steps yourself and check that both sides come out the same." Lean does this for a list it knows nothing about, so the answer holds for every list at once. If any step leaked, freed twice or freed too early, the result would be `none`, not the right list, and Lean would refuse.

That is the whole idea:

1. Take the code the compiler really produced.
2. Write down the rules it must follow.
3. State what it should compute.
4. Let Lean check it for every input.

### 5. Prove the proof can fail

A checker that never says no is useless. So, on every run, CI breaks the compiler's code on purpose and makes sure Lean **refuses** each broken version:

| Broken on purpose | What was changed | Rule it breaks |
| --- | --- | --- |
| Leak | the free is removed | a leftover list at the return |
| Double free | the old list is freed twice | freeing memory that is not alive |
| Early free | the old list is freed before the new one is made | reading memory that was freed |
| Wrong branch | the `if` in `growWhen` picks up the wrong list | the wrong list is returned |

For example, here is the leak version. It is the same list with `.free (some 0)` missing:

```lean
[.reserve 0, .store 0 none, .reserve 1, .store 1 none, .store 1 (some 0),
 .store 1 none, .append 0 1, .store 0 (some 1), .ret 1]
```

Two lists are now alive at the return, so Lean answers:

```text
error: Tactic `rfl` failed: The left-hand side
  execute x { heap := [(0, xs)], slots := [] } emitted
is not definitionally equal to the right-hand side
  some (xs ++ [x])
```

If the compiler ever produced code like this, the pull request would go red.

### 6. Check the checker

Lean has been carefully built and tested, but it is still software. So each finished proof is exported and checked again by **Osprey's own proof checker**, a separate checker written in Osprey. It runs three ways: with garbage collection, with reference counting, and as WebAssembly. Each must accept the proof and report `axioms: none`, which means the proof relied on nothing unproved.

## What else is proved this way

The same steps check more of the compiler's output and runtime:

- `compiledGrowWhen`: `growWhen(xs, x, enabled)` frees the old list when it grows it, keeps it when it doesn't, and is right both ways.
- `runtimeRootOrigins`: the C helpers for list append, list prepend and map set return either memory they just made or a value they were given, never memory that belongs to someone else.
- `renderedStringOrigin` and `stringResultOrigins`: building a string and 15 string operations hand back a string the caller owns.
- `compiledPairFields` and `compiledChildDrop`: a record with two fields frees each field exactly once.

Separately, [OwnershipLedger.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLedger.lean) proves the ownership rules themselves make sense. For example, `lent_pins_owner` proves that while a value is borrowed, its owner can't free it or give it away.

## Every proof runs in CI

The required `CI` check runs every one of these proofs on each pull request that changes code. The job `Tests: proof kernel and Lean speed gate` runs `make _test_proof_ci`, which runs `make _test_ownership_proof`, which runs both proof scripts:

- [ownership-lowering.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-lowering.sh) compiles the program, translates it, checks every theorem, and checks that every broken version is refused.
- [ownership-ledger.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-ledger.sh) checks the rules, and checks that weaker rules fail.

You can run the same thing locally with `make _test_ownership_proof`.

## What it does not prove yet

A proof only covers what its theorem says. These theorems cover `grow`, `growWhen`, the record code and the runtime helpers above. They do **not** yet prove the compiler frees memory correctly for **every** program. That larger proof is open work in [plan 0045](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0045-compile-time-ownership.md). Until then, every other program is checked by tests: each test program runs in ownership mode and must finish with no memory left alive. The [memory management specification](/spec/0018-memorymanagement/) lists each proof and what it assumes.

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
