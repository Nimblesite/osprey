# Algebraic Effects

An effect declares typed operations. `perform` invokes the innermost matching
lexical handler, which supplies the operation result. Default and ML syntax
lower to the same effect, perform, handler, and resume AST nodes; their runtime
semantics are identical.

The checker validates declared operations and their value types, infers the
operations required by unannotated functions and callbacks, and propagates
those requirements through calls. A handler discharges only the operation arms
it actually supplies, for the same generic effect instantiation. Every
requirement must be discharged before program entry. A missing handler is
therefore a compile error, never a runtime abort.

## Keywords

```text
effect perform handle in resume
```

## Effect Declarations

```ebnf
effectDecl ::= docComment? "effect" IDENT ("<" typeParamList ">")? "{" opDecl* "}"
opDecl     ::= IDENT ":" fnType
```

`[EFFECTS-OP-TYPING]` Every operation named by `perform` or a handler arm must
belong to the declared effect. Perform arguments and handler parameters must
match the operation's positional arity; performed values must also match its
types. Named arguments are not supported on `perform`.

```osprey
effect State {
    get: fn() -> int
    set: fn(int) -> Unit
}
```

```osprey-ml
effect State
    get : Unit => int
    set : int => Unit
```

## Generic Effects

`[EFFECTS-GENERIC-DECL]` An effect may declare type parameters, including
`in` and `out` variance. The checker validates operation parameters as input
positions and operation results as output positions.

```osprey
effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
```

`[EFFECTS-GENERIC-INSTANTIATION]` Each handler site instantiates a generic
effect independently. Handler arm values and performs in the handled body must
agree on that instantiation. This handler instantiates `Stash<string>`:

```osprey
let word = handle Stash
    put value => print(value)
    take => "ready"
do perform Stash.take()
```

`[EFFECTS-GENERIC-RUNTIME]` Generic operation payloads use an erased machine-word
ABI. Code generation boxes and unboxes values using the type inferred at each
site. Static discharge distinguishes resolved instantiations, so a
`Stash<string>` handler does not discharge `Stash<int>.put`. Runtime handler
keys also include the resolved instantiation, such as `Stash$string`; their
null-lookup guard is a defensive backstop and must not be the normal rejection
path for a checked program. Monomorphic effects use their declared name as the
key.

## Effectful Function Types

An effect row follows the return type. It contains one effect reference or a
bracketed list; generic references may include type arguments.

```ebnf
effectSet ::= "!" effectRef | "!" "[" effectRef ("," effectRef)* "]"
effectRef ::= IDENT ("<" typeList ">")?
```

```osprey
fn read() -> string !IO = perform IO.readLine()
fn fetch(url) -> string ![IO, Net] = perform Net.get(url)
```

```osprey-ml
read : Unit -> string !IO
read () = perform IO.readLine

fetch : string -> string ![IO, Net]
fetch url = perform Net.get url
```

`[EFFECTS-GENERIC-ROWS]` A row entry such as `!Stash<int>` pins the generic
effect instantiation used by performs in that function body. A bare generic
entry leaves its arguments to inference.

`[EFFECTS-STATIC-DISCHARGE]` Effect annotations are checked contracts, not
handlers. Writing `!Logger` declares which effect the function body may require;
it does not authorize `Logger.log` at the call site and does not discharge that
operation. The checker rejects an operation outside a non-empty declared row.
It infers requirements when annotations are omitted, propagates them through
named calls and higher-order callback calls, and requires the selected program
entry (`main` when present, otherwise the top-level executable statements) to
have no remaining operation requirements.

Discharge is operation- and instantiation-specific. A handler for `Pair.first`
does not discharge `Pair.second`, and a handler inferred as `Stash<string>` does
not discharge `Stash<int>.put`. Complementary nested partial handlers may each
discharge the operation they cover. Constructing a lambda is pure, but invoking
it contributes its latent requirements; constructing one inside a handler does
not give it authority after it escapes that handler's lexical region.

The current compiler realizes these rules with a closed-program operation
summary and fixed-point call analysis. Explicit open effect-row variables are
not surface syntax and effect rows are not yet exposed as independently
quantified values in the Hindley–Milner type representation.

## Performing Operations

```ebnf
performExpr ::= "perform" IDENT "." IDENT "(" args? ")"
```

```osprey
fn increment() -> int !State = {
    let current = perform State.get()
    perform State.set(current + 1)
    perform State.get()
}
```

The operation result is the value returned by its active handler arm. The
static effect-row check guarantees that a matching handler exists on every
execution path.

## Handlers

```ebnf
handlerExpr ::= "handle" IDENT handlerArm+ "in" expr
handlerArm  ::= IDENT IDENT* "=>" expr
```

A handler with no `resume` expression uses direct value substitution: the arm
returns the operation result and execution continues after `perform`.

```osprey
let result = handle State
    get => 41
    set value => print("set ${value}")
do increment()
```

Lookup is per effect and operation. Nested handlers may override selected
operations; the innermost matching arm wins and an outer arm remains available
for operations not handled by the inner region.

```osprey
handle Logger
    log message => print("outer: ${message}")
do handle Logger
    log message => print("inner: ${message}")
do perform Logger.log("test")
```

A handler arm is not permission to perform its own active operation
recursively. The checker rejects a perform with the same effect, resolved
generic instantiation, and operation as the active arm. A different operation
not covered by a partial handler, or a different generic instantiation, may
instead be discharged by an enclosing matching handler. Every remaining arm
requirement follows the ordinary entry-discharge rule.

## Handler-Owned State

`[EFFECTS-HANDLER-STATE]` A handler arm may capture a mutable binding. Code
generation promotes the captured binding to a shared heap cell, so every arm,
the handled body, and code after the region observe the same location. This is
the **sanctioned form of mutation** in Osprey: a `mut` cell is meant to change
*through* an effect handler like the one below, not by free imperative
reassignment in ordinary statement position (see
[Bindings](0003-Syntax.md#bindings)). The checker enforces this boundary:
assignment to a mutable binding outside a handler arm is a type error.

```osprey
mut cell = 0
let result = handle State
    get => cell
    set value => { cell = value }
do increment()
print("result=${result} cell=${cell}")
```

Handler state is also preserved when a perform crosses a spawned-fiber or HTTP
callback boundary. The native conformance cases are
`tests/regressions/effects/fiber_effects.test.osp` and
`tests/regressions/effects/http_state_levels.test.osp`.

## Resuming Handlers

`[EFFECTS-RESUME]` `resume(value)` supplies the current operation result and
runs the rest of the handled computation. It evaluates to that computation's
answer, so the arm may execute code after the resumed computation returns.
`resume()` supplies `Unit`.

```ebnf
resumeExpr ::= "resume" "(" expr? ")"
```

```osprey
effect Ask { value: fn() -> int }

let answer = handle Ask
    value => {
        let completed = resume(21)
        print("completed=${completed}")
        completed
    }
do perform Ask.value() * 2
```

Resuming handlers have these rules:

- They are deep: the same handler remains installed while the continuation
  runs.
- They are single-shot. A second resume of one continuation aborts with
  `fatal: continuation already resumed (multi-shot resume is not supported)`.
- Handler mode is selected per arm, not per region. An arm containing no
  `resume` supplies its operation result directly and the caller continues,
  whatever its siblings do. In an arm that does contain `resume`, returning
  from the selected branch without resuming stops the suspended computation and
  its value becomes the result of the whole handler; a single arm may
  intentionally resume its success branch and return from its error branch,
  which is the exception-style early-exit pattern. Adding `resume` to one arm
  therefore leaves every sibling arm's mode untouched — reading the mode
  region-wide, so that a sibling's `resume` silently converted a substituting
  arm into an early exit, was
  [issue #177](https://github.com/Nimblesite/osprey/issues/177).
- `resume` is lexical to the arm. It is rejected at top level and inside a
  lambda declared in an arm, because that lambda has no live arm continuation.
- Explicit resume is native-only. WebAssembly supports direct value-substitution
  handlers but not the pthread-backed continuation runtime.

`[EFFECTS-RESUME-NESTING]` A continuation reaches from its `perform` out to the
handler that answers it, so it CONTAINS every arm suspended in between. An
operation that crosses an inner region to reach an outer one therefore has its
arm installed OUTSIDE the inner arm that was live, and `resume` puts that inner
arm back inside — so the inner arm's post-resume code always settles before the
outer arm's. Settlement is reverse order of the live arm frames, which equals
reverse order of entry only while no operation crosses a region.

```osprey
effect Alpha { alpha: fn(string) -> int }
effect Beta { beta: fn(string) -> int }

mut settled = ""
let total = handle Alpha
    alpha label => {
        let answer = resume(10)
        settled = "${settled}a:${label}|"
        answer
    }
do handle Beta
    beta label => {
        let answer = resume(100)
        settled = "${settled}b:${label}|"
        answer
    }
do {
    let p = perform Alpha.alpha("a1")
    let q = perform Beta.beta("b1")
    let r = perform Alpha.alpha("a2")
    let s = perform Beta.beta("b2")
    (p + q) + (r + s)
}
print("${settled}")
```

Entry order is `a1 b1 a2 b2`, but `a2` crosses the Beta region while `b1` is
still live, so `a2`'s arm sits outside `b1`'s and `settled` is
`b:b2|b:b1|a:a2|a:a1|` — NOT the `b:b2|a:a2|b:b1|a:a1|` that reversing the entry
order would give. The conformance case is
`tests/effects/resume/resume_lifo_audit.test.osp`, which pins the crossing
orders at two, three and four depths and with a partial inner region.

`[EFFECTS-HANDLER-ARMS]` An arm's value is checked against whichever of the two
things it actually supplies, which follows from that ARM's own mode:

- The arm contains no `resume`: its value substitutes for its operation's
  declared result, and the handled expression's own value is the region's
  result. A sibling arm's `resume` does not change this.
- The arm contains `resume`: the operation's result was already supplied by
  `resume`, and the arm runs on afterwards, so the arm's value is the region's
  ANSWER and that is what it is checked against. The same holds for a branch of
  such an arm that returns without resuming: it abandons the continuation, the
  operation's result is never produced — the `perform` waiting for it never
  returns — and the branch's value answers for the whole `handle`.

Disagreement in the second case is a type error naming both types:

```text
handler arm `Mixed.b` resumes, so its value becomes the whole `handle`
expression's result — but it is `string` and that result is `int`. Make the
arm's value agree with the handled expression's type
```

The conformance cases are
`examples/failscompilation/effect_arm_answer_type_mismatch.ospo` and its ML twin
`ml_effect_arm_answer_type_mismatch.ospo`, which cover both directions and a
`Result` answer; `tests/regressions/effects/abort_vs_resume.test.osp` holds the
accepted counterparts.

The rule lives in inference rather than code generation because it needs the
source types. The runtime shape descriptor
([TYPE-ANY](0004-TypeSystem.md#the-any-type--type-any)) has since removed the
word-level conflation — an erased `any` now reaches code generation as its own
boxed representation — but the rule stays in inference: it is about source
types, not representation.

### Known limits of abandoning a region

Abandoning a region ends the suspended computation with `pthread_exit`, and a
killed thread runs no epilogue. Two consequences are unresolved. Both predate
the operation mailbox and neither is reachable with scalar operands, which is
why `tests/regressions/effects/abort_vs_resume.test.osp` passes the ARC leak
oracle: its operands are integers.

**Heap operands owned by the killed frames are not reclaimed.** The mailbox's
own reference is retired correctly — the dispatcher frees it, and a performer
killed before the handoff releases what it took ([EFFECTS-OPERATION-MAILBOX]) —
but the performing frame's *own* reference, the one an ordinary return would
drop, is abandoned with the stack. Under `--memory=arc` with `OSPREY_ARC_DEBUG=1`
this program reports one live object at exit, the six bytes of `alpha`:

```osprey
effect Label { tag: fn(string) -> string }

fn ask(subject) !Label = perform Label.tag(subject)

let answer = handle Label
    tag subject => match subject == "alpha" {
        true  => "stopped at ${subject}"
        false => resume("saw ${subject}")
    }
do ask("al" + "pha")
```

Reclaiming them needs generated cleanup along the abort path — unwinding — not a
release the runtime could issue, because the owning slots are `alloca`s in every
frame on the killed stack.

**Abandoning a region whose body is awaiting a spawned fiber deadlocks.**
`__osprey_coro_abort` joins the body thread, and a body blocked in `await` of a
fiber the same abort has just killed inside its own `perform` never returns:

```osprey
fn pair(a, b) !Label = {
    let f1 = spawn ask(a)
    let f2 = spawn ask(b)
    await(f1) + await(f2)
}
```

Resolving it means deciding what `await` of an abandoned fiber yields, which is
the same cancellation question as [issue #177](https://github.com/Nimblesite/osprey/issues/177).
Until then, do not `await` inside a region whose arms can abandon it.

Native resume uses one suspended pthread stack as the continuation. Regions
whose arms contain no `resume` stay on the direct handler-call path.

`[EFFECTS-OPERATION-MAILBOX]` A resumable operation's arguments cross into the
handler in a **mailbox** allocated per suspension: a word array sized by the
operation's real arity, a parallel array of operand kinds, and that arity. The
mailbox carries no fixed capacity, so an operation of any declared arity
delivers every argument it was given.

Each slot's kind says whether its word is a managed pointer or a bare scalar,
and the mailbox **owns** the managed ones: the performer transfers a reference
when it suspends, and retiring the mailbox releases exactly those slots. A
handler arm therefore borrows its operands for the whole time it can reach them
— including after a `resume` returns, when the performer's own frame may already
be gone — and an operand can neither be freed early nor outlive its perform.

The dispatcher *takes* the mailbox before reading it, so an arm that resumes can
let the body perform again: the nested suspension installs its own mailbox
instead of overwriting one still in use. Reading a slot the operation never sent
is a compiler bug, not a recoverable condition, and aborts rather than answering
zero.

Three critical implementation defects previously limited operation values; all
three are fixed and locked by paired Default/ML cases under `tests/effects`:

- ~~[issue #182](https://github.com/Nimblesite/osprey/issues/182): the native
  resumable-operation mailbox transports 16 arguments; the compiler accepts a
  17th but the runtime silently delivers zero for it.~~ **Fixed** by the
  length-carrying mailbox above.
- ~~[issue #183](https://github.com/Nimblesite/osprey/issues/183): a direct
  handler corrupts an operation result whose type is `Result<T, E>`.~~ **Fixed.**
  A direct handler transports a complete `Result<T, E>` operation value in both
  flavors and under all three memory backends.
- ~~[issue #185](https://github.com/Nimblesite/osprey/issues/185): under ARC, a
  resuming handler leaks one managed object when its completed continuation
  answer is a dynamic string.~~ **Fixed** by the kind-tagged mailbox above,
  together with registering the continuation answer as owned at the `resume`
  site — the one effect boundary that received an owned value and never claimed
  it.

Coverage: `tests/effects/errors/direct_recovery.test.{osp,ospml}` case 10 for
the whole-`Result` operation value, and
`tests/effects/resume/resume_error_policies.test.{osp,ospml}` for the managed
continuation answer, the sixteen- and seventeen-argument boundaries, and nine
managed with nine scalar operands crossing one operation. Each ran as a
self-passing `Skip` before it was made to assert. The ARC exit audit in
`crates/run_test_corpus.sh` is what proves the release half — the value
assertions pass either way.

`[EFFECTS-FIBER-PERFORM]` Concurrent performs into one resuming handler are
serialized for the full suspend-to-resume round trip. This prevents arguments
or results from being delivered to the wrong performer. This shipped
round-trip serialization is also the seed of the **turn** model — handler
regions as implicit monitors, static reentrancy checking, and transactional
composition — specified as a normative target in
[Structured Concurrency](0036-StructuredConcurrency.md#the-handler-is-the-monitor--serial-turn).
