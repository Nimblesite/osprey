---
mlTwins: manual
layout: page
title: Python flavor for AI agents
description: Operating manual for AI coding agents. Every loose .py script an agent writes is Osprey source in the Python flavor, compiled to a native binary by the osprey compiler. Covers the toolchain, the CLI, PEP annotation syntax, algebraic effects and handlers, the rejected Python constructs, and the mandatory Bug-issue procedure when a script fails.
permalink: /docs/python-flavor-for-ai/
tags: [flavors, python, algebraic-effects, ai-agents]
---

**Audience: AI coding agents (LLMs) with no prior knowledge of Osprey.** This page is written for machine consumption: it is exhaustive, imperative and assumes you will copy its patterns verbatim. Human readers want the [Python flavor guide](/docs/python-flavor/).

## Mandatory rules

1. **Every loose `.py` script you write is Osprey source in the Python flavor.** Run it with `osprey script.py --run`. Never run it with `python`, `python3`, `uv`, `pip` or any CPython tool; CPython cannot execute it.
2. **Line 1 of every file is `# osprey: flavor=python`.** No exceptions, even though `.py` alone selects the flavor when the file is passed to `osprey` directly.
3. **The code is statically typed and PEP-compliant.** Formatting follows [PEP 8](https://peps.python.org/pep-0008/) (four-space indent, `snake_case` functions and bindings, two blank lines between top-level definitions). Annotations, when you write them, use [PEP 484](https://peps.python.org/pep-0484/), [PEP 526](https://peps.python.org/pep-0526/), [PEP 585](https://peps.python.org/pep-0585/), [PEP 591](https://peps.python.org/pep-0591/) and [PEP 695](https://peps.python.org/pep-0695/) notation. The only non-PEP constructs are the algebraic-effect forms: `@effect`, `@static_effect`, `@control`, `@handler(E)`, `@static_handler(E[T])`, `@requires(...)`, `perform(...)`, `resume(...)`, `update(...)` and handler `with` blocks.
4. **Every value has a static type, checked before the program runs, but you omit every annotation inference can derive.** Osprey uses Hindley–Milner type inference. A redundant annotation compiles but emits `warning: redundant type annotation on ...`. Treat every warning as a defect and delete the annotation. Annotate only where the compiler reports it cannot infer a type (see [Annotations you must write](#annotations-you-must-write)).
5. **Integer `+`, `-`, `*`, `/`, `//` and `%` on run-time values require an `Arith` handler.** A program that does such arithmetic outside a handler for the built-in `Arith` effect is rejected; only constant expressions (`2 + 3`, `7 // 2`) are exempt. Copy an [arithmetic policy](#arithmetic-needs-a-policy) into every script that does integer arithmetic.
6. **No exceptions, no reassignment, no loops.** Expected failure is a `Result` value matched with `match`; every binding and every value is immutable; iteration is `range`, `map`, `filter`, `fold`, `forEach`, `forEachList` and recursion.
7. **🚨 CRITICAL: when a script fails, file a GitHub issue of type Bug.** See [Report every failure as a Bug](#report-every-failure-as-a-bug). Do not silently work around a compiler failure.

## Report every failure as a Bug

Whenever an Osprey script, an `osprey` command, or a `make` target in the Osprey repository fails, log a GitHub issue **with the issue type `Bug`** on [Nimblesite/osprey](https://github.com/Nimblesite/osprey/issues). This applies to:

- The compiler crashing, panicking, hanging, or printing an internal error.
- The compiler rejecting a program that this page says is valid.
- A compiled program printing the wrong output, exiting with the wrong status, or crashing.
- A diagnostic that is wrong, unclear, or points at the wrong source position.
- A command or `make` target documented on this site failing.

Do this procedure every time:

1. Reduce the failure to the smallest `.py` file that still reproduces it.
2. Search for an existing open issue first, using a distinctive fragment of the error text:

   ```bash
   gh issue list --repo Nimblesite/osprey --state open --search "redundant type annotation"
   ```

   If one matches exactly, add the new reproduction as a comment (`gh issue comment <number> --repo Nimblesite/osprey --body-file report.md`) instead of filing a duplicate.
3. Otherwise write `report.md` with every section below, then file it with `--type Bug`:

   ````markdown
   ## Summary
   One sentence: what failed.

   ## Environment
   - osprey: output of `osprey --version`
   - OS: output of `uname -a` (or the Windows version)
   - clang: first line of `clang --version`

   ## Command
   ```bash
   osprey repro.py --run
   ```

   ## Source (repro.py)
   ```python
   # osprey: flavor=python
   ...
   ```

   ## Expected
   What this page or the specification says should happen.

   ## Actual
   The complete stdout and stderr, unedited, plus the exit status (`echo $?`).
   ````

   ```bash
   gh issue create --repo Nimblesite/osprey --type Bug \
     --title "Python flavor: <short symptom>" \
     --body-file report.md
   ```

4. Put the issue URL in your final report to the user, then continue the task using a documented alternative construct if one exists.

The issue type is the `Bug` issue type, set with `--type Bug`. It is not a label: do not substitute `--label bug`. If `gh` is not authenticated (`gh auth status` fails), tell the user that the issue could not be filed and include the full `report.md` text in your reply.

## What Osprey is

Osprey is a statically typed functional programming language compiled ahead of time to native machine code through LLVM. Its defining features:

- **Algebraic effects and handlers.** A function requests an operation (logging, storage, randomness, arithmetic overflow policy, tensor maths) without choosing how it is carried out. The caller installs a handler that answers it. Swapping the handler swaps the implementation, for example a real database in production and an in-memory fake in a test, without passing service objects through every function.
- **Hindley–Milner type inference.** Every type is known at compile time without annotations. There is no `Any` fallback for missing annotations and no runtime type check.
- **Immutable data, `Result` for failure, exhaustive `match`.** No `null`, no exceptions.
- **Flavors.** A flavor is a surface syntax. The Default flavor (`.osp`, braces), the ML flavor (`.ospml`, indentation and currying) and the Python flavor (`.py` with the marker) each parse their own concrete syntax tree (CST) and lower it into one canonical abstract syntax tree (AST). Type inference, effect checking, modules, memory management and code generation only ever see that AST. A project can mix flavors and import across them.
- **Targets.** Native executables (default), WebAssembly (`--target=wasm32`), C ABI libraries for iOS and Android, and CPython extension modules (`--target=python`).
- **Isolated fibers** for concurrency: lightweight threads that share no memory and communicate by message passing.

Osprey is actively evolving. Expect missing features; that is what the [Bug procedure](#report-every-failure-as-a-bug) is for.

### The Python flavor is not Python

| | Python flavor | Python (CPython) |
| --- | --- | --- |
| Language | Osprey | Python |
| Line 1 | `# osprey: flavor=python` | anything |
| Executed by | `osprey`, which compiles to a native binary | the CPython interpreter |
| Type checking | Hindley–Milner, before anything runs | none at run time; optional external checkers |
| Data | immutable records and lists | mutable objects |
| Failure | `Result` values | exceptions |
| `import` | Osprey modules only | Python packages |
| Integers | signed 64-bit with an `Arith` policy | arbitrary precision |

Python packages (`os`, `sys`, `json`, `re`, `numpy`, `requests`, …) do not exist. Use the [built-in functions](#built-in-functions). To call real CPython code from Osprey, see [Python interop](/spec/0042-pythoninterop/); that is a separate, explicit foreign-function boundary.

## Install the toolchain

Osprey emits LLVM IR and shells out to `clang` (LLVM 15 or newer) to produce and link the executable. `clang` must be on `PATH`, or named by `OSPREY_CC`.

**Inside a checkout of the Osprey repository** (`github.com/Nimblesite/osprey`), build the compiler from source and use the built binary:

```bash
make build                  # C runtime archives + Rust workspace (release) + VS Code extension
./target/release/osprey --version
```

The binary is `target/release/osprey`. Put it on `PATH` for the session (`export PATH="$PWD/target/release:$PATH"`) or call it by path. `cargo build --release -p osprey-cli` rebuilds only the compiler once the runtime archives exist (`make build` creates them).

**Anywhere else**, install a release:

```bash
brew install nimblesite/tap/osprey            # macOS and Linux
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket && scoop install osprey   # Windows
```

On macOS without a full LLVM, `xcode-select --install` provides a working `clang`. Full instructions and troubleshooting: [Installing Osprey](/docs/installation/).

Verify before writing code:

```bash
clang --version
osprey --version
```

## Run a script

```python
# osprey: flavor=python
"""Prints a greeting."""


def main():
    print("hello from the Python flavor")
```

```bash
osprey hello.py --run
```

```text
hello from the Python flavor
```

`main()` is the entry point; it takes no parameters. The `if __name__ == "__main__":` idiom does not exist and is rejected. A leading triple-quoted string is a documentation comment, not a value.

### CLI reference

`osprey <file-or-project> [mode] [options]`. The mode defaults to `--check` for a file. **Without `--run` nothing executes.**

| Command | Effect |
| --- | --- |
| `osprey f.py` | Type-check only (same as `--check`). Prints `f.py: ok (N statements)` and exits 0 on success |
| `osprey f.py --run` | Compile to a temporary native binary and run it; the exit status is the program's |
| `osprey f.py --compile -o app` | Compile a native executable to `./app` |
| `osprey f.py --ast` | Print the canonical AST |
| `osprey f.py --llvm` | Print the generated LLVM IR |
| `osprey f.py --symbols` | Print the file's symbols as JSON |
| `osprey f.py --run --memory=arc` | Choose the memory backend: `default` (malloc passthrough), `gc` (tracing collector) or `arc` (reference counting) |
| `osprey f.py --compile --target=wasm32` | WebAssembly. Other targets: `native` (default), `ios`, `ios-sim`, `android-arm64`, `android-x64`, `python` |
| `osprey f.py --run --sandbox` | Deny HTTP, WebSocket, file system and FFI. Individually: `--no-http`, `--no-websocket`, `--no-fs`, `--no-ffi` |
| `osprey f.py --run --quiet` | Suppress compiler progress output |
| `osprey f --flavor python` | Force the Python flavor regardless of extension |
| `osprey build [dir]` | Build the project whose `osprey.toml` is in `dir` (default `.`) |
| `osprey test [path] [--filter name]` | Run Osprey tests |
| `osprey fmt [--check \| --stdout] f.py` | Format in place, check formatting, or print formatted source |
| `osprey --version` | Print the compiler version |

Compile errors are printed to stderr prefixed with the file name, plus `line:column` when the error has a source position, and the exit status is non-zero. Standard input reaches the program (`input()` reads one line), so `echo 42 | osprey f.py --run` works.

## Types

| Annotation (PEP spelling) | Osprey type | Notes |
| --- | --- | --- |
| `int` | `int` | signed 64-bit; overflow goes to the `Arith` handler |
| `float` | `float` | IEEE 754 binary64 |
| `bool` | `bool` | `True`, `False`; the only type allowed as a condition |
| `str` | `string` | UTF-8, immutable |
| `None` | `Unit` | the "no value" result; not a nullable value |
| `list[T]` (or `List[T]` from `typing`) | `List<T>` | immutable, homogeneous |
| `dict[str, V]` (or `Dict[str, V]`) | `Map<string, V>` | immutable, string keys |
| `Callable[[A, B], R]` | `(A, B) -> R` | import from `typing` or `collections.abc`, not both; inside an effect operation signature, name it with a `type` alias |
| `Result[T, E]` | `Result<T, E>` | `Success(value=...)` or `Error(message=...)` |
| `Any` | `any` | must be pattern-matched before use; avoid |
| `class Name: field: T` | record | closed, immutable |
| `type Name = T` | type alias | PEP 695 |
| `class Box[T]:` | generic record | PEP 695 type parameters |

`from typing import Callable, Final, List, Dict, Any` and `from collections.abc import Callable` are compile-time annotation imports. They load nothing at run time. Importing the same name from both modules is rejected with ``binding `Callable` cannot be reassigned``.

### Annotations you must write

The compiler tells you when it needs one. Write an annotation only when:

- An empty literal has no other context: `empty: list[int] = []`.
- A function's `Result` type is not fixed by its body, for example a function whose only `return` sits inside a `with` handler block (`def safely(work) -> Result[int, str]:` in [control operations](#control-operations-and-resume)).
- An `@extern` declaration: every parameter and the return type are mandatory.
- An effect operation signature (inside `@effect` or `@static_effect`): every parameter and the return type are mandatory. A function-typed parameter must use a `type` alias (`type Kernel = Callable[[int], float]`, then `kernel: Kernel`); writing `Callable[[int], float]` inline there is rejected.
- A function-valued record field (`def discount(amount: int) -> int: ...` inside a plain `class`): fully annotated.

Everywhere else, remove the annotation. The `redundant type annotation` warning names each one.

## Bindings and functions

```python
# osprey: flavor=python

@handler(Arith)
class wrapping:
    def overflow(_op, _left, _right, wrapped):
        return wrapped


def scale(value, factor):
    return value * factor


def classify(n):
    if n > 100:
        return "large"
    elif n > 10:
        return "medium"
    else:
        return "small"


def main():
    twice = lambda value: scale(value, 2)
    with wrapping:
        print(f"{scale(value=6, factor=7)} {twice(21)} {classify(500)} {classify(5)}")
```

```text
42 42 large small
```

- `name = value` binds an immutable name. Binding the same name again is rejected with `state mutation is only allowed inside an effect handler arm`; choose a new name instead. Augmented assignment (`x += 1`), tuple unpacking (`a, b = 1, 2`), attribute assignment (`a.b = c`) and index assignment (`a[i] = c`) are rejected.
- `def` declares a flat function. There are no default argument values, `*args`, `**kwargs`, argument unpacking, currying or implicit `self`.
- A call passes its arguments either **all positionally or all by keyword**, never mixed.
- `lambda params: expr` is an anonymous function. It is a single expression.
- **`return` may appear only in tail position**: as the last statement of a function, or as the last statement of every branch of a final `if`/`elif`/`else` or `match`. Early returns are rejected. A function body is one expression; prefer conditional expressions (`a if cond else b`) over statements.
- A function with no `return` returns `None` (`Unit`). `pass` is an empty `Unit` body.
- A non-tail `if` or `match` must produce `None` in every branch.
- `and`, `or`, `not` take and return `bool` only, with short-circuiting. There is no truthiness: `if items:` is a type error (`cannot unify List<int> with bool`); write `if not isEmpty(items):`.
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`, on two operands of the same type (`cannot compare int with float` otherwise). Chained comparisons (`a < b < c`) and `is` are rejected; write `a < b and b < c`.
- `/` always produces a `float` (`7 / 2` is `3.5`, `10 / 2` is `5.0`). `//` is integer division (`intDiv`) and `%` is the remainder. Both truncate toward zero, not Python's floor: `-7 // 2` is `-3` and `-7 % 3` is `-1`.
- Mixed `int` and `float` arithmetic promotes to `float`: `1 + 2.0` is `3.0`. `toFloat(n)` converts explicitly.

## Strings

- Literals: `"..."` or `'...'`, Python escapes (`\n`, `\t`, `\"`, `\\`).
- f-strings interpolate any printable value: `f"{name} has {count} items"`. Format specifiers (`{x:.2f}`), conversions (`{x!r}`) and the debug form (`{x=}`) are not supported.
- `+` concatenates two strings: `"a" + toString(1)`.
- Use one interpolated `print` instead of consecutive `print` calls.
- Every string function is listed in [Built-in functions](#built-in-functions). Functions that can fail (`substring`, `split`, `replace`, `indexOf`, `parseInt`, `parseFloat`) return `Result`.

## Records, Result and match

```python
# osprey: flavor=python

@handler(Arith)
class wrapping:
    def overflow(_op, _left, _right, wrapped):
        return wrapped


class Account:
    owner: str
    balance: int


def withdraw(account, amount):
    if amount > account.balance:
        return Error(message=f"{account.owner} has only {account.balance}")
    else:
        return Success(value=update(account, balance=account.balance - amount))


def report(result):
    match result:
        case Success(value=value):
            return f"{value.owner} now has {value.balance}"
        case Error(message=message):
            return f"declined: {message}"


def main():
    alice = Account(owner="Alice", balance=100)
    with wrapping:
        print(f"{report(withdraw(alice, 30))}\n{report(withdraw(alice, 300))}")
```

```text
Alice now has 70
declined: Alice has only 100
```

- A plain `class` with annotated attributes is a **closed immutable record**. It has no methods, no `__init__`, no `self`, no base classes, no default values and no class variables.
- Construct with keyword arguments for every field: `Account(owner="Alice", balance=100)`. Missing, extra or wrongly typed fields are compile errors.
- `update(record, field=value)` returns a copy with the named fields changed; the original is untouched.
- Read a field with `record.field`.
- A signature-only `def f(x: int) -> int: ...` inside a plain class declares a **function-valued field** that is supplied at construction: `Price(value=100, discount=lambda amount: amount)`. Calling `quote.discount(5)` calls the stored function.
- `Result` values are `Success(value=...)` and `Error(message=...)`. Built-in functions that fail return `Result`.
- `match` must be exhaustive. Patterns: literals (`case 0:`, `case "gpu":`), the wildcard `case _:`, and constructor patterns with keyword fields whose binding name equals the field name (`case Success(value=value):`). Binding a different name (`case Success(value=v):`) is not implemented yet.
- `unwrap_or(result, fallback)` returns the success value or evaluates `fallback` on `Error`.

## Lists, maps and iteration

```python
# osprey: flavor=python

@handler(Arith)
class wrapping:
    def overflow(_op, _left, _right, wrapped):
        return wrapped


def total(xs):
    return fold(range(len(xs)), 0, lambda acc, i: acc + unwrap_or(listGet(xs, i), 0))


def main():
    numbers = [3, 1, 4, 1, 5]
    with wrapping:
        squares = toList(map(range(1, 6), lambda n: n * n))
        print(f"{len(numbers)} {unwrap_or(listGet(numbers, 2), 0)} {total(squares)} {join(toList(map(range(len(squares)), lambda i: toString(unwrap_or(listGet(squares, i), 0)))), ",")}")
        forEachList(numbers, lambda n: print(f"n={n}"))
```

```text
5 4 55 1,4,9,16,25
n=3
n=1
n=4
n=1
n=5
```

- List literals: `[1, 2, 3]`. Lists are immutable. `listAppend`, `listPrepend`, `listConcat` and `listReverse` return new lists.
- `items[i]` and `listGet(items, i)` return `Result`; there is no `IndexError`. A negative or out-of-range index gives `Error`. Slices (`items[1:3]`) are rejected.
- **Iterators and lists are different types.** `range(stop)` and `range(start, stop)` produce an `Iterator<int>`. `map(it, f)`, `filter(it, p)`, `fold(it, seed, f)` and `forEach(it, f)` take an iterator, never a list; passing a list is a type error (`cannot unify Iterator<…> with List<…>`). `toList(it)` materialises a list.
- Walk a list either with `forEachList(xs, f)` (side effects) or by index over `range(len(xs))` with `listGet` (as `total` does above).
- A list cannot be interpolated into an f-string (`cannot convert value for interpolation: List<int>`). Convert each element with `toString` and `join` them, as above.
- `len(x)` is `length(x)` for strings, lists and maps.
- Maps (`dict[str, V]`): `mapGet` (returns `Result`), `mapSet`, `mapRemove`, `mapContains`, `mapKeys`, `mapValues`, `mapMerge`. Each returns a new map.
- Tuples, tuple unpacking, `zip`, `sum`, `max`, `min`, `sorted`, `enumerate` and generator expressions are not provided. Use `fold` and records.
- `for` loops and list comprehensions are specified but **not implemented yet**; the compiler rejects them with ``Python `…` has no supported Osprey lowering``. `while`, `break` and `continue` will never be supported. Use `map`, `filter`, `fold`, `forEach`, `forEachList` or recursion.

## Algebraic effects

This is the part of the Python flavor that is not Python. Read it fully.

### Concepts

- An **effect** is a named, typed interface of **operations**, declared without an implementation.
- **Performing** an operation (`perform(Log.write("x"))`) suspends the current computation and asks the nearest enclosing handler for that effect to answer.
- A **handler** supplies implementations for the operations of one effect. The caller, not the callee, chooses which handler is installed, so the same function can log to the console in production and to a list in a test.
- **Effect checking is static.** The compiler computes which operations a program performs, reaching through helper functions, lambdas passed to higher-order functions, and fibers. A program that performs an operation no installed handler answers is rejected with ``unhandled effect operations at program entry: E.op; add a matching `handle` ``.
- **Value operations** (the default) return a result to the code that performed them, like a function call.
- **Control operations** (`@control`) receive the rest of the computation: the handler arm either calls `resume(value)` to continue it, or returns its own value, which becomes the result of the whole handled block (an early exit, like an exception, but typed and explicit).
- Handler arms execute outside their own handler, so an arm can perform the same effect and be answered by an outer handler (forwarding).
- **Static effects** (`@static_effect`) are resolved entirely at compile time: the handler is selected statically and specialised into the code, so there is no run-time handler lookup. GPT-2 uses one to choose CPU or GPU execution.

### Declare an effect

```python
# osprey: flavor=python
@effect
class Log:
    def write(message: str) -> None: ...
    def count() -> int: ...
```

- `@effect` turns the class into an effect declaration. It is not a class at run time.
- Each operation is a signature with **complete parameter and return annotations** and a body of exactly `...`. There is no `self` parameter.
- Generic effects use PEP 695 parameters: `class Tensor[B]:`.

### Perform an operation

```python
# osprey: flavor=python
def load(name):
    perform(Log.write(f"loading {name}"))
    return len(name)
```

`perform(Effect.operation(args))` is the only way to request an operation. The argument is syntax naming the operation, not a call evaluated first. `load` does not know or care which handler answers.

### Define a handler

```python
# osprey: flavor=python
@handler(Log)
class console:
    def write(message):
        print(f"log: {message}")

    def count():
        return 0
```

- `@handler(E)` turns the class into a **handler function**. Each method is the arm for the operation of the same name. Arms take no `self`; their parameter and return types are inferred from the effect declaration, so leave them unannotated.
- A handler may leave out an operation. Performing an operation that no installed handler answers is rejected as unhandled (`Log.count` here if `console` had no `count` arm).
- Declaring a handler runs nothing.
- Handler names follow function naming rules (`snake_case`).

### Install a handler

Two equivalent forms:

```python
# osprey: flavor=python
def main():
    with console:
        print(f"loaded {load("gpt2")}")
    console(lambda: print(f"loaded {load("weights")}"))
```

- `with handler:` runs the indented suite under the handler. It is not a context manager: `as`, multiple handlers in one `with`, and arbitrary expressions are rejected. Nest `with` blocks to install several handlers.
- `handler(work)` runs the zero-argument function `work` under the handler and returns its result.
- A `with` block in tail position supplies the function's result (`return` inside it). A `with` block in statement position must produce `None`.

### Complete example

The same pieces as one runnable file (`count` is left out because nothing performs it):

```python
# osprey: flavor=python

@effect
class Log:
    def write(message: str) -> None: ...


@handler(Log)
class console:
    def write(message):
        print(f"log: {message}")


@handler(Log)
class audit:
    def write(message):
        print(f"audit: {message}")


def load(name):
    perform(Log.write(f"loading {name}"))
    return len(name)


def main():
    with console:
        print(f"loaded {load("gpt2")}")
    audit(lambda: print(f"loaded {load("weights")}"))
```

```text
log: loading gpt2
loaded 4
audit: loading weights
loaded 7
```

### Control operations and resume

```python
# osprey: flavor=python

@effect
class Ask:
    @control
    def number(prompt: str) -> int: ...


@handler(Ask)
class answer_42:
    def number(_prompt):
        return resume(42)


def main():
    with answer_42:
        print(f"asked: {perform(Ask.number("pick"))}")
```

```text
asked: 42
```

- `@control` above an operation makes it a control operation.
- `resume(value)` continues the computation that performed the operation, with `value` as the operation's result. It is allowed only inside a control arm. Resumption is single-shot: call `resume` at most once per arm invocation.
- An arm parameter you do not use must start with `_` (`_prompt`); otherwise the compiler warns `unused handler parameter`.
- A control arm that returns without calling `resume` ends the whole handled block; its return value becomes the block's result. This is how to exit early with an error:

```python
# osprey: flavor=python

@effect
class Fault:
    @control
    def halt(message: str) -> int: ...


@handler(Fault)
class halting:
    def halt(message):
        return Error(message=message)


@handler(Arith)
class arithmetic:
    def overflow(op, l, r, _):
        return perform(Fault.halt(f"integer overflow: {l} {op} {r}"))

    def divideByZero(op, l):
        return toFloat(perform(Fault.halt(f"division by zero: {l} {op} 0")))

    def remainderByZero(l):
        return perform(Fault.halt(f"remainder by zero: {l} % 0"))


def checked_div(a, b):
    return perform(Fault.halt(f"{a} / 0")) if b == 0 else intDiv(a, b)


def safely(work) -> Result[int, str]:
    with halting:
        with arithmetic:
            return Success(value=work())


def show(result):
    match result:
        case Success(value=value):
            return f"ok {value}"
        case Error(message=message):
            return f"fault {message}"


def main():
    print(f"{show(safely(lambda: checked_div(10, 2)))} {show(safely(lambda: checked_div(1, 0)))}")
```

```text
ok 5 fault 1 / 0
```

`halting` turns any `Fault.halt` into an `Error` result for the whole `with halting:` block. `safely` installs it outside `arithmetic`, so an arithmetic fault is forwarded to `Fault` too. The annotation `-> Result[int, str]` is required here, because the body alone does not fix the error type.

### Arithmetic needs a policy

Integer overflow, division by zero and remainder by zero perform operations of the built-in `Arith` effect. You must choose the policy; the compiler never picks one. Only run-time operands perform them; constant expressions such as `2 + 3 * 4` or `7 // 2` are evaluated by the compiler and need no handler.

| Operation | Performed by | Arm returns |
| --- | --- | --- |
| `overflow(op, left, right, wrapped)` | `+`, `-`, `*` on run-time ints | `int`; `wrapped` is the two's-complement result |
| `divideByZero(op, left)` | `/` with a run-time int divisor | `float` |
| `remainderByZero(left)` | `//` or `%` with a run-time int divisor | `int` |

A handler needs an arm only for the operations the program performs; the diagnostic names each missing one (``unhandled effect operations at program entry: Arith.remainderByZero``). Wrapping is the minimal policy for code that never divides by a run-time value:

```python
# osprey: flavor=python
@handler(Arith)
class wrapping:
    def overflow(_op, _left, _right, wrapped):
        return wrapped
```

A total policy answers all three operations and never stops:

```python
# osprey: flavor=python

@handler(Arith)
class total:
    def overflow(_op, _left, _right, wrapped):
        return wrapped

    def divideByZero(_op, _left):
        return 0.0

    def remainderByZero(_left):
        return 0


def main():
    n = len(input())
    with total:
        print(f"{10 // n} {10 / n} {10 % n} {n * 3} {10 // (n - 2)} {10 / (n - 2)}")
```

```bash
echo hi | osprey total.py --run
```

```text
5 5.0 0 6 0 0.0
```

To stop on the first fault instead, use the `arithmetic` handler forwarding to `Fault`, as shown in [control operations](#control-operations-and-resume). Alternatively avoid the effect with total built-ins: `wrapAdd`, `wrapSub`, `wrapMul`, `satAdd`, `satSub`, `satMul`, or `checkedAdd`, `checkedSub`, `checkedMul` (which return `Result`). Float arithmetic needs no handler. See [arithmetic effects](/spec/0037-arithmeticeffects/).

### Declare required effects

`@requires(Log, Arith)` above a `def` states exactly which effects the function may perform; the compiler checks it. `@requires()` declares a pure function. Inference is the default, so add `@requires` only to pin a public API.

### Static effects and static handlers

```python
# osprey: flavor=python
from typing import Callable

type Kernel = Callable[[int], float]


@static_effect
class Tensor[B]:
    def build(size: int, kernel: Kernel) -> B: ...
    def get(buffer: B, index: int) -> float: ...


@static_handler(Tensor[list[float]])
class on_lists:
    def build(size, kernel):
        return toList(map(range(size), kernel))

    def get(buffer, index):
        return unwrap_or(listGet(buffer, index), 0.0)


def main():
    with on_lists:
        print(f"{perform(Tensor.get(perform(Tensor.build(4, lambda i: toFloat(i) * 2.0)), 3))}")
```

```text
6.0
```

A static effect is generic over a representation (`B`), and each static handler fixes it (`Tensor[list[float]]`, `Tensor[GpuBuffer[float]]`). Code performing `Tensor` operations never names `B`; the installed static handler decides it at compile time. Static operations cannot be control operations. The full working example is [GPT-2 in the Python flavor](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python): `backends.py` declares the effect, `cpu.py` and `gpu.py` are the two handlers, and `model.py` is written once against the effect.

## Built-in functions

Built-ins need no import. Names are camelCase because they are shared by every flavor; `len`, `range`, `update`, `unwrap_or` and the fiber intrinsics are the Python-flavor spellings. The authoritative list with signatures is [Built-in functions](/spec/0012-built-infunctions/).

| Area | Functions |
| --- | --- |
| I/O | `print(value)`, `input()` (one stdin line), `toString(value)` |
| Numbers | `abs`, `toFloat`, `intDiv`, `sqrt`, `exp`, `ln`, `tanh`, `random()` (non-negative `int`), `randomBelow(n)` (Result), `wrapAdd`/`wrapSub`/`wrapMul`, `satAdd`/`satSub`/`satMul`, `checkedAdd`/`checkedSub`/`checkedMul` (Result) |
| String inspection | `length`/`len`, `isEmpty`, `contains`, `startsWith`, `endsWith`, `indexOf` (Result), `byteLength`, `byteAt` (Result), `codePointAt` (Result), `fromCodePoint` (Result) |
| String transformation | `substring` (Result), `take`, `drop`, `split` (Result), `join(parts, sep)`, `lines`, `words`, `toUpperCase`, `toLowerCase`, `trim`, `trimStart`, `trimEnd`, `replace` (Result), `repeat` (Result), `reverse`, `padStart`/`padEnd` (Result) |
| Parsing | `parseInt(s)` (Result), `parseFloat(s)` (Result) |
| Files | `readFile(path)` (Result), `writeFile(path, content)` (Result), `loadTensor(path, name)` (Result) |
| Processes | `spawnProcess(command, callback)` (Result), `awaitProcess(id)`, `cleanupProcess(id)` |
| Lists | `listGet` (Result), `listPrepend`, `listAppend`, `listConcat`, `listReverse`, `listContains`, `forEachList`, `isEmpty` |
| Maps | `mapGet` (Result), `mapContains`, `mapSet`, `mapRemove`, `mapMerge`, `mapKeys`, `mapValues` |
| Iterators | `range`, `map`, `filter`, `fold`, `forEach`, `toList` |
| Fibers | `spawn(work)`, `await_fiber(fiber)`, `yield_fiber()`, `send(...)`, `recv(...)` |
| HTTP, WebSocket, JSON, terminal, GPU | see the [specification](/spec/) chapters 0012, 0014, 0015 and 0034 |

`perform`, `resume`, `update`, `unwrap_or`, `spawn`, `await_fiber`, `yield_fiber`, `send` and `recv` are reserved and cannot be rebound.

Read a file and handle failure:

```python
# osprey: flavor=python

def main():
    match readFile("notes.txt"):
        case Success(value=value):
            print(f"read {len(value)} characters")
        case Error(message=message):
            print(f"cannot read notes.txt: {message}")
```

## Fibers

`spawn(work)` starts a fiber running the zero-argument function `work` and returns a handle; `await_fiber(handle)` waits for its result. Fibers share no memory: pass data in through the closure and out through the result or channels (`send`, `recv`). `async def`, `await`, `yield` and threads are rejected. See [fibers and concurrency](/spec/0011-lightweightfibersandconcurrency/).

## Modules and projects

A single file needs no project. For several files, write `osprey.toml`:

```toml
[project]
name = "tool"
source_roots = ["src"]
python_sources = ["src/main.py", "src/pricing.py"]
default_namespace = "tool"
entry = "src/main.py"
```

- `python_sources` lists every Python-flavor file explicitly. Unlisted `.py` files under a source root are ignored, so real Python scripts can live alongside.
- Each module file starts with directives after the flavor marker:

  ```python
  # osprey: flavor=python
  # osprey: namespace=tool
  # osprey: module=Pricing
  ```

- `__all__ = ["discounted"]` (a literal list of names) or `@export` on a declaration makes it public. Everything else is module-private.
- Import with `from tool.Pricing import discounted`, `from tool import Pricing` or `import tool.Pricing as Pricing`. Imports name Osprey modules, in any flavor; wildcard and relative imports are rejected.
- Build and run with `osprey build` from the directory holding `osprey.toml`, or `osprey . --run`.

## Rejected Python constructs and their replacements

| Python construct | Status | Write instead |
| --- | --- | --- |
| `import os`, `import json`, any package | rejected (``unknown import target `os` ``) | built-in functions |
| `while`, `break`, `continue`, loop `else` | rejected | `fold`, `forEach`, recursion |
| `for x in xs:` | not implemented yet | `forEachList(xs, lambda x: ...)`; `forEach(range(n), lambda i: ...)` |
| `[f(i) for i in range(n)]` | not implemented yet | `toList(map(range(n), f))` |
| `[f(x) for x in xs]` | not implemented yet | `toList(map(range(len(xs)), lambda i: f(unwrap_or(listGet(xs, i), fallback))))` |
| `try`/`except`/`finally`, `raise` | rejected | return `Result`; `@control` effect for early exit |
| `async def`, `await`, `yield` | rejected | fibers |
| class methods, `__init__`, `self`, inheritance, dunders | rejected | free functions over records; function-valued fields |
| `x = 2` after `x = 1`, `x += 1`, `obj.attr = v`, `xs[i] = v` | rejected | `update(record, field=v)`, a new name |
| early `return` | rejected | conditional expression or a final `if`/`match` |
| `if xs:` (truthiness) | type error | explicit `bool` test such as `not isEmpty(xs)` |
| `a < b < c`, `is`, `is not` | rejected | `a < b and b < c`, `==` |
| tuples, `a, b = pair` | rejected | records |
| slices `xs[1:3]` | rejected | `take`, `drop`, `substring` |
| `xs[-1]` | returns `Error` | `listGet(xs, len(xs) - 1)` |
| default, `*args`, `**kwargs` parameters | rejected | explicit parameters, records |
| `f"{x:.2f}"`, `{x!r}`, `{x=}` | rejected | plain `{x}` |
| arbitrary decorators (`@staticmethod`, `@dataclass`, …) | rejected | only the effect decorators, `@extern`, `@export`, `@requires` |
| `@union` sum types | not implemented yet | records plus `Result` |
| `case Success(value=v):` (renamed binding) | not implemented yet | `case Success(value=value):` |
| `if __name__ == "__main__":` | rejected | `def main():` |
| `None` as a nullable value | `None` is `Unit`, never null | `Result` |

A construct marked "not implemented yet" is specified in the [Python flavor specification](/spec/0041-pythonflavorsyntax/). If you need it, file a Bug only when the compiler's behaviour differs from that status.

## Diagnostics you will see

| Diagnostic fragment | Cause | Fix |
| --- | --- | --- |
| `redundant type annotation on ...` | an annotation inference already derives | delete the annotation |
| `unhandled effect operations at program entry: Arith.overflow` | integer arithmetic with no `Arith` handler | wrap the work in `with wrapping:` or another policy |
| ``unhandled effect operations at program entry: E.op; add a matching `handle` `` | an operation performed with no handler installed | install a handler for `E` around the call |
| `unhandled effect operations at program entry: Arith.divideByZero` or `Arith.remainderByZero` | `/`, `//` or `%` with a run-time divisor and no matching arm | add the named arm (see the `total` policy) |
| ``Python `…` has no supported Osprey lowering`` | a Python construct outside the supported surface | use the replacement in the table above |
| `state mutation is only allowed inside an effect handler arm` | a name bound twice | bind a new name |
| `call arity mismatch: expected 2 argument(s), got 1` | positional and keyword arguments mixed in one call | pass all positionally or all by keyword |
| `type mismatch: cannot unify Iterator<…> with List<…>` | a list passed to `map`, `filter`, `fold` or `forEach` | iterate `range(len(xs))` or use `forEachList` |
| `type mismatch: cannot unify List<int> with bool` | truthiness test on a list | `not isEmpty(xs)` |
| `cannot convert value for interpolation: List<…>` | a list inside an f-string | `join` the `toString` of each element |
| `unused handler parameter` | an arm parameter that is never read | prefix it with `_` |
| `a declaration signature must have exactly an ... body` | a method body in a plain `class` | move the logic into a free function |
| `error: could not invoke clang: No such file or directory` | no LLVM toolchain on `PATH`, or a wrong `OSPREY_CC` | install `clang` or set `OSPREY_CC` |

Anything else that blocks a program this page says is valid: [file a Bug](#report-every-failure-as-a-bug).

## Checklist before you finish

- [ ] Line 1 is `# osprey: flavor=python`.
- [ ] `osprey script.py` prints `ok` with zero warnings.
- [ ] `osprey script.py --run` produces the expected output and exit status.
- [ ] `osprey fmt --check script.py` passes (it normalises indentation).
- [ ] No redundant annotations, no consecutive `print` calls, no early `return`.
- [ ] Every integer `+ - * / // %` on run-time values runs under an `Arith` handler with the arms it needs.
- [ ] Every failure you hit has a GitHub issue of type `Bug`, and its URL is in your report.

## Reference

- [Python flavor guide](/docs/python-flavor/), the human-oriented introduction.
- [Python flavor specification](/spec/0041-pythonflavorsyntax/), the full contract.
- [Algebraic effects guide](/docs/effects/) and [effects specification](/spec/0017-algebraiceffects/).
- [GPT-2 in the Python flavor](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python), the largest working program.
