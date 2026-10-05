# How to use this book

You can begin with a text file and one change to a quoted string. This book builds from that first result, explaining each new idea through a program you can run.

Osprey helps you build programs from values and functions, keep ordinary failure visible, and choose how operations such as logging or storage are implemented. That last ability comes from **algebraic effects**. Chapter 9 introduces them from the beginning; no previous knowledge of functional programming is required.

This working manuscript has completed Chapters 1–3 and 8–10. The remaining chapters are labelled outlines. Each finished chapter supplies self-contained examples, so you can explore the effects sequence while the intervening lessons are being written.

![The book moves from one running file through honest data and outside-world interaction to a program the reader can ship and reshape.](assets/diagrams/00-reading-journey.png)

*Figure 0.1 — The four parts grow one Flight Log rather than restarting with disconnected examples.*

## Two ways to begin

The fastest path uses the [Osprey Playground](https://www.ospreylang.dev/playground/). It runs in a browser and requires no local toolchain. Use it when you want the first result now.

The local path uses the `osprey` compiler and LLVM's `clang`. Use it when you want to keep files on your computer and build native programs. The maintained [installation guide](https://www.ospreylang.dev/docs/installation/) has the current steps for macOS, Linux, and Windows.

Chapter 1 works on either path. Command blocks show the local form; Playground readers can paste the same Osprey source into the editor and use its run control.

## One teaching surface first

Osprey can currently be written in Default flavor and ML flavor. More source flavors may arrive in the future.

You do not need to choose among them now.

This book leads with Default flavor. A Default file ends in `.osp` and uses familiar pieces such as `fn`, `let`, braces, and parenthesised calls. That gives readers from mainstream languages less surface syntax to learn at once.

ML flavor is an optional alternative. It uses indentation, whitespace application, and currying by default. The book introduces it after the shared language ideas are comfortable. Skipping every ML aside will still give you a complete journey through the book.

A coding agent can translate a file between source flavors quickly. Treat that translation like any other code change: check it, run the tests, and compare the behavior. Convenience is not evidence; the compiler and tests provide the evidence.

## The Flight Log

Most chapters add one capability to a small project called Flight Log. It records what you want to learn and how far you have travelled.

The project begins as a single printed line. Later it gains:

- named values and small functions;
- explicit states such as planned, learning, and complete;
- lists and transformation pipelines;
- visible success and failure;
- tests;
- effects for outside work;
- files or web calls;
- concurrent tasks; and
- a real build target.

The first version is deliberately tiny. Good programs do not earn their value from file count.

## The page signals

Each chapter uses a few repeated signals.

**Try it** asks you to make one small edit, predict the result, and run it.

**Compiler says** creates a useful error on purpose. Read the reported file, name, or type disagreement before changing anything. Some diagnostics also give a line and column; others do not.

**Under the wing** gives the precise functional-programming term for something you have already used. These notes are optional; they are also a quiet promise to experienced FP readers that the book is teaching the real language ideas.

**Same flight, different feathers** shows an optional source flavor comparison. Default always comes first and receives the full explanation.

**Agent handoff** is a paste-ready task for a coding agent. It includes what must remain unchanged and how the agent should verify the result.

## How to use an agent without losing the lesson

An agent is excellent at typing a mechanical change, explaining a compiler message in different words, and producing a second example. It can also give you a confident answer that has not been checked.

Keep three jobs for yourself:

1. Say what outcome you want.
2. Predict one important part of the behavior.
3. Read the command or test result that proves what happened.

Ask the agent to show a small diff and run a specific check. If it changes the design while translating syntax, ask it to revert to the smallest behavior-preserving change.

## Alpha means honest edges

Osprey is alpha software. Syntax, tooling, and implementation details can change. This working edition records its development compiler's identity and checks in `evidence.json`; a release version still needs to be pinned before publication.

Project modules and imports between Default and ML files already work. The package manager remains planned. The later chapters distinguish using code in your own project from downloading and distributing packages.

Examples identify their target when it matters. Native executables support dynamic control handlers with single-shot resumption; WebAssembly and mobile libraries currently support value handlers and static discharge, but reject dynamic control operations. Chapter 9 explains those terms beside runnable examples.

Memory choices also have limits: the default allocator retains general allocations, while native GC and ARC are separate options. Chapter 12 explains the choices and deployment boundaries. Calling a C library crosses Osprey's memory-safety boundary.

Read each example's requirements and test the program on the target you plan to use.

## A good pace

Type the Chapter 1 program yourself. After that, copying a longer example is fine if you still make the requested change and predict the result.

Stop at each Flight Log checkpoint. Commit it to memory, a notebook, or version control if that helps you see progress. When a chapter introduces a specialist term, connect it to the code you already ran.

You are ready when you can create a text file and change a quoted string. The next chapter turns that small ability into a running program.
