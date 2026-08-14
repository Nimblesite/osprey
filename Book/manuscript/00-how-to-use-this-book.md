# How to use this book

You do not need to know what a monad is. You do not need a favourite operating system, a perfectly configured editor, or a hot take about programming languages. You need enough curiosity to change a line of code and ask what happened.

This book begins there.

Osprey is a practical functional programming language. That description will mean more after you have used it. For now, it means the language helps you build programs from values and functions, keeps ordinary failure visible, and checks a surprising amount before the program runs.

The goal is not to memorise syntax. The goal is to understand why your program behaves the way it does and to make the next change with confidence.

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

**Compiler says** creates a useful error on purpose. Read the source location and the expected shape before changing anything.

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

Osprey is alpha software. Syntax, tooling, and implementation details can change. The book is tied to an edition and a compiler version so its examples can be checked rather than merely remembered.

Some language areas are deliberately qualified. Native programs and WebAssembly do not have identical runtime capabilities. Effect resumption is currently native-only. Complete package and module workflows remain in development. C libraries are useful, but C code sits outside Osprey's memory-safety guarantee.

These are not footnotes designed to spoil the fun. They are part of learning to trust technical claims: say what works, say where it works, and test the program you plan to ship.

## A good pace

Type the Chapter 1 program yourself. After that, copying a longer example is fine if you still make the requested change and predict the result.

Stop at each Flight Log checkpoint. Commit it to memory, a notebook, or version control if that helps you see progress. When a chapter introduces a specialist term, connect it to the code you already ran.

You are ready when you can create a text file and change a quoted string. The next chapter turns that small ability into a running program.

