# Algebraic Effects

Osprey treats effects as first-class language features. An effect declares a set of operations; functions list the effects they may perform; handlers give meaning to operations. The compiler rejects any program that performs an unhandled effect.

> **Flavor layer — shared core (AST and above).**  Effect semantics live entirely at and above the canonical AST and are flavor-blind: an effect declaration is `Stmt::Effect` (carrying `EffectOperation`s), `perform IDENT.op(...)` lowers to `Expr::Perform`, a `handle` region lowers to `Expr::Handler{effect, arms, body}` (arms are `HandlerArm`), and `resume(v)` lowers to `Expr::Resume`. The `handle ... in expr` spelling below is the Default-flavor (`.osp`) surface; the ML flavor writes `handle ... do expr` ([FLAVOR-ML-EFFECT]/[FLAVOR-ML-HANDLER] in [ML Flavor Syntax](0024-MLFlavorSyntax.md)) and lowers to the *same* `Handler` node — type inference, the static unhandled-effect check, and the thread-as-continuation runtime never learn which flavor produced the program ([FLAVOR-BOUNDARY] in [Language Flavors](0023-LanguageFlavors.md)). NOTE: first-class handler *values*, a `Handler E` type, and multi-install are a **deferred** Phase-0 shared-core addition ([FLAVOR-HANDLER-VALUE] in 0023) — not in the AST today; ML effect/handler syntax errors loudly until that lands, so treat ML effects as not yet working.

## Status

Effect declarations, `perform` expressions, effect annotations on function types, handler parsing, and full compile-time unhandled-effect checking are implemented. A handler arm may resume the performer in two ways:

- **Implicit tail-resume.** An arm whose body is an ordinary expression returns that value to the `perform` site, which continues. This is the cheap default and handlers may own mutable state with it (see [Handler-Owned State]).
- **Explicit `resume`.** An arm whose body contains a `resume` expression captures the performer's *delimited continuation*: `resume(v)` runs the rest of the handled computation with `v` as the operation's result and yields its answer back to the arm, so the arm can run code **after** the performer continues. Single-shot (each continuation is resumed at most once) and **deep** (the handler stays installed for the resumed computation). See [Resuming Handlers]. **Status: executable for single-shot deep continuations via the thread-as-continuation runtime in [plan 0008](../plans/0008-algebraic-effects-resume.md).**

Multi-shot resume (resuming one continuation more than once) remains a follow-up.

## Keywords

```
effect perform handle in resume
```

## Effect Declarations

```ebnf
effectDecl ::= docComment? "effect" IDENT ("<" typeParamList ">")? "{" opDecl* "}"
opDecl     ::= IDENT ":" fnType
```

`typeParamList` is the shared production from [Syntax](0003-Syntax.md#type-declarations)
— effects accept type parameters (with variance) exactly as type declarations do.

```osprey
effect State {
    get : fn() -> int
    set : fn(int) -> unit
}
```

```osprey-ml
effect State
    get : Unit => int
    set : int => unit
```

### Generic Effects

`[EFFECTS-GENERIC-DECL]` **Effects accept type arguments for full
polymorphism.** `effect State<T>` binds `T` across every operation signature;
one declaration serves every instantiation. Type parameters may carry variance
(`effect Ask<out T>`, `effect Emit<in T>`), position-checked against the
operation signatures: operation parameters are input positions, operation
results output positions ([TYPE-VARIANCE-POSITIONS](0004-TypeSystem.md#generics-and-variance)).

```osprey
effect Stash<T> {
    put  : fn(T) -> Unit
    take : fn() -> T
}
```

```osprey-ml
effect Stash T
    put : T => Unit
    take : Unit => T
```

`[EFFECTS-GENERIC-INSTANTIATION]` **Each `handle` site and each effect-row
entry instantiates the effect independently.** A handler's arms pin its
instantiation: a non-resuming arm's value substitutes for the operation's
result, so `take => "stash-words"` pins `Stash<string>` (an arm handling a
`Unit`-resulted operation may yield anything — the value is discarded). Inside
the handled body, `perform` sites resolve against the innermost enclosing
instantiation of their effect, matching the runtime's innermost-wins handler
stack; inside a function, the declared effect row provides the instantiation.
Two instantiations of one effect coexist in one program:

```osprey
let words = handle Stash
    take => "stash-words"
    put v => print("put: ${v}")
in relabel()
let count = handle Stash
    take => 41
    put v => print("put: ${v}")
in bumped()
```

`[EFFECTS-GENERIC-ROWS]` **Effect rows carry type arguments.** A row entry
`!State<int>` (or `![State<int>, Log]`) pins the instantiation the function's
`perform` sites check against; performing an operation with arguments that
contradict the row is a compile error. A bare row entry on a generic effect
leaves the instantiation to inference. The row is the function's declared
interface to its dynamically-scoped handler: the checker verifies each side
against its declared instantiation independently (it does not globally prove
that every runtime handler matches every performer's row) — the runtime's
instantiation-keyed dispatch ([EFFECTS-GENERIC-RUNTIME]) is what turns a
handler/row mismatch into a loud unhandled-effect abort instead of undefined
behaviour.

```osprey
fn bumped() -> int !Stash<int> = {
    let n = perform Stash.take()
    n + 1
}
```

```osprey-ml
bumped : Unit -> int ! Stash<int>
bumped () =
    n = perform Stash.take ()
    n + 1
```

`[EFFECTS-GENERIC-RUNTIME]` **Instantiations are erased in the ABI and keyed
in dispatch.** A generic effect's operations keep ONE ABI program-wide: every
type-parameter-mentioning slot travels as a uniform boxed machine word, boxed
at `perform` sites and unboxed at handler-arm entry (and inversely for
results) against the signature inference resolved per site. Handlers
register on the runtime stack under their RESOLVED instantiation
(`Stash$int`), and `perform` sites look up under theirs — so a handler only
satisfies performs of the same instantiation, and the innermost
same-instantiation handler wins. A mismatch (or an instantiation the checker
could not resolve at a site) misses the lookup and aborts loudly with
`unhandled effect: …`, never confusing values of different types.
Monomorphic effects keep their bare names — their programs compile
byte-identically to before. The C runtime treats keys as opaque strings and
needs no changes.

## Effectful Function Types

A function declares the effects it may perform with `!E` after its return type. `E` is either a single effect reference or a bracketed set; each reference may apply type arguments to a generic effect (`!State<int>`, `![State<int>, Log]` — [EFFECTS-GENERIC-ROWS](#generic-effects)).

```ebnf
effectSet  ::= "!" effectRef | "!" "[" effectRef ("," effectRef)* "]"
effectRef  ::= IDENT ("<" typeList ">")?
```

```osprey
fn read() -> string !IO = perform IO.readLine()
fn fetch(url: string) -> string ![IO, Net] = ...
```

```osprey-ml
read : Unit -> string !IO
read () = perform IO.readLine

fetch : string -> string ![IO, Net]
fetch url = perform Net.get url
```

A function with no `!E` is pure; calling an effectful function from a pure context is a compilation error.

## Performing Operations

```ebnf
performExpr ::= "perform" IDENT "." IDENT "(" args? ")"
```

```osprey
fn incrementTwice() -> int !State = {
    let current = perform State.get()
    perform State.set(current + 1)
    perform State.get()
}
```

```osprey-ml
incrementTwice : Unit -> int !State
incrementTwice () =
    current = perform State.get
    perform State.set (current + 1)
    perform State.get
```

If no enclosing handler covers an effect, the program does not compile.

## Handlers

```ebnf
handlerExpr ::= "handle" IDENT handlerArm+ "in" expr
handlerArm  ::= IDENT paramList? "=>" expr
```

```osprey
handle State
    get        => 42
    set newVal => print("set to " + toString(newVal))
in
    incrementTwice()
```

```osprey-ml
handle State
    get => 42
    set newVal => print ("set to " + toString newVal)
in
    incrementTwice ()
```

The innermost matching handler wins for each effect. Handlers may be nested freely:

```osprey
handle Logger
    log msg => print("[OUTER] " + msg)
in
    handle Logger
        log msg => print("[INNER] " + msg)
    in
        perform Logger.log("test")    // prints "[INNER] test"
```

```osprey-ml
handle Logger
    log msg => print ("[OUTER] " + msg)
in
    handle Logger
        log msg => print ("[INNER] " + msg)
    in
        perform Logger.log "test"    // prints "[INNER] test"
```

## Handler-Owned State

`[EFFECTS-HANDLER-STATE]` A handler arm may read and update a mutable binding
from the enclosing scope. Any `mut` an arm captures is promoted to a shared
heap cell that the whole `handle` region — every arm and the code after `in` —
sees as one location. This makes the `State` effect *real*: the effectful code
stays pure (it only `perform`s), and the handler is the single place the state
lives.

```osprey
effect State { get: fn() -> int  set: fn(int) -> Unit }

fn bump() -> int !State = {
    let a = perform State.get()
    perform State.set(a + 1)
    perform State.get()
}

mut cell = 0
let r = handle State
    get        => cell           // reads the shared cell
    set newVal => { cell = newVal }   // writes the shared cell
in bump()
print("r=${toString(r)} cell=${toString(cell)}")   // r=1 cell=1
```

```osprey-ml
effect State
    get : Unit => int
    set : int => Unit

bump : Unit -> int !State
bump () =
    a = perform State.get
    perform State.set (a + 1)
    perform State.get

mut cell = 0
r = handle State
    get => cell                      // reads the shared cell
    set newVal =>
        cell := newVal               // writes the shared cell
in
    bump ()
print "r=${toString r} cell=${toString cell}"   // r=1 cell=1
```

The cell is shared across the C HTTP-callback boundary (a request handler's
`perform` resolves to the active handler) and across fiber boundaries (an effect
performed inside a `spawn`ed fiber is handled in the spawner), so one effect
handler can own the state for a whole running server. See
`examples/tested/effects/http_state_levels.osp` and
`examples/statefulhttp/`.

## Resuming Handlers

> **Status — executable for single-shot deep continuations.** Explicit `resume`
> is parsed, type-checked, and lowered through a thread-as-continuation runtime.
> The worked example below is covered by the CLI regression test
> `explicit_resume_runs_the_performer_continuation`.

`[EFFECTS-RESUME]` A handler arm may name the performer's continuation with
`resume`. `resume(v)` resumes the suspended `perform` with `v` as the operation's
result, runs the rest of the handled computation, and evaluates to **that
computation's answer** — so the arm can run code *after* the performer has moved
on. `resume()` with no argument resumes with `Unit`.

```ebnf
resumeExpr ::= "resume" "(" expr? ")"
```

Semantics:

- **Deep.** The handler stays installed for the resumed computation: if the
  continuation performs the effect again, the same arm runs again.
- **Single-shot.** Each continuation is resumed at most once. Multi-shot resume
  remains a follow-up.
- **Abort.** An arm that returns *without* resuming discards the continuation;
  its value becomes the result of the whole `handle … in` — the basis for
  exceptions and early exit.
- An arm whose body is a plain value (no `resume`) is the implicit tail-resume of
  [Handler-Owned State]; the two styles coexist per effect.

```osprey
effect Audit { step: fn(string) -> int }

fn pipeline() -> int !Audit = {
    let a = perform Audit.step("load")     // suspends here
    let b = perform Audit.step("parse")    // …and here
    a + b
}

mut n = 0
let total = handle Audit
    step label => {
        n = n + 1
        let answer = resume(n)          // performer continues with n
        print("after ${label}: answer=${toString(answer)}")
        answer                          // code AFTER resume — impossible with tail-resume
    }
in pipeline()
print("total=${toString(total)}")
```

```osprey-ml
effect Audit
    step : string => int

pipeline : Unit -> int !Audit
pipeline () =
    a = perform Audit.step "load"     // suspends here
    b = perform Audit.step "parse"    // …and here
    a + b

mut n = 0
total = handle Audit
    step label =>
        n := n + 1
        answer = resume n               // performer continues with n
        print "after ${label}: answer=${toString answer}"
        answer                          // code AFTER resume — impossible with tail-resume
in
    pipeline ()
print "total=${toString total}"
```

Output — the "after" lines unwind **LIFO** as each continuation completes, the
signature of a real delimited continuation:

```
after parse: answer=3
after load: answer=3
total=3
```

### Runtime model

`resume` is implemented as **thread-as-continuation**
(single-shot, deep) ([plan 0008](../plans/0008-algebraic-effects-resume.md)): a
`handle` region whose arms mention `resume` runs its `in` body on a spawned body
thread while the host thread runs the arms; `perform` suspends the body thread and
yields the operation to the host, and `resume` switches back, delivering the
value. The suspended thread *is* the captured continuation, which is why it is
single-shot (a live stack cannot be cloned). Regions with no `resume` keep the
zero-overhead function-call path. The body thread inherits the host's handler
stack via the existing snapshot/restore (`__osprey_handler_snapshot`), so a
`perform` deep inside the continuation still resolves outer handlers. See
[plan 0008](../plans/0008-algebraic-effects-resume.md).

## Effect Inference

The compiler infers the minimal effect set of every expression. Functions either declare their effects or are required to be pure. A function may be polymorphic over an effect set:

```osprey
fn loggedCalculation<E>(x) -> int !E = {
    perform Logger.log("calculating")     // E must include Logger
    x * 2
}
```

```osprey-ml
loggedCalculation : int -> int !E
loggedCalculation x =
    perform Logger.log "calculating"      // E must include Logger
    x * 2
```

## Static Safety Checks

The compiler enforces three static checks on effect programs. Each failure is a compile-time error, not a runtime fault.

| Check                              | Failure mode in other languages |
| ---------------------------------- | ------------------------------- |
| Every `perform` has a handler      | Runtime crash / unhandled exn   |
| No circular effect dependency      | Stack overflow                  |
| No handler that performs the same effect it handles | Infinite loop |

### Circular Dependency Example

```osprey
effect StateA { getFromB: fn() -> int }
effect StateB { getFromA: fn() -> int }

fn circularA() -> int !StateA = perform StateA.getFromB()
fn circularB() -> int !StateB = perform StateB.getFromA()

handle StateA
    getFromB => circularB()       // ❌ circular dependency
in
    handle StateB
        getFromA => circularA()   // ❌ circular dependency
    in
        circularA()
```

```osprey-ml
effect StateA
    getFromB : Unit => int

effect StateB
    getFromA : Unit => int

circularA : Unit -> int !StateA
circularA () = perform StateA.getFromB

circularB : Unit -> int !StateB
circularB () = perform StateB.getFromA

handle StateA
    getFromB => circularB ()       // ❌ circular dependency
in
    handle StateB
        getFromA => circularA ()   // ❌ circular dependency
    in
        circularA ()
```

### Handler-Self-Recursion Example

```osprey
effect Counter { increment: fn(int) -> int }

fn performIncrement(n: int) -> int !Counter = perform Counter.increment(n)

handle Counter
    increment n => performIncrement(n + 1)   // ❌ handler performs the effect it handles
in
    performIncrement(5)
```

## Worked Example

`x * 2` returns `Result<int, MathError>`; the function below performs `Exception` on overflow and `State` to record the success.

```osprey
effect Exception { raise: fn(string) -> unit }
effect State     { get: fn() -> int, set: fn(int) -> unit }

fn doubleAndStore(x) -> int ![Exception, State] = match x * 2 {
    Success { value }   => {
        perform State.set(value)
        value
    }
    Error   { message } => {
        perform Exception.raise(message)
        0
    }
}

handle Exception
    raise msg => { print("error: " + msg); -1 }
in
    handle State
        get        => 0
        set newVal => print("state: " + toString(newVal))
    in
        let result = doubleAndStore(21)
        print("result: " + toString(result))
```
