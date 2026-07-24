# Fibers and Concurrency

Fibers are lightweight concurrent computations. They are constructed as values of `Fiber<T>` and communicate through `Channel<T>`. There are no OS threads exposed to user code; the runtime schedules fibers cooperatively. Values cross fiber boundaries — `spawn` captures and channel `send` — by move or copy, never by sharing ([MEM-FIBER-ISOLATION] in [Memory Management](0018-MemoryManagement.md)).

> **Flavor layer — shared core.** Concurrency lowers to `Expr::Spawn`,
> `Expr::Yield`, `Expr::Await`, `Expr::Send`, `Expr::Recv`, and `Expr::Select`.
> Scheduling and channel semantics are shared; ML spellings are in
> [ML Flavor Syntax](0024-MLFlavorSyntax.md).

## Status

`spawn`, `await`, `yield`, and basic channel operations are implemented. `yield`
performs a real cooperative hand-off: in concurrent (thread-backed) execution it
donates the CPU to the scheduler and resumes when next scheduled, forwarding its
operand unchanged. Under the deterministic execution mode used by the test
harness, fibers run sequentially to completion, so `yield` forwards its value
without re-ordering; true cross-fiber interleaving under deterministic mode would
require stackful context switching and is not yet implemented. The `select`
expression is planned and not yet wired through code generation.

## Core Types

```osprey
Fiber<T>     // a concurrent computation that produces a value of type T
Channel<T>   // an in-process communication channel carrying values of type T
```

## Constructing Fibers

Fibers are constructed using ordinary record-construction syntax:

```osprey
let task = Fiber<int> {
    computation: fn() => calculatePrimes(n: 1000)
}
```

```osprey-ml
task = spawn (calculatePrimes 1000)
```

`spawn <expr>` is sugar for the equivalent `Fiber` construction:

```osprey
let result = spawn 42
// equivalent to:
let result = Fiber<int> { computation: fn() => 42 }
```

```osprey-ml
result = spawn 42
```

## Constructing Channels

```osprey
let sync   = Channel<int>    { capacity: 0  }   // unbuffered (rendezvous)
let buf    = Channel<string> { capacity: 10 }   // buffered
```

```osprey-ml
sync = Channel 0    // unbuffered (rendezvous)
buf  = Channel 10   // buffered
```

## Operations

| Operation                                       | Signature                                            |
| ----------------------------------------------- | ---------------------------------------------------- |
| Wait for a fiber to produce its value           | `await(fiber: Fiber<T>) -> T`                        |
| Send a value to a channel                       | `send(channel: Channel<T>, value: T) -> Result<unit, ChannelError>` |
| Receive a value from a channel                  | `recv(channel: Channel<T>) -> Result<T, ChannelError>` |
| Yield to the scheduler, forwarding the value    | `yield(value: T) -> T`                               |

## Producer / Consumer Example

```osprey
let ch = Channel<int> { capacity: 3 }

let producer = spawn {
    range(1, 4) |> forEach(fn(i) => send(ch, i))
}

let consumer = spawn {
    range(1, 4) |> forEach(fn(i) => match recv(ch) {
        Success { value }   => print("got ${value}")
        Error   { message } => print("recv error: ${message}")
    })
}

await(producer)
await(consumer)
```

```osprey-ml
ch = Channel 3

producer = spawn
    range (1, 4) |> forEach (\i => send ch i)

consume i =
    match recv ch
        Success value   => print "got ${value}"
        Error   message => print "recv error: ${message}"

consumer = spawn
    range (1, 4) |> forEach consume

await producer
await consumer
```

## select (planned)

`select` waits on multiple channel operations and runs the arm whose operation completes first:

```ebnf
selectExpr ::= "select" "{" selectArm+ "}"
selectArm  ::= IDENT "=>" channelOp "=>" expr
             | "_"   "=>" expr   (* timeout / default *)
```

```osprey
let ch1 = Channel<string> { capacity: 1 }
let ch2 = Channel<int>    { capacity: 1 }

select {
    msg => recv(ch1) => processString(msg)
    num => recv(ch2) => processNumber(num)
    _   => timeoutHandler()
}
```

> `select` is planned and not yet implemented in either flavor; the ML surface
> syntax for the channel-`select` arm form is unspecified. Use the Default flavor
> illustration above.
