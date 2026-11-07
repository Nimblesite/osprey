---
mlTwins: manual
layout: page
title: Python flavor
description: The Python flavor is Osprey source that looks like typed Python. The Osprey compiler lowers it into the same AST as every other flavor and compiles it to a native binary; CPython never runs it. Ports of NumPy, PyTorch, Transformers and more are already written in it, and a Qwen3.8-27B model is being built on them.
permalink: /docs/python/flavor/
tags: [flavors, python, algebraic-effects, gpu]
---

{% from "native-capture.njk" import capture %}

Osprey's flavors share type inference, effect checking, modules, memory management and native code generation. Each parses its own syntax into the same canonical abstract syntax tree (AST).

The Python flavor is Osprey that looks like typed Python. You write `def`, `lambda`, `match`, f-strings and PEP 484/695 annotations, and declare effects and handlers with decorated classes. But the program is Osprey: the `osprey` compiler parses it, checks it with Osprey's types and effects, and produces a native binary.

Much of Python's ML and data stack is [already ported](#python-packages-already-ported) to it: NumPy, SciPy, PyTorch, scikit-learn, Transformers, tokenizers and more. The [local LLM work](#local-llms-in-python-flavor), a Qwen3.8-27B model and an Ollama-compatible server, is written in it too.

## How Python flavor differs from Python

Python-flavor files and Python files both end in `.py`, but they are different languages run by different programs:

| | Python flavor | Python |
| --- | --- | --- |
| Language | Osprey | Python |
| First line | `# osprey: flavor=python` | anything |
| Run by | `osprey`, which compiles a native binary | CPython, which interprets it |
| Types | Inferred and checked before the program runs | Checked at run time |
| Values | Immutable records, `Result` for failure | Mutable objects, exceptions |
| Imports | Osprey modules, including Osprey's ports of NumPy, PyTorch, Transformers and more | Python packages from PyPI |
| GPT-2 in this repository | [`src/osprey/examples/llms/gpt2/python/`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/gpt2/python), the model | [`src/osprey/examples/llms/gpt2/reference.py`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/llms/gpt2/reference.py), the reference it is checked against |

Every Python-flavor sample here starts with `# osprey: flavor=python` and runs through Osprey. To call CPython libraries, use [Python interop](/spec/0042-pythoninterop/).

### Strong, static types: the biggest difference

Python checks types while the program runs. Python flavor checks every type before the program runs. You don't write type hints: Osprey infers them from how each value is used, and every value must have exactly one type. When it can't work one out, the program does not compile, even if it is valid Python.

```python
# osprey: flavor=python

row = [42, "Ada", 3.5]

print(f"{row[1]} scored {row[2]}")
```

CPython runs this without complaint:

```text
$ python3 row.py
Ada scored 3.5
```

Osprey rejects it. A list holds one element type, and nothing says whether this one is a list of `int`, `str` or `float`:

{{ capture("/assets/images/guides/python-flavor-static-types.png", "The Playground compiling row = [42, \"Ada\", 3.5] in Python flavor: compilation fails with 3:6: type mismatch: cannot unify int with string, and 3:6: type mismatch: cannot unify int with float.", "Valid Python, rejected before it runs: Osprey cannot give the list a single element type.", 1440, 520) }}

In Python, the same mix of types often hides a bug that only shows up on a code path that rarely runs, such as `row[0] + 1` getting the name instead of the id. In Osprey, give each field its own type with a record:

```python
# osprey: flavor=python

class Row:
    id: int
    name: str
    score: float


row = Row(id=42, name="Ada", score=3.5)

print(f"{row.name} scored {row.score}")
```

That prints `Ada scored 3.5`, and every use of `row.id` is now known to be an `int`. A class with annotated fields is an Osprey record; CPython would need `@dataclass` to run it. You add annotations only where nothing pins a type down, such as an empty list that is never used (`no_scores: list[int] = []`).

### Python packages: already ported

`import` names Osprey modules, and the repository already ports a lot of the Python ML and data stack to Python flavor. The ports keep their originals' module, function and argument names, and their test suites compare results against the real library running under CPython:

- **Arrays and maths:** [`@osprey/numpy`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/numpy), [`@osprey/scipy`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/scipy), [`@osprey/einops`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/einops)
- **Machine learning:** [`@osprey/torch`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/torch), [`@osprey/sklearn`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/sklearn), [`@osprey/micrograd`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/micrograd), [`@osprey/flash_attn`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/flash_attn), [`@osprey/faiss`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/faiss)
- **Language models:** [`@osprey/transformers`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/transformers), [`@osprey/tokenizers`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/tokenizers), [`@osprey/tiktoken`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/tiktoken), [`@osprey/safetensors`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/safetensors), [`@osprey/gguf`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/gguf), [`@osprey/sacrebleu`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/sacrebleu), [`@osprey/rouge_score`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/rouge_score)
- **Apps and agents:** [`@osprey/fastapi`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/fastapi), [`@osprey/pydantic_ai`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/pydantic_ai), [`@osprey/jinja2`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/jinja2), [`@osprey/re`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/re)

Imports look the way they do in Python. This is the top of the [vector-search example](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/python/ports/vector-search/main.py), FAISS's flat-index tutorial ported to Osprey:

```python
from numpy import Array, Errors, Host
from faiss import Flat, Normalize
from scipy import Information
```

Each package's README lists what its port covers; for example, `@osprey/numpy` covers float64 arrays only. To call a package that hasn't been ported, call CPython through [Python interop](/docs/python/interop/).

### Language features

The Python flavor borrows Python's syntax, not its runtime. Code that depends on Python's runtime behavior is rejected; it is never passed to an interpreter.

- Integers are 64-bit with an `Arith` policy, and floats are binary64. There is no arbitrary precision. Only booleans can be used as conditions, and `and`/`or` return booleans.
- `while`, `break`, `continue`, `try`/`except`, `raise`, `async`/`await` and `yield` are rejected. Use `fold`, recursion, `Result` and fibers instead.
- Classes are records, effects or handlers. There is no inheritance, `__init__`, `self` or method dispatch.

`for` loops and list comprehensions are implemented over the existing iterator operations; `for` takes an iterator, not an arbitrary Python iterable. `@union` declares sum types, and keyword patterns may bind a different name (`case Circle(radius=r)`). Frontend and tooling coverage is still incomplete: VS Code still opens `.py` files as ordinary Python. [Plan 0031](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0031-python.md) tracks the remaining requirements.

To run real CPython code, or call Osprey from a Python application, see [Python interop](/docs/python/interop/).

## Converting between flavors (coming)

A flavor converter is planned ([plan 0057](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0057-flavor-conversion.md)). It will turn a file or a whole package from Python flavor into Default or ML, or the other way, from VS Code or the command line. Every flavor parses to the same AST, so the converter prints that AST in the target flavor and writes nothing unless the result parses back to the same program. Until it ships, moving a file between flavors is a manual rewrite.

## Select the flavor

A file is Python flavor when it starts with the marker comment `# osprey: flavor=python`, when it has a `.py` extension and is passed directly to the compiler, or when the command line says `--flavor python`:

```bash
osprey hello.py --run
```

Projects opt in file by file. List each Python-flavor file under `python_sources` in `osprey.toml`; other `.py` files below a source root, such as build scripts, are ignored.

{{ capture("/assets/images/guides/python-functions.png", "Run inferred functions and a fold: 42, 42 and 55 appear in the output.", "Run inferred functions and a fold: 42, 42 and 55 appear in the output.", 1440, 920) }}

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

Types are inferred from function bodies; annotate only where inference needs help. Pass arguments all positionally or all by keyword.

Integer arithmetic can overflow, so `+`, `-` and `*` on integers perform the built-in `Arith` effect. The `wrapping` handler gives it a policy: return the wrapped result. If no handler covers the arithmetic, the compiler rejects the program; it does not pick a policy for you. [Arithmetic needs a policy](/docs/effects/#arithmetic-needs-a-policy) shows the alternatives.

{{ capture("/assets/images/guides/python-records.png", "The same account accepts one withdrawal and returns a visible declined Result for another.", "The same account accepts one withdrawal and returns a visible declined Result for another.", 1440, 920) }}

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

A plain `class` declares a closed immutable record; keyword construction builds it and `update` returns a modified copy. Expected failures are Result values, with exhaustive matching and no exceptions. `return` belongs at a function's tail or in every branch of its final `if`/`match`.

{{ capture("/assets/images/guides/python-effects.png", "Change the handler: the output switches from log to audit while load remains unchanged.", "Change the handler: the output switches from log to audit while load remains unchanged.", 1440, 920) }}

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
# Selected operations from the current implementation.
{{ gpuExample.tensor | safe }}
```

The model performs the operations and never names a buffer type:

```python
# osprey: flavor=python
# src/osprey/examples/llms/gpt2/python/model.py, abridged
def product(a, b, rows, inner, cols):
    return perform(Tensor.mat_mul(a, b, rows, inner, cols))
```

Two handlers, each in its own file, answer every operation. [`cpu.py`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/llms/gpt2/python/cpu.py) uses host lists:

```python
# osprey: flavor=python
# Selected operations from the current implementation.
{{ gpuExample.cpu | safe }}
```

[`gpu.py`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/llms/gpt2/python/gpu.py) uses GPU buffers, run on Metal or OpenCL:

```python
# osprey: flavor=python
# Selected operations from the current implementation.
{{ gpuExample.gpu | safe }}
```

The installed handler selects the backend. Both must answer every operation with compatible types; products and loads return Result so callers handle failure. [Run the same model on CPU or GPU](/docs/cpu-or-gpu/) walks through the full effect and both handlers.

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

The `# osprey: namespace=` and `# osprey: module=` comments place the file in a module. `__all__` lists the names it exports; anything not listed stays private to the module. `from gpt2.Backends import Tensor` and `from gpt2 import Cpu, Gpu` import Osprey modules, whatever flavor they are written in, including the ported packages above.

## Local LLMs in Python flavor

Python flavor is what Osprey's language-model work is written in. The goal is a local model server you can use instead of Ollama, written in Osprey and running on Osprey's own GPU kernels. [Plan 0049](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0049-local-coding-model.md) tracks it.

- **The generation layer:** [`@osprey/transformers`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/transformers) ports Hugging Face's model-independent generation code (logits processors and warpers, sampling, stopping criteria and `generate`) and is checked against Transformers and PyTorch.
- **The Qwen3.8-27B model:** [`qwen/model`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/qwen/model) translates Transformers' `qwen3_5` model (attention, DeltaNet, RoPE, the cache, the tool-call parser) to Osprey GPU code. So far it has only run a tiny 4-layer fixture, compared layer by layer against PyTorch. It does not run the real weights yet.
- **Real weights:** [`qwen/run`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/qwen/run) runs the real Qwen3.8-27B `UD-Q4_K_XL` GGUF on Metal through the package model — `Qwen3_5Gguf` maps the file, the tokenizer comes from the file's own tables — and continues "The capital of France is" with " Paris.\nThe capital of Germany is Berlin.\n…". One run measured 10.0 tokens/s, against 8.0 for Ollama on the same file. Both runs were on a loaded machine, so treat the figures as preliminary; plan 0049 will re-measure them on a quiet one.
- **The Ollama-compatible server:** [`qwen/server`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/qwen/server) speaks Ollama's API (`/api/chat`, `/api/generate`, `/api/tags` and more) and OpenAI's (`/v1/chat/completions`, `/v1/models`), with streaming and tool calls. Tools that talk to Ollama can point at it. Today it answers from a small built-in table model; it will serve real Qwen output once the package model runs the real weights.

GPT-2 came first and is still a working example: the [Python-flavor GPT-2](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/llms/gpt2/python) loads published Hugging Face weights, and its output matches the Default-flavor generator byte for byte on host lists, Metal and OpenCL. From the repository root, after building Osprey:

```bash
make -C llms gpt2 FLAVOR=python GPU=metal PROMPT="The capital of France is" TOKENS=20
```

- [Python flavor specification](/spec/0041-pythonflavorsyntax/): the full contract.
- [Language flavors](/spec/0023-languageflavors/): how every flavor lowers into the shared AST.
