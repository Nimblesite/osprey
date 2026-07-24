---
layout: page.njk
title: "Osprey Flavors: One Core, Two Flavors, Zero Compromise"
excerpt: "Braces or layout — pick your tribe and go all in. The ML flavor isn't braces-optional and the Default flavor isn't deprecated. It's the same language underneath."
description: "Compare Osprey’s brace-style and ML-style flavors, how both lower to one canonical AST, and which shared compiler features and limitations apply today."
modified: 2026-07-23
tags: ["blog", "language-design", "flavors", "functional-programming", "ml-syntax"]
author: "Christian Findlay"
readingTime: 7
image: /assets/images/blog/osprey-flavors.png
---

When I started working on Osprey, the dream was *zero compromise*. I think the biggest shame about the most popular languages is that they compromise. The language designers make decisions for adoption but end up watering down the original spirit of the language because of this. I didn't want this for Osprey. I wanted something else. I wanted the language to be exactly what I wanted it to be and to have all the performance, safety and elegance of the other great, modern languages.

I found out immediately that there was a big catch to this. You cannot please everyone. It's not just about aesthetics. You have to make tradeoffs when you design syntax. Certain decisions push you in directions that have rippling effects. The most obvious decision is the decision on whether to use indentation or braces to specify blocks.

Many programmers that are accustomed to C style languages like C#, Java, Dart and so on find indentation based syntax to be too uncomfortable to use. But, indentation based languages mean you can remove a lot of symbols and reduce visual noise. At the end of the day, it's cleaner but it also alienates a whole group of people. Many people wouldn't touch Osprey because it looks like code from another tribe.

Every language picks a side, and every side loses someone. Curly braces or significant whitespace. `fn add(x, y)` or `add x y`. The systems programmer who wants explicit blocks and named arguments, versus the FP devotee who wants layout and curry-by-default. Pick braces and the Haskell crowd wrinkles their nose; pick layout and the C crowd walks away. The syntax wars are real, and they force you into a tribe before you've written a line.

I had encountered dilemmas several times while build Osprey and the answer was always "put an abstraction here". My thinking was "Can we defer this decision? Can we make this aspect of the language pluggable?". We shouldn't bake decisions into the language early. We should allow people building with Osprey to make their own decisions. And, in many cases, I found that leaving an abstraction where the develop could make their own call was the exact right decision.

Then, I asked the question "Can we make the syntax pluggable?". Well, it turns out that we absolutely can and it's barely even a for the compiler. When we parse code, we convert the code into a Concrete Syntax Tree (CST). This is the first pass that just converts the code unprocessed into in-memory data. Then, that data is converted to an Abstract Syntax Tree (AST). This is the processed syntax that can be readily converted to code.

It turns out that we can basically swap any CST in front of the AST. That's how we get flavors. It's a powerful concept. There is nothing about any language that weds it to the syntax. The syntax is basically just a taste aspect of the language. So, I made the obvious choice to allow multiple flavor syntaxes instead of tying Osprey to one view of the world.

Osprey's answer is to stop pretending there's one right answer. **One core. Two flavors. Zero compromise.**

## The problem: syntax forces a tribe

The FP-snob-versus-systems-programmer divide is mostly about spelling. The ideas — algebraic data types, exhaustive matching, immutability, effects — are not in dispute. What's in dispute is whether `do`-blocks should have braces, whether application should need parentheses, whether a function with two arguments is one value or two. These are aesthetic and ergonomic preferences, and they are *strong* preferences. Telling someone their preferred surface is wrong is how you lose them.

Most languages resolve this by declaring a winner and grudgingly bolting on the loser as an afterthought — a "lite" mode, an optional layout extension, a deprecated legacy syntax kept alive for migration. The afterthought is always second-class, and everybody can tell.

## Osprey's answer: flavors

Osprey ships **two first-class, permanent syntaxes** called flavors. Neither is the watered-down one.

- **Default flavor (`.osp`)** — C-style braces, `fn`, `f(x: a, y: b)` calls with named arguments. Explicit, familiar, block-structured. This is the surface a systems programmer reaches for. Fully implemented today.
- **ML flavor (`.ospml`)** — offside-rule layout (indentation, no braces), curry-by-default, whitespace application `f a b`, `\x => e` lambdas, `:=` mutation, `->` for types and `=>` for clauses. Terse, expression-first, ML/Haskell-shaped. This is the surface an FP devotee reaches for. Fully implemented today.

The point is **no compromise**. The ML flavor is not "braces optional." The Default flavor is not deprecated or transitional. Each surface goes all the way in its own direction. Systems programmers get real braces and real named arguments; FP folks get real layout and real currying. Nobody is asked to swallow the other camp's spelling. The language belongs to *your* tribe — pick your flavor and go all in.

Here's the ML flavor saying hello — this runs today:

```osprey-ml
greeting = "Hello from the ML flavor"
print greeting
print "2 + 3 = ${2 + 3}"
```

No `fn`, no braces, no parentheses around the print argument. Layout and whitespace application all the way down.

## How it actually works

A flavor is not a preprocessor or a transpiler bolted onto a host language. Each flavor is a **parser plus a lowerer** that converge on **one canonical AST** — `osprey_ast::Program`. After lowering, there is exactly one type checker, one effect system, one optimiser, one LLVM/wasm backend. None of them know which flavor you wrote. The flavor is gone by the time any analysis runs.

That's what makes the "no compromise" claim more than a slogan: both surfaces meet at the same tree, so both get the same Hindley-Milner inference, typed effect operations, and the same performance. There is no second-class path.

The one honest difference between the surfaces is currying, and it's machine-checked. In ML, every function is curried by default:

```osprey-ml
inc : int -> int
inc x = x + 1

add : int -> int -> int
add x y = x + y

// partial application falls straight out of currying:
addTen = add 10
answer = addTen 32        // 42
```

That ML `add x y` lowers to *exactly* the same canonical AST as this Default-flavor explicit-curry definition:

```osprey
// Default flavor (.osp):
fn add(x) = fn(y) => x + y
// ML flavor (.ospml) — identical canonical AST:
add x y = x + y
```

We have a test that asserts the two produce the same tree. Note the precision here: ML `add x y` equals the *explicit-curry* Default form, not the multi-parameter `fn add(x, y)`. The latter is deliberately a different value — a single two-argument function, not a chain. The flavors converge where they should and stay distinct where the semantics genuinely differ.

The ML surface carries its FP shape all the way through. Layout-driven `match`:

```osprey-ml
classify n =
    match n
        0 => "zero"
        1 => "one"
        _ => "many"
```

Higher-order functions and `Result` payload matching (integer division and mod return `Result<int, MathError>`, so you match the payload):

```osprey-ml
twice : (int -> int) -> int -> int
twice f x = f (f x)

bump x = x + 10

safeMod a b =
    match a % b
        Success value => value
        Error e => -1
```

Bindings and mutation, with `=` to bind and `:=` to mutate:

```osprey-ml
mut counter = 0
counter := counter + 1      // := mutates; = binds
print "counter = ${counter}"
```

## Same folder, compiled together

Because every flavor lowers to the same canonical AST *before* any type checking, the flavor is a **per-file** choice — not a per-project one. A `.osp` file and a `.ospml` file can sit in the same folder and compile into one program:

```osprey
// One project folder, two flavors, one compiled program:
//   project/
//     math.ospml     # ML flavor — curry-by-default module
//     app.osp        # Default flavor — braces; imports math
// Each file is wholly one flavor (chosen by extension/marker/--flavor). Both lower to
// the SAME canonical AST, so they share one type checker and one binary. Exports are
// canonical signatures, so a Default module and an ML module import each other normally.
```

Exports are canonical signatures with stable names and ordering, so a Default module and an ML module reference each other with no glue layer. The team is never forced to pick one tribe; each developer picks the flavor for the file they're writing.

To be precise about what ships today: **per-file flavor selection is implemented and green.** You select the ML surface with the `.ospml` extension, the `--flavor ml` CLI flag, or a leading `// osprey: flavor=ml` marker (precedence: flag > marker > extension > Default). That mechanism is exercised by tested examples right now. The multi-file *cross-flavor import* — a Default module pulling in an ML module in the same build — is the design direction the canonical-AST architecture is built for, but it is not yet covered by a tested example, so we're showing you the folder model and the per-file selection that is green, not a runnable cross-flavor import program.

## Effects: in both flavors

Osprey's headline feature is typed algebraic effects — and the lexical effect syntax works in **both** flavors. Here's the same `Logger` demo, first in the Default flavor:

```osprey
effect Logger {
    log: fn(string) -> Unit
}

fn greet(name: string) -> Unit !Logger =
  perform Logger.log("Hello, ${name}!")

// Production: write to stdout
handle Logger
  log msg => print(msg)
in greet("Alice")

// Test: stay silent — same code, new handler
handle Logger
  log msg => 0
in greet("Bob")
```

…and the identical program in the ML flavor — layout, whitespace application, `handle … in`, the lot:

```osprey-ml
effect Logger
    log : string => Unit

greet name =
    perform Logger.log "Hello, ${name}!"

handle Logger
    log msg => print msg
in greet "Alice"

handle Logger
    log msg => 0
in greet "Bob"
```

The `!Logger` row documents and helps instantiate the `Logger` operations used by `greet`. Swap the handler and the same code logs to stdout or stays silent — no global mutable wiring, just a different `handle` block. Both flavors lower to the same `Handler` node, so the effect checker and runtime never learn which one you wrote. Complete static effect-row propagation and missing-handler rejection are still in progress; today a missing runtime handler aborts with an explicit diagnostic.

## Status, honestly

The **Default flavor is the most mature surface**. It includes effects, persistent collections, native continuations, and the wider runtime, with the limitations recorded on the [feature status page](/status/).

The **ML flavor is fully implemented too**, with runnable proof you can read and run: the [tested ML examples](https://github.com/Nimblesite/osprey/tree/main/examples/tested) cover hello-world, curry-by-default with partial application, higher-order functions, `Result` matching, layout `match`, mutation, fibers, and algebraic effects with `handle … in` — each one runs through the compiler and its `stdout` is byte-compared against a checked-in `.expectedoutput`.

ML effects run today: `effect`, `perform`, `handle … in`, and native `resume` examples are byte-checked in the [tested examples](https://github.com/Nimblesite/osprey/tree/main/examples/tested/effects). Important shared-core limits remain: compile-time effect coverage is incomplete, and **first-class handler values** — a `Handler E` type you can pass around and install dynamically — are deferred. Lexically scoped `handle … in` regions work in both flavors.

## Pick your flavor

If you live in braces and named arguments, write `.osp` and never think about layout again. If you live in layout and currying, write `.ospml` and never type a brace. Either way you get the same Hindley-Milner type checker, typed effect operations, backend, and standard library — because after lowering, nothing downstream can even tell which flavor you wrote.

Pick your flavor. Go all in. It's the same Osprey.
