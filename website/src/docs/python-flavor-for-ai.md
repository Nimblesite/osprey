---
mlTwins: manual
layout: page
title: Python flavor for AI agents
description: Operating manual for AI coding agents. Every loose .py script an agent writes is Osprey source in the Python flavor, compiled to a native binary by the osprey compiler. Covers the toolchain, the CLI, PEP annotation syntax, algebraic effects and handlers, the rejected Python constructs, and the mandatory Bug-issue procedure when a script fails.
permalink: /docs/python-flavor-for-ai/
tags: [flavors, python, algebraic-effects, ai-agents]
---

**Audience: AI coding agents (LLMs) with no prior knowledge of Osprey.** This page records operating rules, supported constructs and links to runnable examples. Human readers want the [Python flavor guide](/docs/python-flavor/).

## Mandatory rules

1. **Every loose `.py` script you write is Osprey source in the Python flavor.** Run it with `osprey script.py --run`. Never run it with `python`, `python3`, `uv`, `pip` or any CPython tool; CPython cannot execute it.
2. **Line 1 of every file is `# osprey: flavor=python`.** No exceptions, even though `.py` alone selects the flavor when the file is passed to `osprey` directly.
3. **Use the supported Python annotation notation over Osprey semantics.** Formatting follows [PEP 8](https://peps.python.org/pep-0008/) (four-space indent, `snake_case` functions and bindings, two blank lines between top-level definitions). Annotations, when you write them, use [PEP 484](https://peps.python.org/pep-0484/), [PEP 526](https://peps.python.org/pep-0526/), [PEP 585](https://peps.python.org/pep-0585/), [PEP 591](https://peps.python.org/pep-0591/) and [PEP 695](https://peps.python.org/pep-0695/) notation. Osprey also supplies `@union` and the algebraic-effect forms: `@effect`, `@static_effect`, `@control`, `@handler(E)`, `@static_handler(E[T])`, `@requires(...)`, `perform(...)`, `resume(...)`, `update(...)` and handler `with` blocks.
4. **Every value has a static type, checked before the program runs, but you omit every annotation inference can derive.** Osprey uses Hindley–Milner type inference. A redundant annotation compiles but emits `warning: redundant type annotation on ...`. Treat every warning as a defect and delete the annotation. Annotate only where the compiler reports it cannot infer a type (see [Annotations you must write](#annotations-you-must-write)).
5. **Choose an `Arith` policy for fallible arithmetic.** The compiler names missing operations. Integer overflow and integer/float zero divisors have distinct arms; constants, proven-total sites and explicit total helpers need no handler. See [arithmetic policies](#arithmetic-needs-a-policy).
6. **Use Result, immutable data and iterator-based computation.** `for` and comprehensions lower to existing iterator operations. Mutation remains confined to handler-owned state; `Final` forbids reassignment. There are no language exceptions or `while` loops.
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

Osprey is a functional language with algebraic effects, inferred static types, exhaustive matching and LLVM compilation. A handler chooses how an operation runs. Native executables, WebAssembly, Apple/Android C ABI archives and Python-callable shared libraries use the same compiler. Default, ML and Python-flavor modules can share a project.

Osprey is actively evolving. Validate it against your workload before relying on it in production.

### The Python flavor is not Python

Python flavor keeps `def`, indentation, f-strings and supported PEP annotation notation over Osprey semantics. It has no CPython object model, implicit exceptions or runtime imports. Integers are signed 64-bit; conditions require `bool`; failures use `Result` or explicit effects.

An ordinary `import numpy` does not install or load CPython NumPy. Osprey source ports can be configured through the [package client](/docs/package-manager/) and imported by their logical module names. Calling a CPython library uses the separate [interop boundary](/spec/0042-pythoninterop/).

## Install the toolchain

Native compilation needs compatible LLVM/clang 15 or newer, found on `PATH` or selected with `OSPREY_CC`. Follow [Installing Osprey](/docs/installation/) for platform-specific dependencies, Homebrew and Scoop.

Inside this checkout, build and select the local compiler:

```bash
make build
export PATH="$PWD/target/release:$PATH"
osprey --version
clang --version
```

`make build` creates the runtime archives, release Rust workspace and VS Code extension. After the archives exist, `cargo build --release -p osprey-cli` rebuilds the compiler alone.

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
| `osprey f.py --run --memory=arc` | Choose the memory backend: `gc` (default tracing collector), `arc` (reference counting) or explicit `none` (unmanaged allocation) |
| `osprey f.py --compile --target=wasm32 --memory=none` | WebAssembly. Other targets: `native` (default), `ios`, `ios-sim`, `macos`, `android-arm64`, `android-x64`, `python` |
| `osprey f.py --run --sandbox` | Deny HTTP, WebSocket, file system, process and FFI capabilities. Individual subset flags: `--no-http`, `--no-websocket`, `--no-fs`, `--no-ffi` |
| `osprey f.py --run --quiet` | Suppress compiler progress output; warnings and errors still print |
| `osprey f.py --run --no-warnings` | Skip the warning pass entirely; errors still print |
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
| `Callable[[A, B], R]` | `(A, B) -> R` | import from `typing` or `collections.abc`; a `type` alias can name a callback signature |
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
- An effect operation signature (inside `@effect` or `@static_effect`): every parameter and the return type are mandatory. A named callback alias such as `type Kernel = Callable[[int], float]` can keep operation signatures readable. Callback effect provenance must still be provable.
- A function-valued record field (`def discount(amount: int) -> int: ...` inside a plain `class`): fully annotated.

Everywhere else, remove the annotation. The `redundant type annotation` warning names each one.

## Bindings and functions

The [functions example](/docs/python-flavor/#functions-and-inference) shows inferred parameters, named calls and an arithmetic handler.

- `name = value` starts a lexical binding. Prefer immutable bindings; reassignment is checked under the existing handler-owned state rules. Ordinary function-body assignment is rejected. `Final[T]` forbids reassignment. Augmented, field/index assignment and tuple unpacking are rejected.
- `def` declares a flat function; a nested `def` is rejected. Bind a lambda instead. There are no default arguments, `*args`, `**kwargs`, argument unpacking, currying or implicit `self`.
- Calls are fully positional or fully named. `lambda params: expr` is a single expression.
- `return` is allowed at the tail of a function or every branch of a final `if`/`match`. A non-tail conditional must produce `None` in every branch. A function without a tail return, or an empty `pass` body, produces `Unit`.
- `and`, `or`, `not` take booleans and short-circuit. There is no truthiness; write `not isEmpty(items)`.
- Compare operands of the same type. Chained comparisons and `is` are rejected; write `a < b and b < c`.
- `/` produces `float`. Integer `//` and `%` truncate toward zero: `-7 // 2` is `-3`; `-7 % 3` is `-1`. Their fallible sites need an arithmetic policy.
- Mixed integer/float arithmetic promotes to `float`. `toFloat(n)` converts explicitly.

## Strings

- Literals: `"..."` or `'...'`, Python escapes (`\n`, `\t`, `\"`, `\\`).
- f-strings interpolate any printable value: `f"{name} has {count} items"`. Format specifiers (`{x:.2f}`), conversions (`{x!r}`) and the debug form (`{x=}`) are not supported.
- `+` concatenates two strings: `"a" + toString(1)`.
- Use one interpolated `print` instead of consecutive `print` calls.
- Every string function is listed in [Built-in functions](#built-in-functions). Functions that can fail (`substring`, `split`, `replace`, `indexOf`, `parseInt`, `parseFloat`) return `Result`.

## Records, Result and match

The [record example](/docs/python-flavor/#records-result-and-match) shows construction, immutable updates and exhaustive Result handling.

- A plain `class` with annotated fields declares a closed immutable record. There is no inheritance, `__init__`, `self`, method dispatch, field default or class variable.
- Construct every field by keyword: `Account(owner="Alice", balance=100)`. Missing, extra or wrongly typed fields fail checking.
- `update(record, field=value)` creates a copy; `record.field` reads a field.
- A fully typed signature-only `def discount(amount: int) -> int: ...` inside a record declares a function-valued field supplied at construction, not a method.
- `Success(value=...)` and `Error(message=...)` construct `Result`. Match both cases or supply `unwrap_or(result, fallback)`, whose fallback is evaluated only on Error.
- `match` is exhaustive. Literal, wildcard and constructor patterns are supported. Keyword fields can bind a different name: `case Success(value=answer):` and `case Circle(radius=r):` work.
- `@union` on a class declares a sum type; each immediately nested record class supplies one variant. Construct a variant with its fields by keyword. See [union syntax](/spec/0041-pythonflavorsyntax/#closed-classes--flavor-python-class).

## Lists, maps and iteration

```python
# osprey: flavor=python

def main():
    squares = [wrapMul(n, n) for n in range(1, 4) if n > 0]
    text = join(toList(map(range(len(squares)), lambda i: toString(unwrap_or(listGet(squares, i), 0)))), ",")
    for i in range(2):
        print(f"{i}: {text}")
```

This prints `0: 1,4,9` and `1: 1,4,9` on separate lines. `wrapMul` chooses wrapping explicitly, so this example needs no arithmetic handler.

- Lists are immutable. `listAppend`, `listPrepend`, `listConcat` and `listReverse` return new lists. `items[i]` and `listGet(items, i)` return Result, including Error for negative/out-of-range indexes. Slices are rejected.
- `range(stop)` and `range(start, stop)` produce `Iterator[int]`. `map`, `filter`, `fold`, `forEach` and `for name in iterator:` consume iterators, not lists. Use `forEachList` or iterate list indexes as above. A loop variable stays inside its callback scope.
- List comprehensions accept the iterator surface, including filters and nested clauses; they produce host lists. They do not add arbitrary Python iterables or GPU placement.
- A list cannot be interpolated into an f-string. Convert elements with `toString` and `join` them. `len` handles strings, lists and maps.
- `mapGet` returns Result; `mapContains` returns bool; `mapKeys`/`mapValues` return lists. `mapSet`, `mapRemove` and `mapMerge` produce maps.
- `while`, loop `else`, `break`, `continue`, tuple unpacking, generator/set/dictionary comprehensions, `zip`, `sum`, `sorted` and `enumerate` are rejected. Use folds and records.

## Algebraic effects

### Concepts

An effect declares typed operations. `perform(E.op(...))` requests one; the caller installs its handler. Value operations return to the caller. A control arm can resume or answer for the whole handled block. Arms run outside their own activation and can forward to an outer handler. Requirements propagate through helpers, callbacks and fibers; missing handlers are compile errors.

### Declare an effect

`@effect` above a class declares an effect. Each operation is a fully typed `def` signature ending in `...`, with no `self`. Generic effects use PEP 695 type parameters.

### Perform an operation

`perform(Log.write("loading"))` names an operation syntactically; it does not evaluate a Python method call first.

### Define a handler

`@handler(Log)` declares a callable handler. Methods are operation arms with inferred parameters/results and no `self`. Creating it runs no work. An omitted arm remains an obligation for an outer handler.

### Install a handler

`with console:` runs its suite under the handler; `console(lambda: work())` runs a zero-argument callback. A tail `with` may supply the function result; a statement-position suite must produce Unit. Nest suites for several handlers. Arbitrary context managers, `as` and multiple context managers in one `with` are rejected.

### Complete example

The [Python effects example](/docs/python-flavor/#effects-and-handlers) includes the declaration, request, console/audit handlers, both installation forms and output. Use that complete program rather than combining partial declarations.

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

The compiler requires handlers for sites it cannot prove total. Constants and other proven-total sites need none. Host execution invokes the policy; current Metal/OpenCL kernels abort on faults instead of recovering through it. See the [device recovery limit](/spec/0037-arithmeticeffects/#device-recovery-arith-device-recovery).

| Operation | Fallible sites | Arm result |
| --- | --- | --- |
| `overflow(op, left, right, wrapped)` | integer `+`, `-`, `*`, unary `-`, `abs`, and minimum-integer divided by `-1` through `//`/`intDiv` | int |
| `divideByZero(op, left)` | `/` with any operands; `%` with a float operand | float |
| `remainderByZero(left)` | integer `//`/`intDiv` or `%` with a zero divisor | int |

A policy answering all three:

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
```

Use `with total:` around the computation. A smaller `wrapping` handler can provide only `overflow` with the same arm. A diagnostic names every missing operation. To stop at a fault, use the `Fault` forwarding example above.

`wrapAdd`/`wrapSub`/`wrapMul` and `satAdd`/`satSub`/`satMul` are total; `checkedAdd`/`checkedSub`/`checkedMul` return Result. Float `+`, `-`, `*`, unary minus and `abs` need no policy; float `/` and `%` still require zero-divisor handling unless proven safe. See [arithmetic effects](/spec/0037-arithmeticeffects/).

### Declare required effects

`@requires(Log, Arith)` above a `def` states exactly which effects the function may perform; the compiler checks it. `@requires()` declares a pure function. Inference is the default, so add `@requires` only to pin a public API.

### Static effects and static handlers

`@static_effect` declares operations resolved through a statically selected `@static_handler(E[T])`. Every static operation needs an answer; static operations cannot be control operations. Specialization removes runtime effect dispatch, not every function call or allocation.

GPT-2's [backends.py](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/python/backends.py) declares `Tensor[B]`; [cpu.py](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/python/cpu.py) answers with lists and [gpu.py](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/python/gpu.py) with GPU buffers. [model.py](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/python/model.py) performs the same operations under either handler. Static selection adds no automatic offloading.

## Built-in functions

Builtins need no import. The shared [function reference](/docs/functions/) and [builtin specification](/spec/0012-built-infunctions/) cover strings, numbers, files, processes, collections, JSON, networking, terminal control and GPU operations. Their camelCase names remain available in Python flavor.

`len`, one-argument `range`, `update`, `unwrap_or` and the fiber intrinsics have Python-flavor lowering. `perform`, `resume`, `update`, `unwrap_or`, `spawn`, `await_fiber`, `yield_fiber`, `send` and `recv` are reserved and cannot be rebound.

Handle fallible results explicitly: match `readFile(path)` as `Success(value=text)` or `Error(message=reason)`. File writes, parsing and indexing also return `Result`. Networking has a mixed interface: legacy HTTP/WebSocket handle and status builtins return integers, with negative values indicating errors; HTTP response acquisition, body/header access and release return `Result`. Check each function's reference signature.

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
| `try`/`except`/`finally`, `raise` | rejected | return `Result`; `@control` effect for early exit |
| `async def`, `await`, `yield` | rejected | fibers |
| class methods, `__init__`, `self`, inheritance, dunders | rejected | free functions over records; function-valued fields |
| ordinary function-body reassignment; augmented, field/index assignment | rejected | a new immutable binding, `update`, folds; handler-owned state where permitted |
| early `return` | rejected | conditional expression or a final `if`/`match` |
| `if xs:` (truthiness) | type error | explicit `bool` test such as `not isEmpty(xs)` |
| `a < b < c`, `is`, `is not` | rejected | `a < b and b < c`, `==` |
| tuples, `a, b = pair` | rejected | records |
| slices `xs[1:3]` | rejected | `take`, `drop`, `substring` |
| `xs[-1]` | returns `Error` | `listGet(xs, len(xs) - 1)` |
| default, `*args`, `**kwargs` parameters | rejected | explicit parameters, records |
| `f"{x:.2f}"`, `{x!r}`, `{x=}` | rejected | plain `{x}` |
| arbitrary decorators (`@staticmethod`, `@dataclass`, …) | rejected | `@union`, effect decorators, `@extern`, `@export`, `@requires` |
| `if __name__ == "__main__":` | rejected | `def main():` |
| `None` as a nullable value | `None` is `Unit`, never null | `Result` |

Supported `for` loops, comprehensions, unions and renamed pattern bindings are documented above. The [Python flavor specification](/spec/0041-pythonflavorsyntax/) and [delivery plan](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0031-python-flavor.md) record remaining limits; rejection of an explicitly unsupported construct is expected.

## Diagnostics you will see

| Diagnostic fragment | Cause | Fix |
| --- | --- | --- |
| `redundant type annotation on ...` | an annotation inference already derives | delete the annotation |
| `unhandled effect operations at program entry: Arith.overflow` | integer arithmetic with no `Arith` handler | wrap the work in `with wrapping:` or another policy |
| ``unhandled effect operations at program entry: E.op; add a matching `handle` `` | an operation performed with no handler installed | install a handler for `E` around the call |
| `unhandled effect operations at program entry: Arith.divideByZero` or `Arith.remainderByZero` | `/`, `//` or `%` with a run-time divisor and no matching arm | add the named arm (see the `total` policy) |
| ``Python `…` has no supported Osprey lowering`` | a Python construct outside the supported surface | use the replacement in the table above |
| `state mutation is only allowed inside an effect handler arm` | assignment outside a permitted handler arm | bind a new name or use a fold |
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
- [ ] Every fallible arithmetic site, including float zero divisors, has the required policy; selected-device recovery limits are respected.
- [ ] Every failure you hit has a GitHub issue of type `Bug`, and its URL is in your report.

## Reference

- [Python flavor guide](/docs/python-flavor/), the human-oriented introduction.
- [Python flavor specification](/spec/0041-pythonflavorsyntax/), the full contract.
- [Algebraic effects guide](/docs/effects/) and [effects specification](/spec/0017-algebraiceffects/).
- [GPT-2 in the Python flavor](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python), the largest working program.
