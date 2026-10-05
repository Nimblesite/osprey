# Glossary

This glossary is the vocabulary authority for *The Osprey Book*. Definitions favour the meaning a learner needs in the chapter where a term first appears.

## Argument

A value supplied when calling a function. In `greet("Mika")`, the string `"Mika"` is an argument.

## Binding

A name connected to a value. Default flavor writes an immutable binding as `let name = "Mika"`. The name helps later expressions refer to that value; it does not imply a box that must change.

## Callback

A function passed as a value so another piece of code can call it. In `mika(greeting)`, the handler receives `greeting` and calls it after installing its operations. A callback does not by itself imply asynchronous execution.

## Compiler

The program that reads Osprey source, checks it, and produces a native program, WebAssembly module, or mobile library. Running with `--check` stops after checking.

## Continuation

The work remaining after a suspended operation. A handler for a declared `control` operation can use `resume(value)` to continue that work or answer the surrounding handler call without continuing. Current dynamic resumptions in native executables are single-shot: at most one resumption of each captured remainder. WebAssembly and mobile library targets currently reject dynamic `control` operations.

## Default flavor

The book's teaching surface and Osprey's default source syntax. It uses `.osp` files, braces, `fn`, `let`, and parenthesised calls.

## Effect

A typed interface through which code requests an operation, such as reading configuration, reporting progress, or accessing storage. The code uses `perform` to ask; a handler decides how to answer.

## Expression

Code that produces a value. A string, a function call, a `match`, and many blocks are expressions in Osprey.

## Fiber

A concurrent computation created with `spawn`; `await` receives its result. In the normal native runtime, each fiber uses one operating-system thread. Fibers can exchange values through channels and share ownership of immutable allocations; they do not have separate heaps.

## Flavor

A source-level way to write Osprey. Default and ML are the currently available flavors. A flavor changes how code is written and read; shared checking and code generation operate after that source has been translated into the language's common program form. More flavors may be added in the future.

## Function

A named or anonymous transformation from input values to an output value. A function can be called more than once with different arguments.

## Handler

An implementation of effect operations for a region of a program. A callable `handler` value is applied to the work, as `h(work)`; a block-scoped `handle` governs the remaining statements in its block. Value operations supply a result to their caller. Operations declared `control` may resume or abandon the remaining computation.

## Immutable

Unable to be reassigned after creation. Most Osprey bindings are immutable, so a name continues to mean the value it was given.

## Inference

The compiler's ability to work out types from how values are created and used. Inference keeps strong checking while removing obvious annotations.

## Memory backend

The runtime's method for managing allocated values. The default backend retains general allocations. Tracing garbage collection (`gc`) and automatic reference counting (`arc`) are separate native options with platform limits; WebAssembly and mobile libraries currently support only the default backend.

## ML flavor

An optional Osprey source flavor using `.ospml` files, indentation-based layout, whitespace application, and currying by default. This book teaches it as an alternative after the shared language ideas are comfortable.

## Module

A named group of declarations with an explicit public boundary. A project can contain several source files and import exported declarations across Default and ML flavors. Imports refer to logical names rather than file paths.

## Native program

A program compiled for a particular operating system and processor, without a virtual machine or JIT warm-up. Osprey produces native code through LLVM and clang.

## Operation

A named request declared by an effect, with specified argument and result types. `perform Learner.name()` requests the `name` operation. A value operation returns its handler arm's answer to the requesting code; a declared control operation gives its handler a suspended remainder to resume or abandon.

## Package

A distributable unit of reusable project code. Osprey's package registry and manager are planned; existing project modules and imports do not require them.

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
