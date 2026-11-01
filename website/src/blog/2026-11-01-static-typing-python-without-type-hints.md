---
layout: page.njk
title: "Static Typing for Python-Style Code, Without Writing Type Hints"
excerpt: "Static typing catches the wrong value before the program runs; dynamic typing finds out when that line executes. Osprey's Python flavor keeps def, lambda and f-strings, and infers every type and checks it at compile time. Here it is on a real 1,262-line port of a Chromecast sender."
description: "What static typing is, how it differs from dynamic typing, and how Osprey's Python flavor checks Python-style code at compile time with inferred types instead of hints, shown on a Chromecast sender ported from a CPython script."
date: 2026-11-01
tags: ["blog", "python-flavor", "static-typing", "type-inference", "chromecast"]
author: "Christian Findlay"
readingTime: 7
image: /assets/images/blog/static-typing-python.png
imageWebp: /assets/images/blog/static-typing-python.webp
imageAlt: "Blue and gold ribbons passing through a prism into interlocking geometric blocks."
---

**Static typing** means every value's type is checked before the program runs. **Dynamic typing** means types are checked as each line executes. Python is dynamic: pass a number where a function expects text and you find out when that line runs, perhaps in production. Type hints with a checker such as mypy, pyright or ty add a static check on top, but only as far as you write the hints, and the interpreter ignores them at run time.

[Osprey](/) is a functional language built for safety, elegance and performance. Its **Python flavor** is a way to write Osprey with Python's syntax: `def`, `lambda`, f-strings, conditional expressions and `match`. The compiler infers every type and refuses to build a program whose types do not fit. You get static typing without writing the types.

{% videoPlayer "static-typing-python", true %}

## The same mistake, caught before it runs

```python
# osprey: flavor=python

def label(name, host):
    return f"{name} ({host})" if endsWith(host, ".local") else name


def main():
    print(label("Kitchen", "kitchen.local"))
```

`label` has no annotations. The compiler works out that `host` is a string, because `endsWith` takes one, and that `label` returns a string. This runs and prints `Kitchen (kitchen.local)`. Now pass the port number by mistake:

```python
    print(label("Kitchen", 8009))
```

```text
bad.py:8:4: type mismatch: cannot unify string with int
```

No binary is built. In CPython, the equivalent `host.endswith(".local")` would raise `AttributeError` when that line ran, if it ever ran. Inference also reaches places hints often stop: lambdas, records, list contents and the values inside `Result`.

## Static vs dynamic typing, side by side

| | Dynamic (CPython) | Hints + checker | Osprey Python flavor |
| --- | --- | --- | --- |
| When types are checked | As each line runs | Before running, where hints exist | Before building, everywhere |
| Annotations you write | None | Every boundary you want checked | Only where inference cannot decide, such as an empty list with no other use |
| Can the run ignore the check? | — | Yes, hints are not enforced | No, an ill-typed program is not built |
| Failures | Exceptions | Exceptions | `Result` values the caller must handle |
| Output | Interpreted | Interpreted | A native binary through LLVM |

The last two rows matter as much as the first. Osprey has no exceptions and no panics. A function that can fail returns `Result`, and the caller must `match` it, so a missing case is a compile error too.

## A real program: a Chromecast sender

To test this on something bigger than a sample, I ported a stdlib-only CPython Chromecast sender, `cast.py`, to Python flavor. It became [`@osprey/chromecast`](https://github.com/Nimblesite/osprey/tree/main/packages/chromecast): 1,262 lines covering network discovery, protobuf framing, a TLS session with heartbeats, media loading and playback controls. It powers the [Mac casting player](/blog/2026-11-01-chromecast-from-mac-to-tv/).

Here is how it reads a device's friendly name out of a discovery record:

```python
def friendly_name(line):
    """The `fn=` value of a TXT line, with `\\ ` read as a space."""
    after = unwrap_or(listGet(unwrap_or(split(line, "\"fn="), []), 1), "")
    raw = unwrap_or(listGet(unwrap_or(split(after, "\""), []), 0), "")
    return unwrap_or(replace(raw, "\\ ", " "), raw)
```

No hints, and every type is known. `split` can fail, so it returns a `Result`; `listGet` can run past the end, so it returns one too; `unwrap_or` says what to use instead. Where `cast.py` would raise `IndexError` on a malformed record, the port has already decided what happens.

Finding a device by name shows the same idea at the API boundary:

```python
def named(devices, name) -> Result[Device, str]:
    """The device with this friendly name."""
    match listGet(filterList(devices, lambda d: d.name == name), 0):
        case Success(value=device):
            return Success(value=device)
        case Error():
            return Error(message=f"no Cast device named {name}")
```

The reference did `devs[name]` and raised `KeyError` for a missing TV. Here the missing TV is part of the type, so every caller must handle it. 71 of the port's 134 functions carry no annotation at all. The exported ones name their return types, which makes the package's API readable at a glance.

## Checked against the original

A port is only useful if it does what the original did. The original's pure functions run under CPython in an oracle script, which writes the expected frames, parses and loop decisions to a file. The Osprey suite checks the port against every one, then drives the whole sender over real TLS against a scripted device:

```text
ok varint.encode.0
ok varint.encode.1
ok varint.encode.2
...
ok fail.no_route
ok fail.no_file
252 passed, 0 failed
```

All of the sender's I/O goes through one algebraic effect, `Wire`, so the tests swap the network for a fake TV with a handler. Types catch the wrong value; effects make the outside world replaceable.

## What it is not

Python flavor is Osprey, not CPython. An arbitrary Python program or package does not run unchanged: classes are records with no methods or inheritance, `try`/`except`, `while` and `yield` are rejected, and imports name Osprey modules, not Python packages. Porting `cast.py` meant rewriting it in that style. To call real CPython libraries, Osprey has [Python interop](/spec/0042-pythoninterop/). The [Python flavor guide](/docs/python-flavor/) lists what the flavor leaves out.

## Try it

[Install Osprey](/docs/installation/), save the sample above as `label.py`, and run `osprey label.py --run`. Then change the argument to a number and run it again. The [Python flavor guide](/docs/python-flavor/) covers records, `Result`, `match` and effects.

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
