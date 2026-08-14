# Glossary

This glossary is the vocabulary authority for *The Osprey Book*. Definitions favour the meaning a learner needs in the chapter where a term first appears.

## Argument

A value supplied when calling a function. In `greet("Mika")`, the string `"Mika"` is an argument.

## Binding

A name connected to a value. Default flavor writes an immutable binding as `let name = "Mika"`. The name helps later expressions refer to that value; it does not imply a box that must change.

## Compiler

The program that reads Osprey source, checks it, and produces a native program or WebAssembly module. Running with `--check` stops after checking.

## Default flavor

The book's teaching surface and Osprey's default source syntax. It uses `.osp` files, braces, `fn`, `let`, and parenthesised calls.

## Effect

A typed request for work outside an ordinary calculation, such as logging or storage. The code performing an effect asks for an operation; a handler decides how to answer it.

## Expression

Code that produces a value. A string, a function call, a `match`, and many blocks are expressions in Osprey.

## Fiber

A lightweight unit of concurrent work. Osprey fibers communicate by sending values rather than sharing mutable state.

## Flavor

A source-level way to write Osprey. Default and ML are the currently available flavors. A flavor changes how code is written and read; shared checking and code generation operate after that source has been translated into the language's common program form. More flavors may be added in the future.

## Function

A named or anonymous transformation from input values to an output value. A function can be called more than once with different arguments.

## Handler

Code that gives meaning to one or more effect operations for a particular region of a program.

## Immutable

Unable to be reassigned after creation. Most Osprey bindings are immutable, so a name continues to mean the value it was given.

## Inference

The compiler's ability to work out types from how values are created and used. Inference keeps strong checking while removing obvious annotations.

## ML flavor

An optional Osprey source flavor using indentation-based layout, whitespace application, and currying by default. This book teaches it as an alternative after the shared language ideas are comfortable.

## Native program

A program compiled for a particular operating system and processor, without a virtual machine or JIT warm-up. Osprey produces native code through LLVM and clang.

## Parameter

A name in a function declaration that receives an argument. In `fn greet(name) = ...`, `name` is a parameter.

## Pattern

A shape used by `match` to recognise and, when needed, unpack a value.

## Pattern matching

A decision that compares a value with explicit patterns. For a known union or `Result`, the compiler requires every possible case to be covered.

## Persistent collection

An immutable list or map whose updates return a new collection while safely reusing unchanged internal structure.

## Pipeline

A left-to-right chain made with `|>`. The value on the left becomes the first argument of the function on the right.

## Record

A type or value with named fields that belong together, such as a project with a `name` and `status`.

## Result

A value that is either `Success` with a useful value or `Error` with failure information. `Result` keeps expected failure visible in the type.

## Type

A description of which values an expression may produce and which operations make sense for them.

## Union

A type that lists a closed set of possible cases. Functional programmers may know this as a sum type or algebraic data type.

## Value

A piece of data a program can use, such as a string, number, boolean, list, record, union case, or function.

## WebAssembly

A portable compilation target that can run in supported browser and server environments. Osprey's WebAssembly target supports a smaller runtime surface than native programs.
