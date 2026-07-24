# Iterators and Iteration

Osprey has no `for`, `while`, or `loop` construct. Iteration is expressed as composition of `range`, `forEach`, `map`, `filter`, and `fold` using the pipe operator `|>`.

> **Flavor layer — shared core.** Iterator functions use `Expr::Call`, pipes
> use `Expr::Pipe`, and callbacks use `Expr::Lambda`; stream fusion is
> flavor-blind. ML uses `\x => e`, whitespace for single-argument calls, and
> uncurried comma-lists for flat multi-argument built-ins (`range (1, 5)`,
> `fold (0, add)`). Whitespace application instead curries
> ([FLAVOR-ML-CALL](0024-MLFlavorSyntax.md)).

## Core Iterator Functions — [BUILTIN-ITER]

`Iterator<T>` is the type of a lazily-produced sequence. It exists during type-checking and is always fused away at compile time (see [Stream Fusion](#stream-fusion--builtin-iter-fusion)); it is never a materialised runtime collection. Like all built-ins, iterator functions take positional arguments in subject-first order (rule 4 of [Function Calls](0005-FunctionCalls.md#rules)); parameter names below are descriptive only.

### `range(start: int, end: int) -> Iterator<int>` — [BUILTIN-ITER-RANGE]
Generates integers from `start` (inclusive) to `end` (exclusive).

```osprey
range(1, 5)      // 1, 2, 3, 4
```

```osprey-ml
range (1, 5)     // 1, 2, 3, 4
```

### `forEach(iterator: Iterator<T>, function: fn(T) -> U) -> unit` — [BUILTIN-ITER-FOREACH]
Applies `function` to each element for its side effects.

```osprey
range(1, 5) |> forEach(print)
```

```osprey-ml
range (1, 5) |> forEach print
```

### `map(iterator: Iterator<T>, function: fn(T) -> U) -> Iterator<U>` — [BUILTIN-ITER-MAP]
Transforms each element.

```osprey
range(1, 5) |> map(double)
```

```osprey-ml
range (1, 5) |> map double
```

### `filter(iterator: Iterator<T>, predicate: fn(T) -> bool) -> Iterator<T>` — [BUILTIN-ITER-FILTER]
Keeps elements that satisfy `predicate`.

```osprey
range(1, 10) |> filter(isEven)
```

```osprey-ml
range (1, 10) |> filter isEven
```

### `fold(iterator: Iterator<T>, initial: U, function: fn(U, T) -> U) -> U` — [BUILTIN-ITER-FOLD]
Reduces an iterator to a single value.

```osprey
range(1, 5) |> fold(0, add)   // 0+1+2+3+4 = 10
```

```osprey-ml
range (1, 5) |> fold (0, add)   // 0+1+2+3+4 = 10
```

## Callbacks and Accumulators — [BUILTIN-ITER-CALLBACK]

The function argument to `forEach`/`map`/`filter`/`fold` may be a lambda **or a
function name** — and a named function is accepted whether it is monomorphic or
generic. A generic (unannotated-parameter) function has no emitted symbol; it is
specialised per element by inlining its body, exactly as a direct call would
specialise it. Passing one is therefore an ordinary composition, not a link
error:

```osprey
fn energy(p) = p.mass + p.spin      // generic: inferred, no annotations
range(1, n) |> map(forge) |> map(energy) |> fold(0, add)
```

A `fold` accumulator may be **any type**, including a record. The accumulator
travels through the fused loop in the uniform element representation and is
recovered at its real type at each use, so the reducer can field-access or
record-update it and the final result carries a usable layout:

```osprey
fn bump(p, step) = p { mass: p.mass + 1 }
range(1, n) |> fold(Particle { id: 0, mass: 0, spin: 0 }, bump)   // -> Particle
```

Both hold identically under every memory backend ([MEM-BACKENDS]): a record
accumulator is allocated, copied and — under ARC — freed with zero leaked
values. Exercised at scale by
`examples/tested/basics/memory/struct_allocation_stress.{osp,ospml}`.

## Pipe Operator — [BUILTIN-ITER-PIPE]

`|>` passes its left operand as the first argument to the function on its right.

```osprey
5 |> double |> print                                        // print(double(5))
range(1, 10) |> forEach(print)
range(0, 20) |> filter(isEven) |> map(double) |> forEach(print)
```

```osprey-ml
5 |> double |> print                                        // print(double(5))
range (1, 10) |> forEach print
range (0, 20) |> filter isEven |> map double |> forEach print
```

## Stream Fusion — [BUILTIN-ITER-FUSION]

Chains of `map`, `filter`, `forEach`, and `fold` over an iterator are fused at compile time into a single loop with no intermediate collections. The chain

```osprey
range(1, 5) |> map(double) |> filter(isEven) |> forEach(print)
```

```osprey-ml
range (1, 5) |> map double |> filter isEven |> forEach print
```

compiles to one loop that applies `double`, the `isEven` test, and `print` per element — equivalent to:

```c
for (i = 1; i < 5; i++) {
    value = double(i);
    if (isEven(value)) print(value);
}
```

Fusion applies to any chain of `map` and `filter` terminated by `forEach` or `fold`.

## Patterns

```osprey
// Transform → filter → aggregate
range(1, 20)
  |> map(square)
  |> filter(isEven)
  |> fold(0, add)
  |> print

// Pipeline of named stages
input()
  |> validateInput
  |> normalizeData
  |> processData
  |> formatOutput
  |> print
```

```osprey-ml
// Transform → filter → aggregate
range (1, 20) |> map square |> filter isEven |> fold (0, add) |> print

// Pipeline of named stages
input () |> validateInput |> normalizeData |> processData |> formatOutput |> print
```
