# Fibers and Concurrency

The native target provides `Fiber<T>` computations and buffered `Channel<T>`
communication. A normal native fiber is backed one-to-one by a pthread; Osprey
does not expose a separate OS-thread API. Managed values captured by `spawn` or
sent through a channel may be co-owned across threads as specified by
[fiber boundary ownership](0018-MemoryManagement.md#fiber-boundary-ownership-mem-fiber-isolation).
The WebAssembly target excludes this pthread runtime.

Default and ML syntax lower these forms to the same `Expr::Spawn`,
`Expr::Await`, `Expr::Yield`, `Expr::Send`, and `Expr::Recv` nodes. See
[ML Flavor Syntax](0024-MLFlavorSyntax.md#fibers-and-channels) for ML parsing.

## Sleep [CONCURRENCY-SLEEP]

`sleep(milliseconds: int) -> Unit` blocks the current native thread for the
given number of milliseconds. A duration less than or equal to zero returns
immediately. The runtime's integer status is not exposed.

## Spawn and await [CONCURRENCY-SPAWN-AWAIT]

`spawn expression` captures the expression's free values, schedules it, and
returns `Fiber<T>`, where `T` is the expression type. `await fiber` blocks until
that computation finishes and returns its `T` value. `Fiber<T>` has no public
record constructor; `spawn` is the construction operation.

```osprey
fn work(value: int) -> Result<int, MathError> = value + 1

let task = spawn work(41)
let answer = await(task) ?: 0
```

```osprey-ml
work : int -> Result<int, MathError>
work value = value + 1

task = spawn (work 41)
answer = (await task) ?: 0
```

Under [ARITH-CHECKED](0013-ErrorHandling.md#arithmetic-and-result--arith-checked),
`task` is a `Fiber<Result<int, MathError>>`: `await` returns that complete source
type and `?: 0` explicitly selects the example's overflow policy. It never
erases a `Result` channel. Each spawn site allocates a distinct capture cell.
Pointer and floating-point values likewise return with their source type.

A `Fiber<T>` handle is reusable: awaiting the same completed fiber more than
once MUST return the same `T` value on every call. For a managed `T`, every
`await` produces an independently owned reference. One caller releasing its
value MUST NOT invalidate a later await. Once every spawned computation is
quiescent, normal program teardown MUST release the runtime's completed-result
roots after language-owned values have dropped; it MUST NOT release a cached
result while another fiber can still await it.

## Buffered channels [CONCURRENCY-CHANNEL]

`Channel(capacity)` creates a FIFO channel whose positive integer `capacity` is
the number of buffered values. Zero and negative capacities are rejected; the
runtime does not implement rendezvous channels.

| Operation | Type | Behavior |
| --- | --- | --- |
| `Channel(capacity)` | `int -> Channel<T>` | Create a positive-capacity buffer; `T` is inferred from use. |
| `send(channel, value)` | `(Channel<T>, T) -> Unit` | Block while the buffer is full, then append `value`. |
| `recv(channel)` | `Channel<T> -> T` | Block while the buffer is empty, then remove its oldest value. |

A channel handle is MONOMORPHIC in `T`. Binding one with `let` does not
generalize its element type, so every `send` and `recv` on that handle agrees
on one `T`; sending a `string` and receiving an `int` from the same channel is
a type error, not two independent instantiations. `Fiber<T>` is fixed the same
way by the thunk that produced it.

`recv` returns `T` with its FULL representation, and the guarantee does not
depend on how the handle was reached: a channel read through a `let` binding,
an alias, a function parameter or a function return all deliver the same value.
A nested `List<List<U>>` comes back whole rather than as its outer shape with
the element type erased, and a collection crosses the wire in the runtime
representation a receiver can read — a backend may not put a construction-time
layout on a channel that only its own scope knows how to interpret.

One route does NOT carry `T`, and it is REJECTED rather than guessed at: a
handle stored in a field whose DECLARED type is a type variable — `type Box<t> =
Box { slot: t }` — reaches `recv` with nothing to unbox by, because the
declaration is all the field read has. `recv` and `await` refuse such a program
and name the field. They used to fall back to the uniform wire word, which is a
plausible WRONG VALUE and not an error: a `Channel<List<List<int>>>` read out of
such a field answered its outer shape as an integer, so reading a row out of it
produced `0` where the answer was `3` — exit status 0, no diagnostic, nothing to
notice. The same rule and the same refusal apply to `Fiber<T>`.

A managed `T` handed to `send` is OWNED by the channel until a `recv` takes it.
A send the runtime rejects owns nothing and releases the value again; a value
still buffered when the program ends is released at teardown. Neither an
unreceived send nor a rejected one may leave a live object behind — the ARC
corpus is run with leak accounting armed and holds every program to zero.

`send` is not a `Result`: its native status is internal to the runtime call and
the language expression evaluates to `Unit`. `recv` directly returns `T`. A
backend MUST preserve a `Result<U, E>` element as a complete value; a backend
without a shape-aware channel ABI MUST reject `Channel<Result<U, E>>` at
compilation. It must never unwrap or reinterpret the channel element.

```osprey
let channel = Channel(3)

let producer = spawn {
    send(channel, 1)
    send(channel, 2)
    send(channel, 3)
}

let consumer = spawn {
    print("got ${recv(channel)}")
    print("got ${recv(channel)}")
    print("got ${recv(channel)}")
}

await(producer)
await(consumer)
```

```osprey-ml
channel = Channel 3

producer = spawn
    send channel 1
    send channel 2
    send channel 3

consumer = spawn
    print "got ${recv channel}"
    print "got ${recv channel}"
    print "got ${recv channel}"

await producer
await consumer
```

## Yield and completion [CONCURRENCY-YIELD]

`yield value` evaluates `value`, offers the current pthread's remaining time
slice to the scheduler, and returns the same value with the same type. Bare
`yield` returns `Unit`. In deterministic mode the scheduler hand-off is skipped,
but the value is still returned. The legacy `fiber_yield(int) -> int` built-in
uses the same runtime operation.

`fiberDone(fiber: Fiber<T>) -> int` is a non-blocking completion probe. It
returns `1` after completion, `0` while a normal threaded fiber is running, and
the native runtime returns `-1` for an invalid handle.

## Deterministic execution [CONCURRENCY-DETERMINISTIC]

Native code may declare the runtime control function:

```osprey
extern fn fiber_set_deterministic_mode(enabled: bool) -> int
```

When enabled before spawning, `spawn` queues computations without creating
pthreads. `await` executes queued computations in spawn order through the
requested fiber. This mode is sequential, not an interleaving scheduler;
`yield` therefore cannot switch fibers. `fiberDone` returns `1` for a queued
fiber because its following `await` is what drives execution.

## Cancellation and scopes — design direction [CONCURRENCY-CANCEL-DESIGN]

The shipped runtime has **no cancellation**: a spawned fiber always runs to
completion, `await` blocks until it does, and `fiberDone` is the only probe.
The normative target that changes this — lexical scopes that own their fibers,
`cancel` delivered only at suspension points, `finally` finalizers, `join`
returning an `Outcome<T>`, and deadline/racing forms — is specified in
[Structured Concurrency](0036-StructuredConcurrency.md) and delivered by
[plan 0026](../plans/0026-structured-concurrency.md). Until that plan lands,
nothing in this section's target exists in the compiler.

## Reserved `select` syntax [CONCURRENCY-SELECT-REJECT]

Both parsers reserve and lower `select`, but channel selection has no runtime
semantics. The type checker rejects every `select` expression with
`` `select` is not supported `` before code generation. Use explicit `send`,
`recv`, and `await` operations.
