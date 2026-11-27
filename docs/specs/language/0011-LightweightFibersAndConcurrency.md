# Fibers and Concurrency

The native target provides `Fiber<T>` computations and buffered `Channel<T>`
communication. A normal native fiber is backed one-to-one by a pthread; Osprey
does not expose a separate OS-thread API. Managed values captured by `spawn` or
sent through a channel may be co-owned across threads as specified by
[fiber boundary ownership](../compiler/0018-MemoryManagement.md#fiber-boundary-ownership-mem-fiber-isolation).
The WebAssembly target runs the same fibers on its one thread ([CONCURRENCY-WASM](#fibers-on-wasm32-concurrency-wasm)).

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
fn work(value: int) -> int = satAdd(value, 1)

let task = spawn work(41)
let answer = await(task)
```

```osprey-ml
work : int -> int
work value = satAdd value 1

task = spawn (work 41)
answer = await task
```

`await` returns the fiber's complete source type and never erases a `Result` channel. Host arithmetic in a spawned body reaches the enclosing `Arith` handler through the serialized `[EFFECTS-FIBER-PERFORM]` round trip, or the program is rejected ([ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total)). Launching a selected-device kernel from a fiber retains the current [device-recovery limitation](0037-ArithmeticEffects.md#device-recovery-arith-device-recovery): a kernel fault aborts instead of reaching that handler. Each spawn site allocates a distinct capture cell. Pointer and floating-point values likewise return with their source type.

A `Fiber<T>` handle is reusable: awaiting the same completed fiber more than
once MUST return the same `T` value on every call. For a managed `T`, every
`await` produces an independently owned reference. One caller releasing its
value MUST NOT invalidate a later await. A fiber's result is released with the
fiber, when nothing can await it again ([CONCURRENCY-FIBER-LIFETIME]); it MUST
NOT be released while any handle to the fiber is still held.

## How many fibers and channels [CONCURRENCY-HANDLES]

A program can create any number of fibers and channels over its life. A fiber costs memory only while something can still await it or it is still running ([CONCURRENCY-FIBER-LIFETIME]): a program that spawns and awaits in a loop runs in constant memory, however many fibers it makes. A channel costs memory only while something can still name it ([CONCURRENCY-CHANNEL-LIFETIME](#channel-lifetime-concurrency-channel-lifetime)): a few hundred bytes plus its buffer (the runtime tests fail above 512 bytes for a channel of one word), given back when its last handle dies.

A fiber's thread ends when the fiber does, whether or not it is awaited. When the system has no thread left to give, `spawn` runs the body on the spawning thread instead, and the fiber has finished when `spawn` returns. A body that then waits for its spawner blocks for ever.

## Fiber lifetime [CONCURRENCY-FIBER-LIFETIME]

This section explains when a fiber's memory is given back, in every memory backend and on every target. A `Fiber<T>` is a future in Halstead's sense: a placeholder for a value another thread is computing, which every touch reads without recomputing it ([Halstead, "Multilisp", TOPLAS 7(4), 1985](https://doi.org/10.1145/4472.4478)). Baker and Hewitt observed that a process nothing can reach is garbage like any other storage, and that a garbage collector can find it ([Baker and Hewitt, "The Incremental Garbage Collection of Processes", MIT AI Memo 454, 1977](https://dspace.mit.edu/handle/1721.1/41969)). Osprey applies that rule to finished fibers.

A fiber is held by:

- the program's handle, a managed value the memory backend reclaims like any other: ARC at its last release, the tracing collector at the first sweep after it becomes unreachable;
- its runner, from `spawn` until the body has finished: the fiber's thread, the stack running it in place, or the sequential queue waiting to run it ([CONCURRENCY-DETERMINISTIC](#deterministic-execution-concurrency-deterministic));
- each `await`, `fiberDone` and interpolation of the fiber, for as long as it reads the fiber.

The last holder to let go frees the fiber, its result and its synchronisation. So:

The rule is the same in every memory backend and on every target:

| Backend | When the handle dies | Who drops the handle's hold |
| --- | --- | --- |
| `gc` (native) | at the first sweep after the handle is unreachable | the runtime, at the next `spawn` or the end of `main` |
| `arc` (native, wasm32) | at the handle's last release | the same |
| `ownership` (native, wasm32) | when the last of its sharers frees it with `osp_ownership_free`; a handle is the one value a program shares there ([MEM-OWNERSHIP-STATIC-HANDLES](../compiler/0018-MemoryManagement.md#rules-for-shared-values-mem-ownership-static-rules)) | the same |

- A fiber that has finished and that the program can no longer name is freed. 1.2 million sequential `await(spawn …)` used to keep every fiber, 330 MiB outside the managed heap and its ceiling (#359); `finished_fibers_are_reclaimed_in_every_backend` now holds the compiled program under 64 MiB natively under gc, arc and ownership and on wasm32 under arc and ownership (ownership runs int fibers, as it refuses a string handed out of one).
- A running fiber is never freed or cancelled because the program dropped its handle. Baker and Hewitt also collect unreachable running processes, but an Osprey fiber may still perform effects nobody else can see, so dropping the handle detaches it instead.
- `await` and `fiberDone` take their own hold, so a collector that reclaims the handle while they wait never frees the fiber under them.

A handle's finalizer runs inside the memory backend, which may hold its heap lock or have stopped every other thread. Running arbitrary cleanup there deadlocks or corrupts state, so a finalizer must not take locks or allocate and must hand its work to an ordinary thread ([Boehm, "Destructors, Finalizers, and Synchronization", POPL 2003](https://www.hboehm.info/popl03/web)). The finalizer therefore only pushes the fiber onto a lock-free list, a Treiber stack ([Treiber, IBM RJ 5118, 1986](https://dominoweb.draco.res.ibm.com/reports/rj5118.pdf)) whose single consumer takes the whole list at once, so no pop can meet a recycled node. The runtime drops those holds outside the collector, at the next `spawn` and at the end of `main`.

The holder count is proved correct with Osprey's proof kernel. [`FiberLifetime.lean`](../../../src/toolchain/benchmarks/proof/tools/FiberLifetime.lean) models a fiber as `live n` (`n + 1` holders) or `freed`, `held_hold` as `acquire` and `held_release` as `release`, and a step on a freed fiber as a use after free. `make _check_synthesis_proofs` exports ten theorems and requires the Osprey kernel built for gc, arc and wasm32 to accept each with no axioms:

| Theorem | Claim |
| --- | --- |
| `run_valid` | Every trace in which only holders act runs without a use after free and leaves exactly the holders it says. |
| `freed_iff` | The fiber is freed exactly when no holder is left. |
| `freed_is_final` | Nothing can be done to a freed fiber. |
| `spawn_then_both_release` | A spawned fiber, awaited any number of times, is freed once its runner finishes and its handle dies. |
| `dead_list_conserves` | Whatever the interleaving of finalizer pushes and runtime drains, no fiber is lost from the dead-handle list or duplicated in it. |
| `drained_releases_each_once` | After a final drain, every fiber a finalizer pushed has had its handle's hold dropped exactly once per push. |
| `channel_then_handle_release` | A channel, used any number of times, is freed once its handle dies ([CONCURRENCY-CHANNEL-LIFETIME](#channel-lifetime-concurrency-channel-lifetime)). |
| `share_is_acquire` | Under ownership, sharing a handle takes exactly one hold. |
| `free_is_release` | Under ownership, a sharer's free ends exactly one hold. |
| `free_last_iff` | Under ownership, a handle is finalized and freed exactly by its last sharer. |

The same target builds three mutants, a last release that keeps the fiber (the #359 leak), a drain that loses the list it took, and an ownership free that ends a handle another sharer still holds, and fails unless Lean rejects each. The model covers the count, not the C code that keeps it: the C runtime's suite checks that code under AddressSanitizer, including a fiber whose other holders all let go while it ran.

The C runtime's suites check the code against the model: `fiber_runtime_tests` (gc, also under AddressSanitizer with leak detection on Linux) spawns, awaits and drops 50,000 fibers and requires under 8 bytes left per fiber, and runs a fiber whose every other holder lets go while it runs; `memory_ownership_tests` requires a fiber or channel handle's finalizer to run exactly once, at the free by the last of up to seven sharers, and never with nothing registered; the GC and ARC suites check the subtype dispatch. `finished_fibers_are_reclaimed_in_every_backend` runs 1.2 million fibers end to end.

The proofs model the count; they are not derived from the C code. The [IR to Lean framework](../tooling/IrToLean.md) could one day read the compiled runtime itself, but it is not in CI yet, translates 12 of the 76 functions of its first module, and has no model of atomic read-modify-write instructions, which `held_release` is built on.

A fiber blocked for ever on a channel nothing else can reach is still held by its runner; [CONCURRENCY-DEADLOCK](#fibers-blocked-for-ever-concurrency-deadlock) covers finding it.

## Channel lifetime [CONCURRENCY-CHANNEL-LIFETIME]

This section explains when a channel's memory is given back. A channel follows the fiber's rule: it is reclaimed once nothing can name it again. A `Channel<T>` is a managed handle like a `Fiber<T>`, and the channel behind it is held by:

- the program's handle, however many fibers, records and other channels share it;
- each `send`, `recv` and `select` arm, for as long as it reads the channel, a blocked one included.

The last holder to let go frees the channel, its buffer and its synchronisation, and releases every value that was sent and never received: the channel owns those from `send` until `recv`. A fiber that captures a channel shares its handle, so a channel lives at least as long as any fiber that can still use it.

| Backend | When the handle dies | Who drops the handle's hold |
| --- | --- | --- |
| `gc` (native) | at the first sweep after the handle is unreachable. Memory the runtime keeps outside the managed heap for fibers and channels counts towards the next collection, so a loop that only makes channels still collects | the runtime, at the next `spawn` or `Channel(…)` and at the end of `main` |
| `arc` (native, wasm32) | at the handle's last release | the same |
| `ownership` (native, wasm32) | when the last of its sharers frees it ([MEM-OWNERSHIP-STATIC-HANDLES](../compiler/0018-MemoryManagement.md#rules-for-shared-values-mem-ownership-static-rules)) | the same |

A channel was a number indexing a table that kept every channel for the life of the process: 600 thousand sequential `Channel(256)` reached 1.4 GiB under ARC and 1.8 GiB under the tracing collector. `finished_channels_are_reclaimed_in_every_backend` now holds that compiled program under 64 MiB natively under gc, arc and ownership and on wasm32 under arc and ownership, and `shared_channels_end_with_their_last_sharer` runs fibers that share channels on the same five and requires nothing live at exit. The C suite makes, uses and drops 55,000 channels, half with a value still buffered, and requires under 8 bytes left per channel, also under AddressSanitizer. The holder count is the fiber's, proved in the same file (`channel_then_handle_release`).

`Channel(capacity)` answers `Success` with a handle, or `Error` when the C entry point `channel_create` answers no handle for a capacity it cannot honour. Nothing aborts ([FAILURE-EXPLICIT]), and every runtime operation on no handle, or on a handle that holds no channel, fails without touching memory.

## Fibers blocked for ever [CONCURRENCY-DEADLOCK]

This section explains how a run finds fibers that can never be woken. A fiber waiting in `send` or `recv` is woken only by another operation on the same channel, and an operation needs the channel's handle. So a waiting fiber whose channel only waiting fibers can reach is blocked for ever: a partial deadlock. The program may carry on, but that fiber, its thread and everything it holds are lost.

Osprey finds these fibers the way Saioc, Lee, Møller and Chabbi do for Go: reachability from the collector's roots over-approximates what a program can still use, so a waiting fiber the collector cannot connect to a running one can never run ([Saioc, Lee, Møller and Chabbi, "Dynamic Partial Deadlock Detection and Recovery via Garbage Collection", ASPLOS 2025](https://cs.au.dk/~amoeller/papers/golf/paper.pdf)). At each collection the tracing collector:

1. traces from every root except the stacks and records of fibers waiting on a channel;
2. adds each waiting fiber whose channel handle that trace reached, traces from it, and repeats until no fiber is added;
3. counts every waiting fiber still left as blocked for ever, once, then traces from those too, so nothing a stuck fiber holds is ever freed under it.

The runtime writes the count to stderr outside the collector, at the next `spawn` or `Channel(…)` and at the end of `main`, where a last collection runs if any fiber is still waiting:

```text
osprey: 20 fiber(s) blocked for ever: only blocked fibers can reach the channel each waits on [CONCURRENCY-DEADLOCK]
```

The report is a diagnostic: the exit status and the program's output do not change, and the fiber is not cancelled, because nothing can run its cleanup for it.

| Where | What is found |
| --- | --- |
| `gc` (native) | every fiber waiting in `send` or `recv` on a channel only waiting fibers reach, unless a stale word on some stack still names the channel |
| `arc`, `ownership` (native) | nothing: the waiting fiber's own reference keeps the channel's count above zero, and a count cannot tell that reference from one that could wake it |
| one thread (wasm32, sequential mode's last wait, `osprey hunt`) | the whole-program case, exactly: a wait with no fiber left to run stops the run with status 70 ([CONCURRENCY-WASM](#fibers-on-wasm32-concurrency-wasm)), and the symbolic engine reports it as a finding at the wait's line |

A report is never a false alarm. Scanning conservatively only adds references, so a fiber that could be woken is always connected; one that is stuck may go unreported. A fiber waiting in `select`, or awaiting a fiber that is itself stuck, is not counted.

[`FiberDeadlock.lean`](../../../src/toolchain/benchmarks/proof/tools/FiberDeadlock.lean) proves the rule with Osprey's proof kernel, with no axioms, on the gc, arc and wasm32 kernels (`make _check_synthesis_proofs`). It models what a collection sees (the references each fiber holds, the references between objects, the channel each waiting fiber waits on) and everything a running fiber can do to it: store a reference it can name, hand one to another fiber, start waiting, and wake a waiter on a channel it can name.

| Theorem | Claim |
| --- | --- |
| `step_keeps_known` | Whatever a live fiber can name after a step, a live fiber could name before it. |
| `step_keeps_live` | No step makes a fiber live that was not. |
| `step_keeps_stuck` | A fiber the collector would report stays reportable after any step. |
| `stuck_for_ever` | The same across any number of steps. |
| `stuck_never_runs` | So a reported fiber never runs again, whatever the rest of the program does. |
| `reachable_wait_is_live` | A fiber waiting on a channel a running fiber can name is never reported. |

The same target builds a mutant in which a fiber wakes a waiter on a channel it cannot name, and fails unless Lean rejects it. The proof covers the rule; the C loop that computes the fixpoint is checked by `fiber_runtime_tests` (40 stuck fibers found, each once; 40 whose channel the program still names never counted, then woken), also under AddressSanitizer, and end to end by `fibers_blocked_for_ever_are_named_by_the_collector`.

## Fibers on wasm32 [CONCURRENCY-WASM]

This section explains how fibers run where there are no threads. WASI preview 1 gives a module one thread, so on `--target=wasm32` every fiber runs on it in spawn order, exactly as [sequential execution](#deterministic-execution-concurrency-deterministic) runs them natively: `spawn` queues the body, and `await`, a blocked `send` or `recv`, and `select` run the queue until what they wait for is ready. `yield` is a passthrough and `fiberDone` reports a queued fiber ready, as in sequential mode. The fiber runtime is compiled for WASI against wasi-libc's single-threaded pthread stubs, so it is the same code as native, with the same lifetime rules and proofs ([CONCURRENCY-FIBER-LIFETIME](#fiber-lifetime-concurrency-fiber-lifetime)).

Every fiber shares the one stack, and both wasm hosts give it a native program's depth ([WASM-TARGET-NODE](../compiler/0022-WebAssemblyTarget.md)), so `fiber_showcase` recurses 80,000 frames inside a fiber on wasm32 as it does natively. `sleep` sleeps the thread through WASI's own clock subscription.

A wait that no queued fiber can satisfy could only be woken by another thread, and there is none. Instead of hanging, the run stops with `osprey: every fiber is blocked: this wait can never be woken` and exit status 70. A `select` with nothing queued takes its `timeout` arm at once, without waiting for the clock, as in [sequential execution](#deterministic-execution-concurrency-deterministic).

## Buffered channels [CONCURRENCY-CHANNEL]

`Channel(capacity)` creates a FIFO channel whose positive integer `capacity` is
the number of buffered values. A zero, negative or unallocatable capacity is an
`Error` with the message `Channel capacity must be positive and fit in memory`;
the runtime does not implement rendezvous channels, and it never aborts.

| Operation | Type | Behavior |
| --- | --- | --- |
| `Channel(capacity)` | `int -> Result<Channel<T>, string>` | Create a positive-capacity buffer, or an `Error` for a capacity it cannot honour; `T` is inferred from use. |
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

A handle stored in a generic field also preserves its resolved type. For
`type Box<t> = Box { slot: t }`, a field containing
`Channel<List<List<int>>>` carries the instantiated channel element type into
`recv`; a field containing `Fiber<T>` carries the instantiated result type into
`await`. Field access uses the solved instance, including through aliases and
nested records. It must never guess from the erased machine word. A backend
that cannot recover this metadata must reject the program with a diagnostic.
The former metadata-loss reproduction sends `[[1, 2, 3]]` through a boxed
channel and now correctly reports row length `3` rather than `0`.

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
fn exchange(channel) = {
    let producer = spawn {
        send(channel, 1)
        send(channel, 2)
        send(channel, 3)
    }
    let consumer = spawn "got ${recv(channel)} ${recv(channel)} ${recv(channel)}"
    await(producer)
    print(await(consumer))
}

match Channel(3) {
    Success { value } => exchange(value)
    Error { message } => print("no channel: ${message}")
}
```

```osprey-ml
exchange channel =
    producer = spawn
        send channel 1
        send channel 2
        send channel 3
    consumer = spawn "got ${recv channel} ${recv channel} ${recv channel}"
    await producer
    print (await consumer)

match Channel 3
    Success value => exchange value
    Error message => print "no channel: ${message}"
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
requested fiber. A fiber can use the whole runtime as it runs, on the stack of
whatever ran it. A `send`, `recv` or `select` that would block runs the next
queued fiber instead, as nothing else could unblock it. An `await` that nothing
on the stack could ever answer, such as a fiber awaiting itself, returns `-1` in
the native runtime. This mode is sequential, not an interleaving scheduler;
`yield` therefore cannot switch fibers. `fiberDone` returns `1` for a queued
fiber because its following `await` is what drives execution.

`cancel` before the victim's driving `await` MUST still dequeue the victim
from the deterministic execution queue before it is driven. This is part of
the [Structured Concurrency](0036-StructuredConcurrency.md) target and is not
implemented yet, as stated in [CONCURRENCY-CANCEL-DESIGN].

## Cancellation and scopes — design direction [CONCURRENCY-CANCEL-DESIGN]

The shipped runtime has **no cancellation**: a spawned fiber always runs to
completion, `await` blocks until it does, and `fiberDone` is the only probe.
The normative target that changes this — lexical scopes that own their fibers,
`cancel` delivered only at suspension points, `finally` finalizers, `join`
returning an `Outcome<T>`, and deadline/racing forms — is specified in
[Structured Concurrency](0036-StructuredConcurrency.md) and delivered by
the structured-concurrency section of [plan 0016](../../plans/0016-algebraic-effects-and-handlers.md#structured-concurrency). Until it lands,
nothing in this section's target exists in the compiler.

## Choosing among channel operations [CONCURRENCY-SELECT]

`select` waits for the first of several channel operations and runs that arm:

```osprey
let outcome = select {
    message = recv(inbox) => handle(message)
    recv(cancel) => "stopped"
    send(outbox, reply) => "sent"
    timeout(500) => "timed out"
}
```

```osprey-ml
outcome = select
    message = recv inbox => handle message
    recv cancel => "stopped"
    send outbox reply => "sent"
    timeout 500 => "timed out"
```

- **Arms.** An arm is `name = recv(channel)`, `recv(channel)`, `send(channel, value)`,
  `timeout(milliseconds)` or `_`. A receive binding has the channel's element type, and
  `_ = recv(channel)` receives and drops. A `select` needs at least one `send` or `recv`
  arm and takes at most one `timeout` or `_` arm.
- **Evaluation.** Every channel, value and timeout operand evaluates once on entry, in
  source order, whichever arm runs.
- **Choice.** The first ready arm in source order runs, in every mode, and is the only
  operation performed. A program that needs fairness among busy channels orders or
  rotates its own arms. With nothing ready, `_` runs at once, `timeout` runs when the
  time has passed (`timeout(0)` is `_`), and otherwise the `select` blocks until an arm
  is ready.
- **Ownership.** A send that is not chosen drops the reference it would have handed to
  the channel; a received managed value belongs to the chosen arm's region.
- **Result.** The arms' bodies join as a `match`'s do, so the `select` has their type.
- **Deterministic mode.** With nothing ready, a `select` runs queued fibers one at a time
  in spawn order until an arm is ready. Once none is left, `_` or `timeout` runs without
  waiting for the clock, since nothing could change. Otherwise the `select` blocks, as a
  `recv` on an empty channel does.
- **Targets.** Native only; wasm32 and the Python target reject it like every other fiber
  operation.

