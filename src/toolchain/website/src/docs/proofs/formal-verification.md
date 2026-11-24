---
layout: page
title: Formal Verification
description: "Experimental: how Osprey uses Lean to prove its compiled code, its runtime and its audio code correct for every input, and why that goes further than testing."
permalink: /docs/proofs/formal-verification/
---

> **Experimental.** Formal verification in Osprey is research in progress. A few compiled functions and much of the audio code are proved today. The compiler and the C runtime as a whole are not. Treat these proofs as evidence about the code they name, not as a guarantee about your program.

## What it is

Formal verification proves that code is correct instead of checking samples. You state exactly what the code must do. Then a proof checker checks an argument, step by step, that the code does it. Osprey uses [Lean](https://lean-lang.org/) and its own [Lean-compatible proof kernel](/docs/mathlib-port/) as checkers. If one step is wrong, the checker refuses the whole proof.

## Why it goes further than testing

A test runs the code on the inputs someone picked. A proof covers **every** input: every list length, every pan position, every sample value, including the cases nobody thought to try. A passing test means the tried cases work. A checked proof means no case fails. Osprey still tests too, because a proof covers only what its statement says.

## Example: a pan control that keeps loudness steady

The Prism DAW pans a track between the left and right speakers with this code from [arrange_math.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/daw/src/arrange_math.ospml):

```ospml
export panLeft pan = cos (0.7853981633974483 * (pan + 1.0))
export panRight pan = sin (0.7853981633974483 * (pan + 1.0))
```

The promise is that moving the pan never makes the track louder or quieter. The power of the two channels must always add up to one:

```text
panLeft(pan)² + panRight(pan)² = 1      for every pan from −1 to 1
```

CI reads those two lines from the shipped source and turns them into Lean. This is the theorem, simplified for reading ([full version](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/ProductionSpectral.lean.in)):

```lean
theorem panEqualPower (pan : α)
    (pythagoras : ∀ θ, cos θ * cos θ + sin θ * sin θ = 1) :
    panLeft pan * panLeft pan + panRight pan * panRight pan = 1
```

The proof notices that both channels use the same angle and then applies `cos² + sin² = 1`. To show the proof really depends on the code, CI changes the last digit of the right channel's constant and requires the proof to fail. It does fail, because the two channels no longer share an angle.

## Not just maths formulas

People often think proofs only suit maths formulas and pure functions, with no memory, no pointers and no side effects. Osprey proves facts about the **LLVM IR** the compiler emits, which is the low-level code just before machine code, including allocating and freeing memory.

With `--memory=ownership`, the compiler decides where every free goes. For this function:

```osprey
fn grow(xs, x) = listAppend(xs, x)
```

it emits:

```llvm
%r0 = call i8* @osprey_list_append_of(i8* %$p0, i64 %$p1, i32 0)
call void @osp_ownership_free(i8* %$p0)
ret i8* %r0
```

A translator turns those instructions into Lean on every run, so nobody retypes them. Lean then runs them against a model of memory that fails on a leak, a double free or a use after free:

```lean
theorem compiledGrow (xs : List α) (x : α) :
    execute x ⟨[(0, xs)], []⟩ emitted = some (xs ++ [x]) := by rfl
```

For any list and any item, the compiled code returns the longer list and frees the old one exactly once. CI also deletes the free, frees twice and frees too early, and Lean must refuse every one of those versions. [Proofs and memory ownership, step by step](/docs/proofs/how-proofs-check-code/) walks through this proof.

## Where it is going: IR to Lean

The translator today recognises a few known instruction patterns. The [IR to Lean plan](https://github.com/Nimblesite/osprey/blob/main/docs/plans/ir-to-lean.md) replaces it with a framework that covers the compiler and the C runtime:

1. **Read** the real IR with LLVM's own tools and `@osprey/xdsl`, an Osprey port of the xDSL parser.
2. **Translate** each function into a small model of LLVM in Lean, with a real memory model.
3. **Compare** the model with the real compiled binary on thousands of random inputs. They must agree, so the model is checked against reality.
4. **Prove** that any function following the ownership rules is memory-safe, then show that everything the compiler emits follows those rules.

The C runtime goes through the same steps: clang compiles `list_runtime.c`, `map_runtime.c` and the other runtime files to the same IR.

| Step | Status |
| --- | --- |
| Reader and translator | Working. Reads a 9,143-operation module in about half a second and translates 12 of its 76 functions. |
| General rule: a run only ever makes moves the ownership rules allow | Checked by Lean. |
| Three compiled functions (`growScalar`, `growManaged`, `growWhen`) | Checked by Lean in under a second. Osprey's own kernel is still too slow. |
| Comparing the model with the binary, CI, the C runtime, every function | Not built yet. |

The [IR to Lean specification](/spec/irtolean/) states each theorem and its assumptions.

## Audio synthesis: proved today

Audio is where this has gone furthest. CI reads the shipped Osprey source of the synth package and the Prism DAW and proves that each piece does what the audio textbooks say:

- the FFT combines its halves into the right frequency bins;
- overlapping windows rebuild a sound exactly when their weights add to one;
- the waveshaper's anti-aliasing returns the true average of its curve;
- mixing two tracks adds them, and the pan keeps loudness steady.

Osprey's own kernel checks each proof under garbage collection, reference counting and WebAssembly. CI makes 51 edits to the source, and each edit must break the proof it targets. The proofs assume exact arithmetic, so bounds on rounding error are separate, open work.

[The full list of audio proofs](/spec/synthesis/) cites the textbook source for each one and states what it assumes and what it leaves out. The [audio synthesis guide](/docs/audio-synthesis/) shows the code those proofs cover.
