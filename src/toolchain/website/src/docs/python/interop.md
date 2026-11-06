---
layout: page
title: "Python interop: call Python from Osprey, and Osprey from Python"
description: "Python interopmaxxing in Osprey: call NumPy, PyTorch or any Python library from a compiled, statically typed language, call fast native Osprey code from Python, and reuse ordinary Python code as Osprey."
permalink: /docs/python/interop/
tags: [python, interop, python-interop, python-ffi, embed-python, numpy, pytorch, data-science, machine-learning, algebraic-effects]
---

Osprey goes all in on Python. Call it **Python interopmaxxing**: you keep the Python libraries and habits you already have, and add a compiled, statically typed language with algebraic effects, which makes it easy to test. If your work runs on NumPy, pandas, PyTorch, scikit-learn or SciPy, you shouldn't have to give any of them up to try Osprey.

There are three ways in:

1. **Write Python-style code that is Osprey.** Much ordinary Python already compiles as Osprey source in the [Python flavor](/docs/python/flavor/). Types are inferred and checked before the program runs.
2. **Call Python from Osprey.** A `Python` effect calls into a running CPython, so Osprey code can use any Python library: models, data loaders, tokenizers, plotting.
3. **Call Osprey from Python.** Compile Osprey to a native library and `import osprey` from Python, the way you would use a C extension or bindings, but without writing C.

## A lot of Python is already Osprey

Add `# osprey: flavor=python` to the top of the file, and simple typed-style Python compiles unchanged with `osprey`. This file prints the same three lines under both `python3` and `osprey --run`:

```python
# osprey: flavor=python

def label(score):
    match score:
        case 0:
            return "none"
        case _:
            return "high" if score > 50 else "low"


def report(name, score):
    return f"{name}: {score} is {label(score)}"


print(f"{report("alice", 0)}\n{report("bob", 12)}\n{report("carol", 80)}")
```

```text
alice: 0 is none
bob: 12 is low
carol: 80 is high
```

Osprey infers the types, checks them before the program runs, and compiles a native binary. It isn't CPython, though. Mutation of fields, `for` loops over lists, exceptions and arbitrary Python imports are rejected, and failures are `Result` values. The [Python flavor guide](/docs/python/flavor/) covers what is supported and what is left out.

A [flavor converter](/docs/python/flavor/#converting-between-flavors-coming) is planned to move Python-flavor files to Default or ML Osprey, and back.

## Call Python from Osprey (embed a Python library)

Python is a capability, so Osprey models it as an effect. Code performs `Python.call(module, function, jsonArgs)`. The `live` handler sends the call to CPython, and a test handler answers with no Python at all:

```osprey
{{ pythonInterop.osprey | safe }}
```

`squareRoot` uses Python's `math.sqrt` but never says how Python is reached. `squareRootText` runs it under `live`, and `mockedSquareRoot` runs the same function under `fake`, which is how you test Python-dependent code without Python. Python exceptions come back as `Error` values for Osprey to handle. See [algebraic effects](/docs/effects/#test-doubles-are-just-handlers) for more on test doubles.

This program runs inside a Python process, so it is compiled for the Python target and started from Python, as shown in the next section.

## Call Osprey from Python (native speed, no C extension)

Compile the Osprey file into a native library for Python (on Linux use `.so` instead of `.dylib`):

```sh
python3 -m pip install -e src/toolchain/python
osprey src/osprey/examples/python/interop.osp --compile --target=python --memory=arc -o interop.dylib
```

Then load it in an ordinary Python program and call its exported functions:

```python
import osprey

library = osprey.load("./interop.dylib")
print(library.squareRootText(81.0))
print(library.mockedSquareRoot())
```

```text
9.0
test answer
```

`osprey.load` installs the Python callbacks, so calls flow both ways: Python calls `squareRootText`, which performs `Python.call`, which calls `math.sqrt` back in the same process. `osprey.build` compiles and loads in one step, and `python3 -m osprey` runs a file directly.

The Python-flavor version uses typed `@extern` signatures instead of an effect:

```python
{{ pythonInterop.flavor | safe }}
```

## Real use: an ML data pipeline

[LLM dataset audit](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/python/osprey/dataset_audit/README.md) is an installed Python tool. Python loads datasets and tokenizers, and native Osprey decides the policy for every record. It has run on all 15,011 Dolly records with the GPT-2 tokenizer.

## Limits

- Linux and macOS only. Exported functions take and return scalars, strings and JSON text.
- One loading thread, the GIL is held, and the library uses ARC memory management.
- Python objects such as tensors stay in Python. Pass them as opaque references with `library.keep(obj)`, `resolve(id)` and `release(id)`, and release them explicitly.
- These calls cross Osprey's C memory-safety boundary.

The [Python interop specification](/spec/0042-pythoninterop/) defines the exact protocol. Run the integration tests with `cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli --test python_interop`.
