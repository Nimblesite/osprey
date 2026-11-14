---
layout: page
title: "Python interop: call Python from Osprey, and Osprey from Python"
description: "Python interopmaxxing in Osprey: call NumPy, PyTorch or any Python library from a compiled, statically typed language, call fast native Osprey code from Python, and reuse ordinary Python code as Osprey."
permalink: /docs/python/interop/
tags: [python, interop, python-interop, python-ffi, embed-python, numpy, pytorch, data-science, machine-learning, algebraic-effects]
---
{% from "native-capture.njk" import capture %}

Osprey goes all in on Python. Call it **Python interopmaxxing**: keep your Python libraries and habits, and add a compiled, statically typed language whose algebraic effects make it easy to test. If your work runs on NumPy, pandas, PyTorch, scikit-learn or SciPy, you keep them.

There are three ways in:

1. **Write Python that is Osprey.** Much ordinary Python compiles as the [Python flavor](/docs/python/flavor/), and [NumPy, SciPy, PyTorch, Transformers and more are already ported](/docs/python/flavor/#python-packages-already-ported) to it.
2. **Call Python from Osprey.** A `Python` effect calls into CPython, so Osprey can use any Python library.
3. **Call Osprey from Python.** Compile Osprey to a native library and `import osprey`, without writing C.

## A lot of Python is already Osprey

With `# osprey: flavor=python` at the top, this file prints the same three lines under `python3` and `osprey --run`:

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

Osprey checks the types before it runs, then compiles natively. It isn't CPython: field mutation and exceptions are rejected, failures are `Result` values, and imports name Osprey modules. See [how Python flavor differs from Python](/docs/python/flavor/#how-python-flavor-differs-from-python). A [flavor converter](/docs/python/flavor/#converting-between-flavors-coming) is planned.

{{ capture("/assets/images/guides/python-hello.png", "Python-flavor source compiled and executed by Osprey · a greeting appears in the output.", "Python-flavor source compiled and executed by Osprey · a greeting appears in the output.", 1440, 920) }}
{{ capture("/assets/images/guides/python-flavor-static-types.png", "The Playground compiling row = [42, \"Ada\", 3.5] in Python flavor: compilation fails with 3:6: type mismatch: cannot unify int with string, and 3:6: type mismatch: cannot unify int with float.", "Valid Python, rejected before it runs: Osprey cannot give the list a single element type.", 1440, 520) }}

## Call Python from Osprey (embed a Python library)

Python is a capability, so it is an effect. Code performs `Python.call(module, function, jsonArgs)`; the `live` handler sends it to CPython, and a test handler answers without Python:

```osprey
{{ pythonInterop.osprey | safe }}
```

`squareRootText` runs `squareRoot` under `live`; `mockedSquareRoot` runs it under `fake`, a [test double](/docs/effects/#test-doubles-are-just-handlers). Python exceptions come back as `Error` values. The program runs inside a Python process, started as shown next.

## Call Osprey from Python (native speed, no C extension)

Compile Osprey into a native library (`.so` on Linux):

```sh
python3 -m pip install -e src/toolchain/python
osprey src/osprey/examples/python/interop.osp --compile --target=python --memory=arc -o interop.dylib
```

Then load it from Python:

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

Calls flow both ways: `squareRootText` calls `math.sqrt` back in the same process. `osprey.build` compiles and loads in one step; `python3 -m osprey` runs a file.

The Python-flavor version uses typed `@extern` signatures:

```python
{{ pythonInterop.flavor | safe }}
```

## Real use: an ML data pipeline

In [LLM dataset audit](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/python/osprey/dataset_audit/README.md), Python loads datasets and tokenizers and native Osprey decides the policy for every record. It has run on all 15,011 Dolly records with the GPT-2 tokenizer.

## Speed: in Codon's range

Codon also compiles Python-like code natively. On the [benchmarks](/benchmarks/), Osprey with ARC takes 0.84× Codon's time (geometric mean) and is faster on 19 of 24 programs. Of the rest, five are up to 3× slower. Codon ran on a busy machine, so this is approximate. Performance improves all the time.

## Limits

- Linux and macOS today; Windows coming soon. Exported functions take and return scalars, strings and JSON text.
- One loading thread, the GIL is held, and the library uses ARC.
- Objects from CPython libraries stay in Python, as handles from `library.keep(obj)` that you `release(id)`. The [ported packages](/docs/python/flavor/#python-packages-already-ported), including `@osprey/torch` and `@osprey/transformers`, are pure Osprey and never cross this bridge.
- These calls cross Osprey's C memory-safety boundary.

The [Python interop specification](/spec/0042-pythoninterop/) defines the exact protocol. Run the integration tests with `cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli --test python_interop`.
