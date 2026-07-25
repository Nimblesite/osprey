# Fibers and Concurrency

The native target provides `Fiber<T>` computations and buffered `Channel<T>`
communication. A normal native fiber is backed one-to-one by a pthread; Osprey
does not expose a separate OS-thread API. Managed values captured by `spawn` or
sent through a channel may be co-owned across threads as specified by
[fiber boundary ownership](0018-MemoryManagement.md#fiber-boundary-ownership-mem-fiber-isolation).
The WebAssembly target excludes this pthread runtime.

Default and ML syntax lower the shipped forms to the same `Expr::Spawn`,
`Expr::Await`, `Expr::Yield`, `Expr::Send`, and `Expr::Recv` nodes. See
[ML Flavor Syntax](0024-MLFlavorSyntax.md#fibers-and-channels) for ML parsing.

## Spawn and await [CONCURRENCY-SPAWN-AWAIT]

`spawn expression` captures the expression's free values, schedules it, and
returns `Fiber<T>`, where `T` is the expression type. `await fiber` blocks until
that computation finishes and returns its `T` value. `Fiber<T>` has no public
record constructor; `spawn` is the construction operation.

```osprey
fn work(value: int) -> int = value + 1

let task = spawn work(41)
let answer = await(task)
```

```osprey-ml
work : int -> int
work value = value + 1

task = spawn (work 41)
answer = await task
```

Each spawn site allocates a distinct capture cell. `await` unboxes the result
back to its source type, including pointer and floating-point values.

## Buffered channels [CONCURRENCY-CHANNEL]

`Channel(capacity)` creates a FIFO channel whose positive integer `capacity` is
the number of buffered values. Zero and negative capacities are rejected; the
runtime does not implement rendezvous channels.

| Operation | Type | Behavior |
| --- | --- | --- |
| `Channel(capacity)` | `int -> Channel<T>` | Create a positive-capacity buffer; `T` is inferred from use. |
| `send(channel, value)` | `(Channel<T>, T) -> Unit` | Block while the buffer is full, then append `value`. |
| `recv(channel)` | `Channel<T> -> T` | Block while the buffer is empty, then remove its oldest value. |

`send` is not a `Result`: its native status is internal to the runtime call and
the language expression evaluates to `Unit`. `recv` directly returns `T`.

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

## Reserved `select` syntax [CONCURRENCY-SELECT-REJECT]

Both parsers reserve and lower `select`, but channel selection has no runtime
semantics. The type checker rejects every `select` expression with
`` `select` is not supported `` before code generation. Use explicit `send`,
`recv`, and `await` operations.
