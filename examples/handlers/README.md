# Play with handlers

Run from the repository root:

```sh
python3 examples/handlers/run.py
```

This runs actual Osprey, Koka, OCaml, Eff, and Effekt programs. Both Osprey flavors are included. Their tools are installed locally under `target/demo-tools` (OCaml is already on PATH). Build the prototype compiler after editing it with `cargo build --release -p osprey-cli`.

Edit a language's `handlers` file and rerun just that language:

```sh
python3 examples/handlers/run.py osprey
python3 examples/handlers/run.py osprey-ml
python3 examples/handlers/run.py koka
python3 examples/handlers/run.py ocaml
python3 examples/handlers/run.py eff
python3 examples/handlers/run.py effekt
```

Two examples live in each file. The first reuses one answer handler twice. The second supplies Ada or Grace to the same `greet` function, then reuses Ada's handler. Expected output:

```text
42
42
Hello, Ada!
Hello, Grace!
Hello, Ada!
```

Osprey's new definition and application need no `in` or `do`:

```osprey
effect Reader { name: fn() -> string }
fn reading(person) = handler Reader { name => person }
fn greet() = "Hello, " + perform Reader.name() + "!"
let ada = reading("Ada")
print(ada(greet))
```

The ML spelling is in [handlers.ospml](handlers.ospml). The prototype also supports installing a handler for the rest of a block: `handle Reader { name => "Ada" }`, followed by the work it handles, with no `in`/`do`.

| Language | Handler definition | Run the same work with it |
|---|---|---|
| [Osprey](handlers.osp) | `handler Reader { name => person }` | `ada(greet)` |
| [Koka](handlers.kk) | `handler` with a `fun name()` clause | `ada(greet)` |
| [OCaml](handlers.ml) | An `Effect.Deep.handler` record | `match_with greet () ada` |
| [Eff](handlers.eff) | `handler` with `effect Name k -> k person` | `with ada handle greet ()` |
| [Effekt](handlers.effekt) | A function wrapping `try`/`with name` | `reading("Ada") { greet() }` |

Effekt uses scoped computation blocks here; this is the same behavior, not a claim that its handler representation is identical to OCaml's records or Koka's functions. Osprey currently uses callable closures for handler values. The [effects spec](../../docs/specs/0017-AlgebraicEffects.md) defines the unified target; [plan 0016](../../docs/plans/0016-algebraic-effects-and-handlers.md) records what remains to implement.

## Continuation semantics

```sh
python3 examples/handlers/run.py all --demo semantics
python3 examples/handlers/run.py all --demo semantics --check
```

Both commands now run the same behavior in every language. Osprey declares `control value` and explicitly resumes when the work should continue. The expected output was preserved from the reference-language comparison.

| Probe | Koka `ctl`, OCaml, Eff, Effekt | Osprey, both flavors |
|---|---:|---:|
| Resume with 41; the body adds 1 | 42 | 42 |
| Resume with 41, then add 100 to the whole answer | 142 | 142 |
| Return 0 without resuming | 0 | 0 |
| Return 0 with an unreachable resume in another branch | 0 | 0 |

An ordinary operation supplies a value and continues once. A `control` operation receives the continuation and may resume or abandon it. Unreachable code does not change that mode. Koka makes the same distinction explicit with `fun` and `ctl`; Osprey records it on the operation's declaration. See `[EFFECTS-HANDLER-ARMS]` and [plan 0016](../../docs/plans/0016-algebraic-effects-and-handlers.md) for the remaining implementation work.

## Transforming the answer

```sh
python3 examples/handlers/run.py all --demo returns --check
```

The work still returns an integer. Swap in a handler that returns a string:

```osprey
let around = handler ControlAsk {
    value => "${resume(41)}!"
    return n => "done=${n}"
}
print(around(controlWork))
```

The return clause transforms normal completion to `done=42`. Resumption
receives that string, then the control arm adds `!`. A control arm returning
`"stopped"` bypasses the return clause. Every language's `returns` example
prints `done=42`, `done=42!`, then `stopped`.

| Language | Normal-completion transformation |
|---|---|
| [Osprey](returns.osp) / [ML flavor](returns.ospml) | `return n => "done=${n}"` |
| [Koka](returns.kk) | `return(n : int) label ++ n.show` |
| [OCaml](returns.ml) | The handler record's `retc` function |
| [Eff](returns.eff) | The handler's ordinary value-pattern clause |
| [Effekt](returns.effekt) | Transform the computation's result inside `try` |

Osprey's return clause runs outside its own handler, like its operation arms.
The Effekt example reproduces this pure transformation's output; an effectful
transform inside `try` would have a different scope. The compiler checks cover
Osprey's outward forwarding, repeated deep resumption, managed answers and
static handler captures under default, GC and ARC memory modes.

## Try an open-row callback prototype

After `cargo build --release -p osprey-cli`, run the [Default](open-rows.osp),
[ML](open-rows.ospml) and [Koka](open-rows.kk) versions:

```sh
python3 examples/handlers/run.py all --demo open-rows --check
```

All three print `live:job` and `test:job`. Osprey's `run` calls a callback
while its `!e` annotation leaves the callback's requirements open, so the same
`work` is interpreted by two different `Log` handlers. Koka writes the
relationship explicitly as `callback : () -> e ()` and `forward : e ()`.
Osprey's version is a closed-program prototype: function types do not yet
carry independently quantified rows, so the
[row-polymorphism acceptance gate](../../docs/plans/0016-algebraic-effects-and-handlers.md)
remains unfinished.

## Reproducibility

Verified locally with Koka 3.2.3, OCaml 5.4.1, Eff 5.1 (source commit `503da71b9cb927af04fc62e28511e63cd7199151`), and Effekt 0.80.0. Eff interpreter binding/type echoes are removed from the displayed comparison; its full transcript remains in `target/handler-demo/eff.log`. Compiler logs and executables stay under `target/handler-demo`, outside the source directory.

On another machine, install the language tools using their official instructions: [Koka](https://koka-lang.github.io/koka/doc/book.html#sec-install), [OCaml](https://ocaml.org/install), [Eff](https://github.com/matijapretnar/eff#installation--usage), [Effekt](https://effekt-lang.org/docs). The runner finds them on PATH or accepts `OSPREY`, `KOKA`, `OCAMLOPT`, `EFF`, and `EFFEKT` executable overrides. This is an executed five-language comparison, not an exhaustive survey of every language or library with effects.
