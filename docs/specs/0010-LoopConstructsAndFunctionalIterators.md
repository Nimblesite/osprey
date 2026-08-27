# Iterators and Iteration

Osprey has no `for`, `while`, or `loop` construct. Range pipelines provide the
iteration surface in both language flavors.

## Core Iterator Functions — [BUILTIN-ITER]

`Iterator<T>` is the opaque type produced by `range`. It is distinct from a
materialized `List<T>` and is consumed by `forEach` or `fold`.

### `range(start: int, end: int) -> Iterator<int>` — [BUILTIN-ITER-RANGE]
Generates integers from `start` (inclusive) to `end` (exclusive).

```osprey
range(1, 5)      // 1, 2, 3, 4
```

```osprey-ml
range (1, 5)     // 1, 2, 3, 4
```

### `forEach(iterator: Iterator<T>, function: fn(T) -> Unit) -> Unit` — [BUILTIN-ITER-FOREACH]
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
fn add(total: int, value: int) -> int = (total + value) ?: total
range(1, 5) |> fold(0, add)   // 0+1+2+3+4 = 10
```

```osprey-ml
add : (int, int) -> int
add (total, value) = (total + value) ?: total

range (1, 5) |> fold (0, add)   // 0+1+2+3+4 = 10
```

## Callbacks and Accumulators — [BUILTIN-ITER-CALLBACK]

Callbacks may be lambdas, named functions, or function values. Generic named functions are specialized at the call site. Iterator combinators preserve a callback's complete return type, including `Result<T, E>`; they never unwrap a failure channel. A callback used where a plain accumulator or record field is required must handle checked integer arithmetic explicitly ([ARITH-CHECKED](0013-ErrorHandling.md#arithmetic-and-result--arith-checked)). The requirement reaches into callbacks precisely because the totality guarantee does: a fault inside a lambda passed to a combinator is discharged or the program is rejected, never dropped ([ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total)). Under [plan 0027](../plans/0027-arithmetic-effects.md) the callbacks below lose their `?:` and the region installs one policy instead.

```osprey
fn energy(p) = (p.mass + p.spin) ?: 0      // generic: inferred, no annotations
fn addEnergy(total: int, value: int) -> int = (total + value) ?: total
range(1, n) |> map(forge) |> map(energy) |> fold(0, addEnergy)
```

A `fold` accumulator may be any inferred type, including a record:

```osprey
fn bump(p, step) = p { mass: (p.mass + 1) ?: p.mass }
range(1, n) |> fold(Particle { id: 0, mass: 0, spin: 0 }, bump)   // -> Particle
```

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

Chains of `map` and `filter` over a range are emitted as one loop when consumed
by `forEach` or `fold`; no intermediate collection is created:

```osprey
range(1, 5) |> map(double) |> filter(isEven) |> forEach(print)
```

```osprey-ml
range (1, 5) |> map double |> filter isEven |> forEach print
```

is equivalent to:

```c
for (i = 1; i < 5; i++) {
    value = double(i);
    if (isEven(value)) print(value);
}
```
