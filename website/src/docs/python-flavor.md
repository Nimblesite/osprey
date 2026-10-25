---
mlTwins: manual
layout: page
title: Python flavor
description: The Python flavor is Osprey source that looks like typed Python. The Osprey compiler lowers it into the same AST as every other flavor and compiles it to a native binary; CPython never runs it. GPT-2 is the worked example.
permalink: /docs/python-flavor/
tags: [flavors, python, algebraic-effects, gpu]
---

Osprey's flavors share type inference, effect checking, modules, memory management and native code generation. Each parses its own syntax into the same canonical abstract syntax tree (AST).

The Python flavor is Osprey that looks like typed Python. You write `def`, `lambda`, `match`, f-strings and PEP 484/695 annotations, and declare effects and handlers with decorated classes. But the program is Osprey: the `osprey` compiler parses it, checks it with Osprey's types and effects, and produces a native binary.

[GPT-2 in the Python flavor](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python) loads published Hugging Face weights and uses effect handlers to choose CPU or GPU execution. Its output matches the Default-flavor generator byte for byte on tested backends.

## Python flavor is not Python

Python-flavor files and Python files both end in `.py`, but they are different languages run by different programs:

| | Python flavor | Python |
| --- | --- | --- |
| Language | Osprey | Python |
| First line | `# osprey: flavor=python` | anything |
| Run by | `osprey`, which compiles a native binary | CPython, which interprets it |
| Types | Inferred and checked before the program runs | Checked at run time |
| Values | Immutable records, `Result` for failure | Mutable objects, exceptions |
| Imports | Osprey modules, in any flavor | Python packages |
| GPT-2 in this repository | [`examples/gpt2/python/`](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python), the model | [`examples/gpt2/reference.py`](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/reference.py), the reference it is checked against |

Every Python-flavor sample here starts with `# osprey: flavor=python` and runs through Osprey. To call CPython libraries, use [Python interop](/spec/0042-pythoninterop/).

## Select the flavor

A file is Python flavor when it starts with the marker comment `# osprey: flavor=python`, when it has a `.py` extension and is passed directly to the compiler, or when the command line says `--flavor python`:

```bash
osprey hello.py --run
```

Projects opt in file by file. List each Python-flavor file under `python_sources` in `osprey.toml`; other `.py` files below a source root, such as build scripts, are ignored.

## Functions and inference

```python
# osprey: flavor=python

@handler(Arith)
class wrapping:
    def overflow(_op, _left, _right, wrapped):
        return wrapped


def scale(value, factor):
    return value * factor


def main():
    twice = lambda value: scale(value, 2)
    with wrapping:
        print(f"scale: {scale(value=6, factor=7)}, twice: {twice(21)}, sum of squares: {fold(map(range(1, 6), lambda n: n * n), 0, lambda acc, n: acc + n)}")
```

```text
scale: 42, twice: 42, sum of squares: 55
```

None of these functions has an annotation, but every type is known. Osprey's Hindley–Milner inference works out each function's type from the body. Annotations are optional, and you only add one when inference cannot decide. A call passes its arguments either all by position or all by keyword.

Integer arithmetic can overflow, so `+`, `-` and `*` on integers perform the built-in `Arith` effect. The `wrapping` handler gives it a policy: return the wrapped result. If no handler covers the arithmetic, the compiler rejects the program; it does not pick a policy for you. [Arithmetic needs a policy](/docs/effects/#arithmetic-needs-a-policy) shows the alternatives.

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

A plain `class` declares a closed, immutable record. `Account(owner=..., balance=...)` builds one, and `update(account, balance=...)` returns a copy with one field changed. A failure that callers should expect is returned as a `Result` value; there are no exceptions. `match` must cover every case. `return` may only appear at the end of a function or at the end of every branch of a final `if` or `match`, so each function remains a single expression.

## Effects and handlers

An effect lists operations a program can ask for without choosing how they are carried out. A handler answers them, and the caller decides which handler is installed.

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

- `@effect` turns a class into an effect declaration. Each operation is a typed signature whose body is `...`.
- `perform(Log.write(...))` asks for the operation. `load` does not know who answers.
- `@handler(Log)` turns a class into a handler function. Its methods are the operation arms, and they take no `self`.
- `with console:` runs the block under `console`. `audit(work)` is the same thing written as a call: it runs the zero-argument function `work` under the handler.
- The compiler rejects a program that performs an operation no installed handler answers, even when the operation is performed inside a helper, a lambda or a fiber.

An operation marked `@control` can resume the code that performed it, using `resume(value)`, or end the handled block with its own answer. `@requires(Log, Arith)` on a function states the effects it may perform. The [algebraic effects guide](/docs/effects/) explains these semantics; they are the same in every flavor.

## CPU or GPU is a handler

GPT-2 is written against a static effect called `Tensor`. Static effects are resolved at compile time, so the program does no handler lookup at run time. `B` is the buffer type, and the handler picks it:

```python
# osprey: flavor=python
# examples/gpt2/python/backends.py, abridged
@static_effect
class Tensor[B]:
    def build(size: int, kernel: Kernel) -> B: ...
    def get(buffer: B, index: int) -> float: ...
    def mat_mul(a: B, b: B, rows: int, inner: int, cols: int) -> B: ...
    def load(path: str, name: str) -> B: ...
    def device() -> str: ...
```

The model performs the operations and never names a buffer type:

```python
# osprey: flavor=python
# examples/gpt2/python/model.py, abridged
def product(a, b, rows, inner, cols):
    return perform(Tensor.mat_mul(a, b, rows, inner, cols))
```

Two handlers, each in its own file, answer every operation. [`cpu.py`](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/python/cpu.py) uses host lists:

```python
# osprey: flavor=python
# examples/gpt2/python/cpu.py, abridged
@static_handler(Tensor[list[float]])
class on_lists:
    def mat_mul(a, b, rows, inner, cols):
        return toList(map(range(rows * cols), lambda i: dot(a, b, i, inner, cols)))

    def device():
        return "lists"
```

[`gpu.py`](https://github.com/Nimblesite/osprey/blob/main/examples/gpt2/python/gpu.py) uses GPU buffers, run on Metal or OpenCL:

```python
# osprey: flavor=python
# examples/gpt2/python/gpu.py, abridged
@static_handler(Tensor[GpuBuffer[float]])
class on_gpu:
    def mat_mul(a, b, rows, inner, cols):
        return or_abort(gpuMatMul(a, b, rows, inner, cols))

    def device():
        return gpuDevice()
```

The model source is the same for both. The handler the program installs decides where the arithmetic runs, and the compiler checks that both handlers answer every operation with the right types. [One model, two backends](/docs/one-model-two-backends/) walks through the full effect and both handlers.

## Modules and mixed-flavor projects

Every flavor lowers into the same AST, so one project can mix them. Here a Default-flavor entry point imports a module written in the Python flavor:

```toml
# osprey.toml
[project]
name = "mixed"
source_roots = ["src"]
python_sources = ["src/pricing.py"]
default_namespace = "shop"
entry = "src/main.osp"
```

```python
# osprey: flavor=python
# osprey: namespace=shop
# osprey: module=Pricing
"""Prices, written in the Python flavor."""

__all__ = ["discounted"]


def discounted(price, percent):
    return price - price * percent / 100.0
```

```osprey
namespace shop;

import shop::Pricing::{discounted}

fn main() = print("Discounted: ${discounted(price: 80.0, percent: 25.0)}")
```

```text
Discounted: 60.0
```

The `# osprey: namespace=` and `# osprey: module=` comments place the file in a module. `__all__` lists the names it exports; anything not listed stays private to the module. `from gpt2.Backends import Tensor` and `from gpt2 import Cpu, Gpu` import Osprey modules, whatever flavor they are written in. They cannot import Python packages.

## What the Python flavor leaves out

The Python flavor borrows Python's look, not its runtime. Code that depends on Python's runtime behavior is rejected; it is never passed to an interpreter.

- `import numpy` and other Python packages are unavailable, because imports name Osprey modules.
- Integers are 64-bit with an `Arith` policy, and floats are binary64. There is no arbitrary precision. Only booleans can be used as conditions, and `and`/`or` return booleans.
- `while`, `break`, `continue`, `try`/`except`, `raise`, `async`/`await` and `yield` are rejected. Use `fold`, recursion, `Result` and fibers instead.
- Classes are records, effects or handlers. There is no inheritance, `__init__`, `self` or method dispatch.

`for` loops and list comprehensions are implemented over the existing iterator operations; `for` takes an iterator, not an arbitrary Python iterable. `@union` declares sum types, and keyword patterns may bind a different name (`case Circle(radius=r)`). These forms share Osprey's type and effect checks. Frontend and tooling coverage remain incomplete: VS Code still opens `.py` files as ordinary Python. [Plan 0031](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0031-python-flavor.md) tracks the remaining requirements.

## Run the Python-flavor GPT-2

From the repository root, after building Osprey:

```bash
make gpt2 FLAVOR=python GPU=metal PROMPT="The capital of France is" TOKENS=20
make gpt2 FLAVOR=python BACKEND=lists PROMPT="The capital of France is" TOKENS=1
```

The first command installs `on_gpu` and the second `on_lists`; the model is unchanged. The test `python_gpt2_generator_output_is_byte_identical_to_the_default_generator` checks that the Python generator prints exactly what the Default generator prints on host lists, Metal and OpenCL. Only generation has been ported so far; the training step is still Default-only.

- [GPT-2 in the Python flavor](https://github.com/Nimblesite/osprey/tree/main/examples/gpt2/python): the complete source.
- [Python flavor specification](/spec/0041-pythonflavorsyntax/): the full contract.
- [Language flavors](/spec/0023-languageflavors/): how every flavor lowers into the shared AST.
