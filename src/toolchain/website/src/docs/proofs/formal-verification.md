---
layout: page
title: Formal Verification
description: "Formal verification is here. A plain-language guide to what it is, why it raises software quality beyond testing, and the code in Osprey that already proves compiled code, memory handling and audio processing correct."
permalink: /docs/proofs/formal-verification/
---

{% from "native-capture.njk" import capture %}

**Formal verification means proving that software is correct, not just testing it, and you can use it today.** Proofs checked by computer already cover a C compiler ([CompCert](https://xavierleroy.org/publi/compcert-CACM.pdf), led by Xavier Leroy) and an operating system kernel ([seL4](https://doi.org/10.1145/1629575.1629596), led by Gerwin Klein). Osprey brings the same method to an everyday programming language and the code its compiler produces. Osprey is testing how far the method can go and how much it can improve ordinary software.

> **Experimental.** Today Osprey proves a few compiled functions and much of its audio code. The compiler and the C runtime as a whole are not proved yet. A proof is evidence about the code it names, not a guarantee about your program.

## What formal verification means

Every piece of code makes a promise. A sort function promises its output is in order. A bank transfer promises that money is neither created nor lost. Formal verification takes such a promise, writes it down exactly, and has a computer check an argument that the code always keeps it.

Each idea below appears on this page.

- **Specification.** The promise, written exactly. "Moving the pan dial never changes the track's loudness" is a specification.
- **Theorem.** The specification written in a language a computer can read.
- **Proof.** The step-by-step argument that the code meets the theorem, written so that a computer can check it.
- **Proof checker.** A small program that checks every step of a proof. If one step does not follow, the checker refuses the whole proof. A checker is kept small so that people can trust it, and its trusted core is called the **kernel**.
- **Lean.** A widely used proof checker and programming language, created by Leonardo de Moura ([Lean](https://lean-lang.org/)). Osprey writes its theorems and proofs in Lean. Osprey also has its own [Lean-compatible kernel](/docs/mathlib-port/), written in Osprey, which checks the same proofs a second time.
- **Assumption.** A fact the proof takes as given, such as "adding numbers in either order gives the same answer". A proof is only as sound as its assumptions, so Osprey writes every one into the theorem where you can read it.

{{ capture("/assets/images/guides/proof-workbench-commutativity.png", "The Osprey Proof Workbench showing the theorem p ∧ q → q ∧ p, its five-step proof, the hypotheses hp : p and hq : q, and the badge Checked by the Osprey kernel.", "All of these ideas on one screen: the theorem p ∧ q → q ∧ p, its five-step proof, and the badge saying Osprey's kernel checked every step.", 1356, 820) }}

## Why it raises software quality

A test runs code on inputs someone picked and checks the answers. Ten tests try ten inputs. Code can still fail on the eleventh input, on an empty list or on a value nobody thought to try. As Edsger Dijkstra wrote, "Program testing can be used to show the presence of bugs, but never to show their absence" ([EWD249](https://www.cs.utexas.edu/~EWD/ewd02xx/EWD249.PDF)).

A proof covers **every** input at once: every list length, every dial position, every sound. If a proof checks, no input breaks the promise. The idea goes back to Tony Hoare's 1969 paper [An axiomatic basis for computer programming](https://doi.org/10.1145/363235.363259).

Proofs also keep holding as the code changes. Osprey re-checks its proofs on every proposed change before the change is accepted. If a change breaks a promise, the proof fails and the change is stopped. A test would only catch that if someone had happened to test the right input.

Osprey still runs tests, because a proof covers only what its theorem says. A proof about loudness says nothing about speed or about how the screen looks.

## How Osprey proves real code

Every proof on this page follows the same five steps:

1. **Take the real code.** The proof reads the code Osprey actually ships or compiles. Nobody copies it by hand, so a proof cannot pass against an out-of-date copy.
2. **Write the rules.** These describe what the computer does, such as how memory behaves.
3. **State the promise** as a theorem.
4. **Check it** with Lean and with Osprey's own kernel.
5. **Break it on purpose.** The checks change the code in small, deliberate ways, such as deleting a line or changing one digit, and require the proof to fail every time. A proof that still passes against broken code would be proving nothing about that code.

## Not only for maths formulas

Many people believe formal verification only suits maths formulas: code that takes numbers and returns numbers, with no memory and no side effects. Osprey proves facts about the low-level instructions its compiler produces, including how they handle memory.

### The problem: giving memory back

When a program builds a list, it borrows a piece of the computer's memory to hold it. When the list is no longer needed, the program must give that memory back, which is called **freeing** it. Three mistakes are common:

- **Leak:** the memory is never freed, so the program slowly uses more and more.
- **Double free:** the same memory is freed twice. This can crash the program or let an attacker take control.
- **Use after free:** the program frees memory and then reads it anyway, which gets garbage or leaks secrets.

These mistakes cause many of the security holes found in software written in C and C++. In Osprey's ownership mode (`--memory=ownership`), the **compiler**, the program that turns Osprey code into instructions the computer runs, decides where every free goes. You never write one. That removes the human mistake, but now you have to trust the compiler. The proof is how Osprey checks the compiler's work.

### The code and what the compiler produces

Take this function from [single_owner.test.osp](https://github.com/Nimblesite/osprey/blob/main/src/osprey/tests/regressions/ownership/single_owner.test.osp):

```osprey
fn grow(xs, x) = listAppend(xs, x)
```

```osprey-ml
grow (xs, x) = listAppend (xs, x)
```

`grow` builds a new list from the old list `xs` with the item `x` on the end. The old list is then no longer needed, so it must be freed exactly once, and only after the new list exists.

The compiler turns this into **LLVM IR**, the compiler's last readable step before machine code ([LLVM's description](https://llvm.org/docs/LangRef.html)):

```llvm
%r0 = call i8* @osprey_list_append_of(i8* %$p0, i64 %$p1, i32 0)
call void @osp_ownership_free(i8* %$p0)
ret i8* %r0
```

Line by line: build the new list, free the old list, return the new list.

### The rules and the promise

A translator reads those instructions and writes them out in Lean every time the proofs run. Lean then runs them against a small model of memory called `execute` ([OwnershipLowering.lean.in](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLowering.lean.in)). The model stops with a failure if code frees memory that is not in use, or if anything other than the returned list is still in use at the end.

```lean
theorem compiledGrow (xs : List α) (x : α) :
    execute x ⟨[(0, xs)], []⟩ emitted = some (xs ++ [x]) := by rfl
```

In plain words: for **any** list `xs` and **any** item `x`, start with `xs` in memory and run the compiler's instructions (`emitted`) under the rules. The run finishes without a failure and returns `xs` with `x` on the end. `by rfl` tells Lean to work through the steps itself and compare the result. Lean does that without knowing what is in the list, so the answer holds for every list.

As the deliberate breakages, the checks delete the free, free twice and free too early. Lean must refuse every version, and it does. [Proofs and memory ownership, step by step](/docs/proofs/how-proofs-check-code/) walks through this proof in more detail. [OwnershipLedger.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLedger.lean) proves that the ownership rules themselves make sense. For example, while a value is lent out, its owner cannot free it.

Researchers have shown before that memory handling can be proved: John Reynolds with [separation logic](https://doi.org/10.1109/LICS.2002.1029817), a way of reasoning about who owns which piece of memory, and Ralf Jung and colleagues with [RustBelt](https://plv.mpi-sws.org/rustbelt/) for the Rust language. Osprey checks the instructions its own compiler actually produced.

## Audio synthesis: proved today

Audio is where Osprey's proofs have gone furthest. [Prism](/docs/audio-synthesis/), the music studio app written in Osprey, makes its sound with the [synth package](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/synth/src). A computer stores sound as a long list of numbers called **samples**, tens of thousands per second. Every effect in Prism is code that turns one list of samples into another, and the proofs show that this code does what the audio textbooks say.

### A worked example: the pan dial

A pan dial moves a sound between the left and right speakers. Prism's pan comes from [arrange_math.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/daw/src/arrange_math.ospml):

```ospml
export panLeft pan = cos (0.7853981633974483 * (pan + 1.0))
export panRight pan = sin (0.7853981633974483 * (pan + 1.0))
```

The code turns the dial position (from −1 for full left to 1 for full right) into an angle. The left speaker gets the cosine of that angle and the right speaker gets the sine. The promise is that turning the dial moves the sound without making it louder or quieter. How loud a channel sounds depends on the **square** of its level, so the two squares must always add up to one:

```text
panLeft(pan)² + panRight(pan)² = 1      at every position of the dial
```

This is true because of a fact about circles, known since Pythagoras: for any angle, cosine squared plus sine squared equals one. Here is the theorem, simplified for reading ([full version](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/ProductionSpectral.lean.in)):

```lean
theorem panEqualPower (pan : α)
    (pythagoras : ∀ θ, cos θ * cos θ + sin θ * sin θ = 1) :
    panLeft pan * panLeft pan + panRight pan * panRight pan = 1
```

Line by line: for any dial position `pan`, assuming the circle fact holds for every angle `θ`, the left power plus the right power equals one. The `panLeft` and `panRight` in this theorem are not copies. [synth_proof.rs](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/crates/osprey-syntax/examples/synth_proof.rs) reads them out of the shipped Osprey file each time the proofs run.

To break it on purpose, [production-fft.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/production-fft.sh) changes the last digit of the right channel's constant. The two speakers then use slightly different angles, the loudness drifts as the dial turns, and the proof fails, as it must.

{{ capture("/assets/images/native/prism-mix-bus.webp", "Prism Mix page: tracks routed to group buses, a kick-keyed compressor, the Space reverb and Master", "Prism's Mix page. Mixing tracks together and the pan law are both proved against the Osprey source that runs here.", 1400, 914) }}

### What else is proved

- **Mixing adds.** Sending two tracks into one mix at given levels gives exactly the level-weighted sum of the two, in either order.
- **The frequency splitter is right.** An FFT (fast Fourier transform) is the standard method for splitting a sound into its frequencies, like the bars on a graphic equaliser. It is fast because it repeatedly splits the job in half. The proofs show that Prism's FFT puts each half back together into the right frequencies.
- **Slices rejoin without a seam.** Some effects work on a long sound in short, overlapping slices. The proofs show the slices rebuild the original sound exactly when their overlap weights add up to one, and only then.
- **Distortion stays clean.** A waveshaper bends the shape of a sound wave to add warmth or grit. Done naively, it adds harsh false tones called aliasing. Prism's fix averages the bend between neighbouring samples, and the proof shows it computes that true average.

{{ capture("/assets/images/native/prism-waveshaper.webp", "Prism track patch: an exciter, strike comb and waveguide string feed Waveshaper 4, whose inspector shows curve, drive, bias, output level and antiderivative antialiasing", "Prism's waveshaper with antialiasing switched on. The averaging it uses is proved against its Osprey source.", 2304, 1504) }}

These proofs follow two standard textbooks: Julius O. Smith's [Spectral Audio Signal Processing](https://ccrma.stanford.edu/~jos/sasp/) and Miller Puckette's [Theory and Technique of Electronic Music](http://msp.ucsd.edu/techniques.htm). Osprey's own kernel checks each proof three ways: with garbage collection, with reference counting (two ways a program can manage memory) and as WebAssembly, the format browsers run. The checks make 51 deliberate edits to the audio source, and each edit must break the proof it targets.

{{ capture("/assets/images/native/prism-spectral.webp", "Prism track patch: a phase operator and additive partials feed Spectral processor 2, set to morph, then an FDN reverb", "Prism's spectral processor, built on the FFT whose recombination is proved.", 3072, 1780) }}

One limit matters. The proofs treat numbers as exact, but computers round numbers very slightly. Measuring how much that rounding can matter is separate work that is still open. [The full list of audio proofs](/spec/synthesis/) gives the textbook source for each proof, what it assumes and what it leaves out.

## Where it is going: the whole compiler and runtime

Today's translator recognises a few known instruction patterns. The [IR to Lean plan](https://github.com/Nimblesite/osprey/blob/main/docs/plans/ir-to-lean.md) replaces it with a general framework covering the compiler and the **C runtime**, which is the part of Osprey written in C that every program uses for lists, maps and text ([list_runtime.c](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/compiler/runtime/list_runtime.c), [map_runtime.c](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/compiler/runtime/map_runtime.c), [string_runtime.c](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/compiler/runtime/string_runtime.c)).

1. **Read** the compiled instructions with LLVM's own tools and [@osprey/xdsl](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/xdsl/src). That package is an Osprey version of [xDSL](https://xdsl.dev/), a widely used reader for compiler code built around LLVM's [MLIR](https://mlir.llvm.org/) toolkit.
2. **Translate** each function into Lean with [ir_lean](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/proof/ir_lean/src). The functions run against a model of LLVM and of memory ([Memory.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/LlvmSubset/Memory.lean), [Exec.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/LlvmSubset/Exec.lean)).
3. **Compare** the model with the real compiled program on thousands of random inputs. They must give the same answers, so the model is checked against what the computer really does.
4. **Prove** that any function following the ownership rules never leaks, double frees or uses freed memory ([Ledger.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/LlvmSubset/Ledger.lean)), then show that everything the compiler produces follows those rules.

The runtime goes through the same steps, because the C compiler turns those C files into the same kind of instructions. [Vellvm](https://github.com/vellvm/vellvm) ([Zhao, Zdancewic and colleagues](https://doi.org/10.1145/2103656.2103709)) showed earlier that LLVM's instructions can be given an exact meaning that proofs can use.

| Step | Status |
| --- | --- |
| Reading and translating | Works. Reads a 9,143-instruction program in about half a second and translates 12 of its 76 functions. |
| The general rule: a run only ever takes steps the ownership rules allow | Checked by Lean. |
| Three compiled functions (`growScalar`, `growManaged`, `growWhen`) | Checked by Lean in under a second. Osprey's own kernel is still too slow for them. |
| Comparing with the real program, the runtime, every function | Not built yet. |

The [IR to Lean specification](/spec/irtolean/) states each theorem and what it assumes.

## Mathematics too

The same checkers are now used at the frontier of mathematics. OpenAI published its solution to a Navier-Stokes problem, about the equations of how water and air flow, together with a Lean version of the proof. Mathlib, Lean's shared library of checked mathematics, states the Riemann hypothesis exactly, so a future proof of it could be checked by machine. Osprey's kernel already checks selected Lean results: see [the Mathlib port](/docs/mathlib-port/) and [the Navier-Stokes film](/docs/proofs/videos/navier-stokes-proof/).

{{ capture("/assets/images/guides/mathlib-check.png", "Recorded Mathlib tutorial: inspect a real check and its result; the Mathlib port guide distinguishes imported certificates from independent operation.", "Recorded Mathlib tutorial: Osprey's kernel checks a real Mathlib result and shows the outcome.", 1920, 1080) }}

## The code doing it

| What | Where |
| --- | --- |
| Osprey's own proof kernel | [infer.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/proof/kernel/src/infer.ospml), [checked.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/proof/kernel/src/checked.ospml) |
| Ownership rules | [OwnershipLedger.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLedger.lean), [ownership-ledger.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-ledger.sh) |
| Proofs about compiled code | [OwnershipLowering.lean.in](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/OwnershipLowering.lean.in), [ownership-lowering.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ownership-lowering.sh) |
| IR to Lean | [@osprey/xdsl](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/xdsl/src), [ir_lean](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/proof/ir_lean/src), [LlvmSubset](https://github.com/Nimblesite/osprey/tree/main/src/toolchain/benchmarks/proof/tools/LlvmSubset), [ir-lean.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/ir-lean.sh) |
| Audio proofs | [synth_proof.rs](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/crates/osprey-syntax/examples/synth_proof.rs), [Synthesis.lean](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/Synthesis.lean), [ProductionSpectral.lean.in](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/ProductionSpectral.lean.in), [ProductionWaveshaper.lean.in](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/benchmarks/proof/tools/ProductionWaveshaper.lean.in), [production-fft.sh](https://github.com/Nimblesite/osprey/blob/main/scripts/proof/production-fft.sh) |

To run the proofs yourself, use `make _test_ownership_proof` and `make _test_synthesis_proofs`.

## Read more

- [Theorem Proving in Lean 4](https://lean-lang.org/theorem_proving_in_lean4/) and [Functional Programming in Lean](https://lean-lang.org/functional_programming_in_lean/), the free official Lean books.
- [Software Foundations](https://softwarefoundations.cis.upenn.edu/), by Benjamin Pierce and colleagues, a free course on proving programs correct.
- [The Lean 4 Theorem Prover and Programming Language](https://doi.org/10.1007/978-3-030-79876-5_37), by Leonardo de Moura and Sebastian Ullrich.
- [Formal verification of a realistic compiler](https://xavierleroy.org/publi/compcert-CACM.pdf), by Xavier Leroy, the story of CompCert.
- [seL4: formal verification of an OS kernel](https://doi.org/10.1145/1629575.1629596), by Gerwin Klein and colleagues.
