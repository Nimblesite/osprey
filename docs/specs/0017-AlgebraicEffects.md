# Algebraic Effects

An effect declares typed operations. `perform` invokes the innermost matching
lexical handler, which supplies the operation result. Default and ML syntax
lower to the same effect, perform, handler, and resume AST nodes; their runtime
semantics are identical.

The checker validates declared operations and their value types. Effect rows
currently constrain operations inside the annotated function body, but are not
stored in function types or propagated through calls. A missing handler can
therefore compile; the generated program aborts with
`unhandled effect: <Effect>.<operation>` when lookup fails.

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
belong to the declared effect. Positional argument count and value types must
match the operation signature. Named arguments are not supported on `perform`.

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
agree on that instantiation. For example, this handler is `Stash<string>`:

```osprey
let word = handle Stash
    put value => print(value)
    take => "ready"
in perform Stash.take()
```

`[EFFECTS-GENERIC-RUNTIME]` Generic operation payloads use an erased machine-word
ABI. Code generation boxes and unboxes values using the type inferred at each
site. Runtime handler keys include the resolved instantiation, such as
`Stash$string`, so a mismatched instantiation misses lookup and aborts instead
of calling a handler with the wrong representation. Monomorphic effects use
their declared name as the key.

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

Rows do not yet form part of `Type::Fun`. The checker does not propagate them
through calls, prove that a caller installs every required handler, or require
an unannotated function to be pure.

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

The operation result is the value returned by its active handler arm. If no
handler exists for that effect and operation, runtime lookup prints the
unhandled-effect message and exits nonzero.

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
in increment()
```

Lookup is per effect and operation. Nested handlers may override selected
operations; the innermost matching arm wins and an outer arm remains available
for operations not handled by the inner region.

```osprey
handle Logger
    log message => print("outer: ${message}")
in handle Logger
    log message => print("inner: ${message}")
in perform Logger.log("test")
```

## Handler-Owned State

`[EFFECTS-HANDLER-STATE]` A handler arm may capture a mutable binding. Code
generation promotes the captured binding to a shared heap cell, so every arm,
the handled body, and code after the region observe the same location.

```osprey
mut cell = 0
let result = handle State
    get => cell
    set value => { cell = value }
in increment()
print("result=${result} cell=${cell}")
```

Handler state is also preserved when a perform crosses a spawned-fiber or HTTP
callback boundary. The native conformance cases are
`examples/tested/effects/fiber_effects.osp` and
`examples/tested/effects/http_state_levels.osp`.

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
in perform Ask.value() * 2
```

Resuming handlers have these rules:

- They are deep: the same handler remains installed while the continuation
  runs.
- They are single-shot. A second resume of one continuation aborts with
  `fatal: continuation already resumed (multi-shot resume is not supported)`.
- Handler mode is selected per region. If any arm contains `resume`, an arm
  that returns without resuming aborts the suspended computation and its value
  becomes the result of the whole handler. This per-region selection is a
  **known deviation** from the per-clause rule of the literature: adding `resume`
  to one arm silently changes how a *sibling* arm's non-resuming return is treated
  (recover-and-continue versus abort). It is tracked as
  [issue #177](https://github.com/Nimblesite/osprey/issues/177) — see
  [Relationship to the Literature](#relationship-to-the-literature). Until it is
  resolved, keep each handler in a single mode: either no arm resumes, or every
  control-flow arm does.
- `resume` is lexical to the arm. It is rejected at top level and inside a
  lambda declared in an arm, because that lambda has no live arm continuation.
- Explicit resume is native-only. WebAssembly supports direct value-substitution
  handlers but not the pthread-backed continuation runtime.

Native resume uses one suspended pthread stack as the continuation. Regions
whose arms contain no `resume` stay on the direct handler-call path.

`[EFFECTS-FIBER-PERFORM]` Concurrent performs into one resuming handler are
serialized for the full suspend-to-resume round trip. This prevents arguments
or results from being delivered to the wrong performer.

## Relationship to the Literature

Osprey's handlers follow the algebraic-effects tradition of Plotkin and Pretnar,
in which an effect is a set of typed operations and a handler interprets each one
by supplying its result — optionally with access to the delimited continuation of
the `perform`. Osprey's two handler modes are the two standard clause shapes:

- **Tail-resume mode** is a *tail-resumptive* clause: the arm returns a value that
  is substituted at the operation site and the computation continues. This is
  Koka's `fun` / `val` operation clause — the common case that needs no
  continuation capture and pays no runtime cost for it.
- **Explicit `resume`** is a general clause with access to the continuation `k`:
  `resume(v)` is `k v`. Running code *after* `resume`, and the LIFO unwinding of
  nested continuations, are the observable signature of a genuine delimited
  continuation.
- **Abort** — an explicit-mode arm that never resumes — *discards* the
  continuation, which is exactly how Plotkin–Pretnar and Eff give exceptions and
  early exit: a clause aborts precisely by not invoking `k`.
- **Deep, single-shot handlers.** The handler stays installed for the resumed
  computation (Plotkin–Pretnar deep handlers; OCaml 5 `continue`). Continuations
  are single-shot — one suspended native stack each, the same default restriction
  OCaml 5 imposes — and multi-shot resumption is refused loudly rather than
  emulated with a stale result.

Osprey keeps the *surface* lighter than these systems: tail-resume needs no
keyword, and `resume` is an expression rather than a continuation variable bound
per clause. The *intended* meaning of a single arm still matches the references
exactly — tail-resume when the arm names no `resume`, explicit-or-abort when it
does, decided **per arm**. The implementation currently decides the mode **per
handler** by scanning every arm for the `resume` token, so a sibling arm can flip
another arm's control flow; that is the one substantive departure from the
references below, tracked as a defect rather than a design choice
([issue #177](https://github.com/Nimblesite/osprey/issues/177)). None of the
systems below infer a clause's resumption mode from its siblings: Eff and OCaml 5
bind the continuation explicitly per clause, and Koka declares the mode per clause
with a keyword.

References:

- Gordon Plotkin and Matija Pretnar. *Handling Algebraic Effects.* Logical Methods
  in Computer Science 9(4), 2013. <https://lmcs.episciences.org/705>
- Andrej Bauer and Matija Pretnar. *Programming with Algebraic Effects and
  Handlers* (the Eff language). Journal of Logical and Algebraic Methods in
  Programming 84(1), 2015. <https://arxiv.org/abs/1203.1539>
- Daan Leijen. *Algebraic Effects for Functional Programming* — Koka's per-clause
  `fun` / `ctl` / `final ctl` resumption modes. Microsoft Research technical
  report MSR-TR-2016-29, 2016.
  <https://www.microsoft.com/en-us/research/publication/algebraic-effects-for-functional-programming/>
- KC Sivaramakrishnan, Stephen Dolan, Leo White, Tom Kelly, Sadiq Jaffer, and
  Anil Madhavapeddy. *Retrofitting Effect Handlers onto OCaml* — OCaml 5's
  explicit, single-shot `continue k`. PLDI 2021. <https://arxiv.org/abs/2104.00250>
