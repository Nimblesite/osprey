# Structured Concurrency: Cancellation and Turn Isolation

**Status:** normative target; implementation has not started. Delivery is fixed
by [plan 0026](../plans/0026-structured-concurrency.md). Today's shipped fiber
behavior — no cancellation, fibers always run to completion — is specified in
[Fibers and Concurrency](0011-LightweightFibersAndConcurrency.md).

The key words `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are to be interpreted as
described by BCP 14 (RFC 2119 and RFC 8174) when they appear in capitals. A
feature is not implemented merely because this document specifies it.

## One mechanism

Most languages bolt concurrency control onto the side: a cancellation token
threaded through every signature (.NET), a `Context` as the first parameter of
every function (Go), an exception that can strike between any two instructions
and a `mask` primitive to hold it off (Haskell), and a mutex around anything
shared. Osprey already has the primitive the research literature builds all of
this from: in an effect-handler runtime, a suspended fiber **is** a
continuation held by a handler
([Leijen, TyDe 2017](#references); [Dolan et al., TFP 2017](#references)).

- **To cancel is to decline to resume** the continuation and run its
  finalizers instead.
- **To serialize is to resume one performer at a time** — which the shipped
  runtime already does for every resuming handler
  ([EFFECTS-FIBER-PERFORM](0017-AlgebraicEffects.md#resuming-handlers)).
- **To compose is to make several operations one turn.**

Concurrency control is therefore not a library beside the effect system. It is
what the handler runtime already does, stated as law and checked by the same
effect rows the compiler already infers. No token parameter, no context
plumbing, no function coloring, and — because interruption can only land where
an effect row says the code can pause — no `mask` primitive at all.

## Part 1 — Scopes and cancellation

### Research basis — [CANCEL-RESEARCH]

Every rule below traces to published work; the [References](#references)
section holds the full citations.

| Decision | Source |
| --- | --- |
| Fibers are owned by a lexical scope; no fiber outlives its scope | Structured concurrency: Smith 2018 (Trio nurseries), Sústrik 2016, Kotlin coroutine scopes (Elizarov et al. 2021), Java `StructuredTaskScope` (JEP 505 preview line), OCaml Eio switches |
| Cancellation is an effect-handler action: drop the continuation, run finalizers | Leijen, TyDe 2017; Ahman & Pretnar, POPL 2021 (interrupts as asynchronous effects) |
| Cancellation lands only at suspension points; pure code is uninterruptible | The inverse of Haskell's asynchronous exceptions (Marlow et al., PLDI 2001), whose "interruptible anywhere, opt out with `mask`" default is this design's cautionary tale |
| Cleanup runs as handler finalization, shielded from further cancellation | Leijen 2018 (deep finalization / Koka `finally`); Trio and Eio shielding |
| Cancelling a client must never corrupt a shared abstraction | Flatt & Findler, PLDI 2004 (kill-safety) |
| Cancellation is not catchable, only finalizable | Kotlin's swallowed-`CancellationException` bug class, deliberately made unrepresentable |

### Scopes own fibers — [CANCEL-SCOPE]

```ebnf
scopeExpr ::= "scope" blockExpr
```

`scope { ... }` evaluates its block with a new fiber scope installed. Every
`spawn` attaches its fiber to the innermost enclosing scope. The scope
expression's value is its block's value, and its type is the block's type —
a scope adds no wrapper.

- **Normal exit:** the scope MUST wait for every child fiber to complete
  before the scope expression produces its value. Work cannot leak past the
  expression that created it.
- **Unwinding exit** (the enclosing fiber is itself cancelled): the scope
  MUST cancel every child, then wait for their finalizers, then continue
  unwinding.
- A `spawn` outside any explicit scope attaches to the **root scope** that
  implicitly encloses program entry. Every existing program keeps its
  meaning; today's teardown rule in
  [CONCURRENCY-SPAWN-AWAIT](0011-LightweightFibersAndConcurrency.md#spawn-and-await-concurrency-spawn-await)
  becomes the root scope's normal exit.

```osprey
fn both(u1, u2) = scope {
    let a = spawn fetch(u1)
    let b = spawn fetch(u2)
    "${await(a)} and ${await(b)}"
}
```

```osprey-ml
both u1 u2 = scope
    a = spawn (fetch u1)
    b = spawn (fetch u2)
    "${await a} and ${await b}"
```

### Requesting cancellation — [CANCEL-REQUEST]

`cancel(fiber: Fiber<T>) -> Unit` requests cancellation of one fiber and,
transitively, of every fiber in the scopes it created. The request is
asynchronous and idempotent: it returns immediately, a second request is a
no-op, and cancelling an already completed fiber is a no-op. There is no
handle to the entry fiber, so user code cannot cancel program entry.

```osprey
let answer = scope {
    let ticker = spawn forever(channel)
    let result = compute()
    cancel(ticker)
    result
}
```

Without the `cancel`, this scope's normal exit would wait forever for the
infinite ticker — structured concurrency makes the leak visible at the scope
boundary instead of letting the fiber escape.

### Where cancellation lands — [CANCEL-POINTS]

A fiber observes cancellation **only at a suspension point**: `await`,
`send`, `recv`, `sleep`, `yield`, and a `perform` answered by a resuming
handler. These are exactly the operations the effect checker already tracks,
so *where a function can be interrupted is readable from its effect row.*

The consequences are the design's core guarantees:

- **Pure code is uninterruptible by construction.** A critical section is any
  expression whose row contains no suspending operation — a fact the checker
  knows statically. Osprey needs no `mask`, `uninterruptibleMask`, or
  shielding bracket, because there is nothing to mask: interruption cannot
  strike between two pure instructions, only where the code already said it
  could pause. Haskell needed `mask` precisely because its asynchronous
  exceptions may land anywhere (Marlow et al., PLDI 2001); Osprey inverts the
  default.
- **A long pure loop is honestly uninterruptible.** The remedy is explicit
  and already in the language: `yield` inside the loop reintroduces exactly
  one poll point, visible in the row.
- Compiler backends MUST NOT introduce hidden suspension points; a row with
  no suspending operations is a binding promise of atomicity with respect to
  cancellation.

### Delivery: decline to resume — [CANCEL-DELIVERY]

When a cancelled fiber reaches (or is already blocked at) a suspension point,
the runtime MUST NOT deliver a value into it. The pending continuation is
discarded; the fiber unwinds outward through its handler regions and scopes,
running each region's `finally` arms ([CANCEL-FINALLY]) and each scope's
child-cancellation ([CANCEL-SCOPE]); then the fiber completes as cancelled.

```mermaid
stateDiagram-v2
    [*] --> Running: spawn
    Running --> Suspended: await, recv, perform
    Suspended --> Running: value resumed
    Suspended --> Finalizing: cancelled
    Running --> Done: block value
    Finalizing --> Cancelled: finally arms ran
    Done --> [*]
    Cancelled --> [*]
```

There is no edge from `Running` to `Finalizing`: cancellation never preempts
executing code. And there is no "catch" state: cancellation is **not a value
and not an exception**. No expression can observe it from inside and decide to
keep running; code can only finalize. This makes the Kotlin bug class of a
swallowed `CancellationException` — a fiber that ignores its own cancellation
— unrepresentable rather than discouraged.

### Finalizers — [CANCEL-FINALLY]

A handler region MAY declare one `finally` arm. It takes no parameters,
evaluates to `Unit`, and runs exactly once when the region exits — after a
normal answer, after a non-resuming arm's early exit, or during cancellation
unwinding. Regions finalize innermost first.

```ebnf
handlerExpr ::= "handle" IDENT handlerArm+ finallyArm? "in" expr
finallyArm  ::= "finally" "=>" expr
```

```osprey
handle Db
    query sql => runQuery(pool, sql)
    finally => releasePool(pool)
do report()
```

```osprey-ml
handle Db
    query sql => runQuery pool sql
    finally => releasePool pool
in report ()
```

While finalizers run, the fiber is **shielded**: a further cancellation
request MUST NOT interrupt them, and their own suspension points execute
without observing the pending cancellation. Shielding is one-shot and
bounded to the finalizers; it resumes unwinding when they return. A finalizer
SHOULD NOT spawn and MUST NOT be a place where new long-running work hides —
its scope still owes its parent a prompt exit. This is Koka's deep
finalization (Leijen 2018) fused with Trio-style cleanup shielding.

### Observing completion — [CANCEL-JOIN]

`await` keeps its type, `Fiber<T> -> T`. Awaiting a fiber that was cancelled
cannot produce a `T`, so cancellation **propagates along the await edge**: the
awaiter itself unwinds as cancelled. Results flow up; cancellation flows down
and across await edges. At program entry, awaiting a cancelled fiber runs the
entry finalizers and exits the process with a nonzero status and a one-line
diagnostic naming the fiber's spawn site.

To observe instead of propagate, `join` is the firewall:

```osprey
type Outcome<T> = Done { value: T } | Cancelled
```

`join(fiber: Fiber<T>) -> Outcome<T>` blocks like `await` but returns the
outcome as ordinary data, forcing a `match` — cancellation handled the way
Osprey handles every other expected condition, as a case the compiler checks.

### Deadlines and races — [CANCEL-DEADLINE]

Timeouts and races are derived forms — sugar over a scope plus `cancel`,
with no additional runtime authority:

```ebnf
withinExpr ::= "within" "(" expr ")" blockExpr
```

- `within(ms) { ... } -> Result<T, TimedOut>` runs its block in a new scope
  with a deadline. On expiry the runtime cancels the scope's children and the
  block's pending suspension, waits for finalizers, and the expression
  evaluates to the `TimedOut` error value. A timeout is expected failure —
  ordinary data, so `?:` and `match` apply.
- `race(a: Fiber<T>, b: Fiber<T>) -> T` waits for the first fiber to complete
  `Done` and cancels the other. It is specified over the channel-selection
  primitive of [plan 0007](../plans/0007-fiber-select.md) and MUST define
  deterministic-mode tie-breaking with it.

```osprey
let page = within(500) { fetch(url) } ?: cachedPage

let nearest = scope {
    race(spawn probe(mirrorA), spawn probe(mirrorB))
}
```

```osprey-ml
page = (within 500 (fetch url)) ?: cachedPage

nearest = scope
    race (spawn (probe mirrorA)) (spawn (probe mirrorB))
```

### Kill-safety — [CANCEL-KILLSAFE]

Cancelling a fiber MUST NOT corrupt any abstraction it shares (Flatt &
Findler, PLDI 2004):

- A fiber cancelled while blocked in `send` or `recv` is removed from the
  channel's wait queue without consuming, duplicating, or losing an element.
- A fiber cancelled while suspended in a handler **turn** ([SERIAL-TURN])
  whose arm is already running lets the turn complete; the answer is
  discarded and the fiber then unwinds. A fiber cancelled while still queued
  for a turn is dequeued without the arm running. Handler state never
  witnesses a half-turn.
- Under ARC and GC alike, dropping a continuation MUST release the values it
  captured; a declined resume is a normal end of ownership, not a leak.

## Part 2 — Turn isolation: reentrancy and serialization

### Research basis — [SERIAL-RESEARCH]

| Decision | Source |
| --- | --- |
| Shared state lives behind a handler; access is serialized turns | E-language vats and turns (Miller, Tribble & Shapiro 2005); actor one-message-at-a-time (Hewitt; Erlang; Orleans, Bykov et al. 2011); already shipped as [EFFECTS-FIBER-PERFORM](0017-AlgebraicEffects.md#resuming-handlers) |
| Reentrancy is a static discipline, not a runtime timeout | Orleans deadlocks on grain call cycles at runtime; Osprey's closed-program effect summaries can reject the cycle at compile time |
| Reentrant operations are asynchronous self-sends only | Erlang's send-to-self; E's eventual sends |
| Multi-operation composition is transactional, with `retry` | Composable memory transactions (Harris, Marlow, Peyton Jones & Herlihy, PPoPP 2005) |
| Serialization implemented by combining, not mutual exclusion | Flat combining (Hendler, Incze, Shavit & Tzafrir, SPAA 2010); MCS queue locks (Mellor-Crummey & Scott 1991) as the fair fallback; Reagents (Turon, PLDI 2012) for lock-free composition |

### The handler is the monitor — [SERIAL-TURN]

Osprey already forbids free shared mutation: handler-owned state is the
sanctioned form ([EFFECTS-HANDLER-STATE](0017-AlgebraicEffects.md#handler-owned-state)),
assignable only inside arms, and concurrent performs into one resuming
handler already serialize for the full round trip
([EFFECTS-FIBER-PERFORM](0017-AlgebraicEffects.md#resuming-handlers)). This
section names that shipped behavior and makes it law.

A **turn** is one perform's complete arm evaluation, from operation entry to
its answer. For each handler region:

- Turns MUST be totally ordered; two turns of one region never interleave.
- Handler state MUST change only inside a turn; between turns it is
  quiescent.
- A turn MUST run to completion regardless of its performer's cancellation
  ([CANCEL-KILLSAFE]).

Data races on handler state are therefore impossible *by construction*, not
by locking discipline — the region is a monitor the programmer never
declares, an E-style vat expressed as a handler. Channels remain the data
plane; turns are the control plane.

```mermaid
sequenceDiagram
    participant A as Fiber A
    participant H as Account handler
    participant B as Fiber B
    A->>H: perform debit
    activate H
    B--)H: perform credit (queued)
    H-->>A: answer
    deactivate H
    activate H
    H-->>B: answer
    deactivate H
```

### Widen the operation, not the lock — [SERIAL-WIDEN]

One turn is atomic; two turns are not. Check-then-act across two performs is
the classic plan-interference hazard (Miller et al. 2005):

```osprey
let bal = perform Account.balance(from)     // turn 1
perform Account.debit(from, amount)         // turn 2 — bal may be stale
```

The first remedy is design, not synchronization: **make the invariant an
operation.** `Account.transfer(from, to, amount)` is one turn, and one turn
is already atomic. Reaching for `atomic` ([SERIAL-ATOMIC]) or a lock
([SERIAL-FALLBACK]) before widening the operation is a design smell this
specification names so reviews can cite it.

### Static reentrancy discipline — [SERIAL-REENTRANCY]

The shipped checker already rejects an arm performing its own active
operation ([Handlers](0017-AlgebraicEffects.md#handlers)). This section
extends that rule from one operation to the **turn graph**: nodes are handler
regions with state or resuming arms; an edge runs from region `R` to region
`S` when any arm of `R` can reach a perform answered by `S` (through helpers
and lambdas, via the same closed-program fixed point that powers effect-row
checking today).

- A cycle in the turn graph is a compile error naming the cycle, because at
  runtime it is a turn waiting for itself — the deadlock Orleans detects
  with timeouts, rejected here before anything runs.
- The escape hatch is declared, not implicit. An operation marked
  `reentrant` MUST return `Unit`, and performing it inside an active turn of
  its own region enqueues a **new** turn instead of nesting — an
  asynchronous self-send in the Erlang and E tradition. `reentrant` edges do
  not close cycles, because they do not wait.

```osprey
effect Mailbox {
    reentrant post: fn(string) -> Unit
    drain: fn() -> string
}
```

```osprey-ml
effect Mailbox
    reentrant post : string => Unit
    drain : Unit => string
```

### Transactional turns — [SERIAL-ATOMIC]

When the invariant genuinely spans operations that cannot be widened —
different effects, a library-owned handler — `atomic` composes several
performs into one logical turn:

```ebnf
atomicExpr ::= "atomic" blockExpr
retryExpr  ::= "retry"
```

```osprey
let receipt = atomic {
    let bal = perform Account.balance(from)
    if bal < amount then retry
    else {
        perform Account.debit(from, amount)
        perform Vault.credit(to, amount)
        Receipt { moved: amount }
    }
}
```

- The block MUST execute as one turn against every region it touches; no
  other turn of any touched region interleaves.
- `retry`, legal only inside `atomic`, abandons the attempt and blocks until
  the state of some touched region changes, then re-runs the block —
  Harris et al.'s composable blocking, so waiting-for-a-condition needs no
  condition variables. (`orElse` composition is a recorded extension, not
  part of this target.)
- **The effect row is the isolation proof.** The block's row MUST contain
  only turn-safe operations: no `spawn`, `await`, `send`, `recv`, `sleep`,
  and no operation of a handler that performs outside work. A block that
  logs inside `atomic` is a compile error, statically. This is the
  precondition Haskell's STM needs a dedicated monad to enforce and
  mainstream languages cannot enforce at all; Osprey's checker already
  carries the information in the row it infers.
- Since a retried block may run more than once, its body must be repeatable —
  which the row restriction above already guarantees.

### Minimal contention — [SERIAL-CONTENTION]

Serialization is a semantic contract; a mutex around user code is only its
crudest implementation. Backends MUST layer:

1. **No synchronization where none is owed.** A direct-substitution region
   with pure arms and no state has no turns to order; concurrent performs
   proceed with no shared write at all.
2. **Combining, not exclusion, on hot regions.** A stateful region's turn
   queue SHOULD be a flat-combining structure (Hendler et al., SPAA 2010):
   one fiber briefly becomes the combiner and applies queued turns in
   sequence, keeping the state hot in one cache and beating a contended
   lock's cache-line ping-pong under load. The uncontended fast path is a
   single compare-and-swap.
3. **Fair queueing as the general fallback.** Where combining does not
   apply, an MCS-style queue (Mellor-Crummey & Scott 1991) preserves
   ordering with local spinning.

Deterministic mode ([CONCURRENCY-DETERMINISTIC](0011-LightweightFibersAndConcurrency.md#deterministic-execution-concurrency-deterministic))
MUST define one total turn order — spawn order of the performers — so
goldens stay byte-stable.

### The declared fallback — [SERIAL-FALLBACK]

OS mutexes and semaphores remain reachable through the C FFI for code that
interoperates with C libraries that require them. They are the fallback
tier, outside Osprey's safety guarantee like the rest of the FFI
([0019](0019-ForeignFunctionInterface.md)): the compiler cannot see a C
lock, so none of this specification's static guarantees — turn atomicity,
cycle rejection, cancellation shielding — extend across one. Osprey code
SHOULD express serialization as turns and `atomic`, not FFI locks.

## What this replaces

| Elsewhere | In Osprey |
| --- | --- |
| Go: `context.Context` threaded as the first parameter, checked by hand | No parameter; scope owns the fiber, cancellation lands only at row-visible suspension points |
| .NET: `CancellationToken` plumbing plus `ThrowIfCancellationRequested` polls | Same — the token's job is done by the scope, the polls by suspension points |
| Kotlin: cooperative cancellation via a catchable `CancellationException` | Not catchable, only finalizable; the swallowed-cancellation bug cannot be written |
| Haskell: async exceptions land anywhere; correctness depends on `mask` brackets | Pure code is uninterruptible by construction; there is no mask to forget |
| Java/Trio: structured task scopes as a library discipline | `scope` is the language's only way to spawn; the discipline is the grammar |
| Orleans: non-reentrant grains, call-cycle deadlocks detected by runtime timeout | Turn-graph cycles are a compile error; `reentrant` self-sends are declared |
| Locks and STM as libraries with by-convention purity rules | Turns are implicit monitors; `atomic`'s isolation precondition is the checked effect row |

## References

- Nathaniel J. Smith. *Notes on structured concurrency, or: Go statement
  considered harmful.* 2018. <https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/>
- Martin Sústrik. *Structured concurrency.* 2016.
- Daan Leijen. *Structured asynchrony with algebraic effects.* TyDe 2017.
- Daan Leijen. *Algebraic effect handlers with resources and deep
  finalization.* MSR-TR-2018-10, 2018.
- Danel Ahman, Matija Pretnar. *Asynchronous effects.* POPL 2021.
- Simon Marlow, Simon Peyton Jones, Andrew Moran, John Reppy. *Asynchronous
  exceptions in Haskell.* PLDI 2001.
- Matthew Flatt, Robert Bruce Findler. *Kill-safe synchronization
  abstractions.* PLDI 2004.
- Mark S. Miller, E. Dean Tribble, Jonathan Shapiro. *Concurrency among
  strangers: programming in E as plan coordination.* TGC 2005.
- Tim Harris, Simon Marlow, Simon Peyton Jones, Maurice Herlihy. *Composable
  memory transactions.* PPoPP 2005.
- Danny Hendler, Itai Incze, Nir Shavit, Moran Tzafrir. *Flat combining and
  the synchronization-parallelism tradeoff.* SPAA 2010.
- John M. Mellor-Crummey, Michael L. Scott. *Algorithms for scalable
  synchronization on shared-memory multiprocessors.* ACM TOCS 1991.
- Aaron Turon. *Reagents: expressing and composing fine-grained concurrency.*
  PLDI 2012.
- Sergey Bykov, Alan Geller, Gabriel Kliot, James Larus, Ravi Pandya, Jorgen
  Thelin. *Orleans: cloud computing for everyone.* SoCC 2011.
- Stephen Dolan, Spiros Eliopoulos, Daniel Hillerström, Anil Madhavapeddy,
  KC Sivaramakrishnan, Leo White. *Concurrent system programming with effect
  handlers.* TFP 2017.
- KC Sivaramakrishnan, Stephen Dolan, Leo White, Tom Kelly, Sadiq Jaffer,
  Anil Madhavapeddy. *Retrofitting effect handlers onto OCaml.* PLDI 2021.
- Roman Elizarov, Mikhail Belyaev, Marat Akhin, Ilmir Usmanov. *Kotlin
  coroutines: design and implementation.* Onward! 2021.
- Sebastian Burckhardt, Alexandro Baldassin, Daan Leijen. *Concurrent
  programming with revisions and isolation types.* OOPSLA 2010. (Recorded as
  a future direction for fork–join state with deterministic merges.)
