# Algebraic Effects

An effect declares typed operations. `perform` invokes the innermost matching
lexical handler, which supplies the operation result. Default and ML syntax
lower to the same effect, perform, handler, and resume AST nodes; their runtime
semantics are identical.

The checker validates declared operations and their value types. Effect rows
currently provide generic-instantiation scope inside the annotated function
body, but are not stored in function types or propagated through calls. A
missing handler can therefore compile; the generated program aborts with
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
  becomes the result of the whole handler.
- `resume` is lexical to the arm. It is rejected at top level and inside a
  lambda declared in an arm, because that lambda has no live arm continuation.
- Explicit resume is native-only. WebAssembly supports direct value-substitution
  handlers but not the pthread-backed continuation runtime.

Native resume uses one suspended pthread stack as the continuation. Regions
whose arms contain no `resume` stay on the direct handler-call path.

`[EFFECTS-FIBER-PERFORM]` Concurrent performs into one resuming handler are
serialized for the full suspend-to-resume round trip. This prevents arguments
or results from being delivered to the wrong performer.
