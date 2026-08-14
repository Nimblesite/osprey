# Chapter 1 — One file, one result

![A broad-winged wireframe osprey rises from one luminous source thread over a geometric launch plane.](assets/illustrations/first-flight.png)

*Figure 1.1 — A first program does not need to be large. One source path, checked and run, is enough to take flight.*

A program is a set of instructions and values that a computer can work with. That definition is accurate, but it does not feel real until the computer responds to something you wrote.

So the first goal is deliberately small: make Osprey print one line.

```osprey
fn main() = print("Hello from Osprey")
```

This is a complete program. It has no project generator, configuration file, class, import list, or type annotation. There will be time for larger programs later. Right now, you need one file and one visible result.

## Reader outcome

By the end of this chapter, you should be able to:

- run a Default-flavor Osprey file;
- read a one-line function from the outside in;
- create a small function with parameters;
- bind immutable values with `let`;
- explain what the compiler inferred;
- pass a value through a pipeline with `|>`; and
- use a coding agent to translate an optional syntax surface without changing behavior.

## Choose your runway

Use the browser path if you want zero setup. Open the [Osprey Playground](https://www.ospreylang.dev/playground/), replace its source with the one-line program, and run it.

Use the local path if the compiler is already installed. Create a file called `hello.osp`:

```osprey
fn main() = print("Hello from Osprey")
```

Then run:

```sh
osprey hello.osp --run
```

The program prints:

```text
Hello from Osprey
```

If `osprey` or `clang` cannot be found, use the Playground now and return to the maintained [installation guide](https://www.ospreylang.dev/docs/installation/) when you want local builds. Toolchain setup is useful, but it is not the programming lesson.

With no explicit override or source marker, the `.osp` ending selects Default flavor. You can also think of it as a small label that helps the editor and compiler know how the file is written.

## Read the line from the outside in

The first program has five pieces.

![The one-line program is annotated with the job performed by fn, main, the body marker, print, and the string value.](assets/diagrams/01-program-anatomy.png)

*Figure 1.2 — Read the structure before worrying about every punctuation mark.*

Start with the shape:

```osprey
fn main() = ...
```

`fn` says you are declaring a **function**. A function is a reusable transformation or action. `main` is the name the runnable program begins with. The empty parentheses mean this function takes no input values. The `=` introduces the function's body: the expression that runs when the function is called.

Now read the body:

```osprey
print("Hello from Osprey")
```

This is a **function call**. `print` is the function being called. The string inside the parentheses is the **argument** supplied to it. A string is text in double quotes.

You can read the whole line aloud:

> Define a function named main. It takes no arguments. Its body prints the string “Hello from Osprey.”

That sentence matters more than memorising which symbol came first. Syntax becomes easier when each piece has a job.

### Try it: change the value

Replace the string with a message of your own:

```osprey
fn main() = print("Mika was here")
```

Before running it, predict the exact output. Then run the program.

You changed a **value**, not the structure of the program. `main` still calls `print`; it simply supplies a different string.

## Source becomes a running program

On a local machine, `--run` performs several jobs for you. Osprey reads the source, checks that the pieces fit, produces LLVM code, asks `clang` to build a native executable, and starts it.

![Osprey source passes through parsing and checks into an LLVM-backed native build, then produces visible output.](assets/diagrams/01-source-to-output.png)

*Figure 1.3 — `--run` is convenient, but the checking stage remains a real gate rather than a guess.*

You can stop after checking:

```sh
osprey hello.osp --check
```

Or compile an executable without starting it:

```sh
osprey hello.osp --compile -o hello
```

Chapter 12 returns to build targets and deployment. For now, remember the useful split:

- `--check` asks whether the source is a valid, well-typed program.
- `--run` checks, builds, and starts it.
- `--compile` checks and builds an artifact you can start later.

## Give one idea a name

The first line does two jobs at once: it decides the message and prints it. Separate those jobs by adding a function:

```osprey
fn launchLine(name, project) = "${name} launched ${project}."

fn main() = launchLine("Mika", "a first Osprey program") |> print
```

`launchLine` has two **parameters**, `name` and `project`. A parameter is a local name that receives an argument when the function is called.

The call supplies two strings:

```osprey
launchLine("Mika", "a first Osprey program")
```

Inside the function, `${name}` and `${project}` place those values into a larger string. This is **string interpolation**: building text while keeping the inserted expressions visible.

The function returns the string it creates. There is no `return` keyword because the body is already an expression. The value of that expression is the function's result.

Then the pipe operator sends that result into `print`:

```osprey
launchLine("Mika", "a first Osprey program") |> print
```

`|>` passes the value on its left as the first argument to the function on its right. You could write the same work as:

```osprey
print(launchLine("Mika", "a first Osprey program"))
```

Both forms are valid. The pipeline reads in the order the data moves: make the launch line, then print it.

### Under the wing: functions are values doing honest work

If you already know functional programming, this is not decorative “functional style.” Osprey functions produce values, expressions form bodies, immutable data is the default, and pipelines compose transformations. Later chapters add algebraic data types, exhaustive matching, persistent collections, higher-order functions, and first-class effects.

If those terms are new, ignore them for now. You have already used the core move: one function produced a value and another consumed it.

## Bind values before using them

The final Chapter 1 program gives the two arguments useful local names:

```osprey
fn launchLine(name, project) = "${name} launched ${project}."

fn main() = {
    let name = "Mika"
    let project = "a first Osprey program"
    launchLine(name, project) |> print
}
```

The braces give `main` a block with more than one step. The two `let` lines create **bindings**:

```osprey
let name = "Mika"
let project = "a first Osprey program"
```

A binding connects a name to a value. These bindings are immutable: `name` continues to mean `"Mika"` throughout this block. That removes a common question from your head. You do not need to wonder whether some earlier line changed what `name` means.

The block's last expression is the value of the block:

```osprey
launchLine(name, project) |> print
```

This calls the function with the named values and sends its result to `print`.

The complete source lives in `examples/chapter-01/first-flight.osp` in the book project. It prints:

```text
Mika launched a first Osprey program.
```

### The missing annotations are intentional

You did not write `name: string` on the bindings or parameters. Osprey inferred the types from the values and operations:

- `"Mika"` is a string, so the `name` binding is a string.
- `name` is interpolated into a string, which agrees with that use.
- `launchLine` builds a string, so its result is a string.
- `print` accepts the produced value.

This is **type inference**. The compiler still checks the types; it simply works out the obvious parts instead of asking you to repeat them.

Annotations are valuable when they add a real boundary or resolve ambiguity. Rewriting facts the compiler already knows adds noise. Chapter 3 develops that judgement.

## Let the compiler catch one mistake

Change the final call without adding a matching binding:

```osprey
launchLine(name, mission) |> print
```

Now run:

```sh
osprey first-flight.osp --check
```

The check must fail because `mission` has no binding. Diagnostic wording may change during alpha development, so this book does not freeze an invented error message. Read three pieces from the real output:

1. the source location;
2. the name or type the compiler could not resolve; and
3. the smallest change that makes the code tell the truth again.

Here the repair is either to use `project`, the name that already exists, or deliberately rename the binding and every use to `mission`. Guessing a new value would hide the mistake.

This failure is useful. The program did not start with a made-up value, silently print the wrong thing, or wait for a user to discover the problem. The compiler stopped at the boundary between what the source says and what it can prove.

### Try it: predict before you repair

Before editing, answer:

- Which names are in scope inside `main`?
- Which names are parameters inside `launchLine`?
- Would changing only the parameter name affect the call site?

Then make the smallest repair and rerun `--check` before `--run`.

## Same flight, different feathers — optional

You can skip this section without missing any Chapter 1 skill.

The currently available ML flavor can express the same program with indentation and different call spelling:

```osprey-ml
launchLine (name, project) = "${name} launched ${project}."

main () =
    name = "Mika"
    project = "a first Osprey program"
    launchLine (name, project) |> print
```

The source file ends in `.ospml`. The compiler selects the ML surface from that extension, then checks and builds the resulting Osprey program through the shared language pipeline.

Notice what did not change: the values, the function's job, the immutable bindings, the interpolation, the pipeline, and the output.

Some flavor differences are deeper than removing `fn` or braces. ML functions are curried by default, and a flat multi-argument call uses its own spelling. That is why Chapter 13 gives flavor switching a proper treatment instead of presenting it as automatic punctuation replacement.

The important emotional fact is simpler: you are not choosing a lifelong camp. Learn Osprey through Default. Try another surface later if it helps you read or write. More flavors can fit the architecture in the future.

## Flight Log checkpoint

Create `flight-log.osp` from the completed example:

```osprey
fn launchLine(name, project) = "${name} launched ${project}."

fn main() = {
    let name = "Mika"
    let project = "a first Osprey program"
    launchLine(name, project) |> print
}
```

Make three changes:

1. Replace `Mika` with your name or handle.
2. Replace the project description with something you want to build.
3. Rename `launchLine` to a name that still explains its job.

Run the check, then the program:

```sh
osprey flight-log.osp --check
osprey flight-log.osp --run
```

Your landing condition is one personalised line of output and a successful check.

## Agent handoff

Paste this into a coding agent when you want help without handing over the whole lesson:

```text
Update flight-log.osp in Default flavor.

Keep the program in one file and preserve its single-line output shape:
"<name> launched <project>."

Use one pure function to build the string, immutable let bindings in main,
and a pipeline into print. Do not add type annotations the compiler can infer.
Show the smallest diff, then run:

osprey flight-log.osp --check
osprey flight-log.osp --run

Report the observed output. Do not translate to another flavor unless asked.
```

For an optional flavor experiment, ask the agent to create an `.ospml` twin without modifying the `.osp` source. Require both files to pass `--check` and produce byte-identical output. The twin is evidence of translation, not a replacement for the teaching source.

## What changed

- You ran a complete Osprey program from one file.
- `main` provided the starting function and `print` made the result visible.
- A function parameter received an argument and produced a new value.
- `let` gave stable names to immutable values.
- The compiler inferred ordinary string types without losing type checking.
- `|>` made the data flow read from left to right.
- A failed check became useful information before the program ran.
- Default flavor carried the lesson; ML remained an optional alternate surface.

Chapter 2 keeps the program small and asks a deeper question: what makes a name useful, and what can you calculate without making a value change under your feet?

## Authoritative sources

- Osprey [Introduction](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0001-Introduction.md) for the language shape and explicit-failure contract.
- Osprey [Syntax](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0003-Syntax.md) for Default bindings, functions, and expressions.
- Osprey [Type System](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0004-TypeSystem.md) for Hindley–Milner inference.
- Osprey [Function Calls](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0005-FunctionCalls.md) for Default call behavior.
- Osprey [Iterators and Iteration](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0010-LoopConstructsAndFunctionalIterators.md) for the pipe operator.
- Osprey [Language Flavors](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0023-LanguageFlavors.md) and [ML Flavor Syntax](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0024-MLFlavorSyntax.md) for the optional comparison.
- The maintained [installation guide](https://www.ospreylang.dev/docs/installation/) for local and no-install paths.
