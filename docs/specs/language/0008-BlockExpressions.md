# Block Expressions

A block evaluates local bindings and expressions in order, then yields its
trailing expression. Default delimits a block with braces; ML uses an indented
layout block. Both lower to `Expr::Block { statements, value }`.

```ebnf
block ::= "{" statement* expression? "}"
```

```osprey
let result = {
    let x = 10
    let y = 20
    x + y
}
```

```osprey-ml
result =
    x = 10
    y = 20
    x + y
```

## Evaluation and scope [BLOCK-SCOPE]

- Statements run from top to bottom.
- The trailing expression can use bindings introduced earlier in the block.
- Each block has a child lexical scope. An inner binding may shadow an outer
  binding; leaving the inner block restores the outer binding.
- A binding introduced in a block is not visible after that block ends.
- An outer binding remains readable inside nested blocks.
- An initializer reads the enclosing binding until the new binding is initialized. Shadowing applies equally to immutable values, shared mutable cells, concrete lambdas and generic function aliases. Leaving a block restores the enclosing binding's storage and callable identity; an escaping closure retains the binding it captured.

The existing `block_statements_basic` Default/ML regression twins assert these rules for nested cells, immutable bindings over cells, concrete lambdas, generic aliases escaping function values and the independent representation of a binding shadowing a `Result`. The debugger observes the same initialization boundary ([DEBUGGER-BINDING-LIFETIME](../compiler/DebugArtifacts.md#binding-lifetime-debugger-binding-lifetime)).

```osprey
let x = 100
let result = {
    let x = 50
    let inner = {
        let x = 25
        x
    }
    x + inner
}
// result is 75; the outer x remains 100
```

## Result value

A block with a trailing expression has that expression's type and value. A
block without one yields `Unit`.

## Discarded values [BLOCK-DISCARD]

A statement runs for its effects and its value is thrown away, so a statement's
type must be `Unit`. Any other type is a compile error naming the type and
pointing at the statement:

```
a `int` value cannot be discarded; bind it with `let _ =` if that is deliberate, or remove the statement
```

A discarded `Result` keeps its own wording, because there the fix is to handle
the error channel rather than to bind the value
([ERROR-RESULT-DISCARD](0013-ErrorHandling.md#discarding-a-result--error-result-discard)):

```
an unhandled `Result` cannot be discarded; use `match` or `?:`
```

The rule reaches every statement position — inside a block, inside a lambda
body, and at file scope — because the value is equally lost in all of them. The
trailing expression is not a statement and is never subject to it.

### Discarding on purpose

Bind to `_` when a value genuinely has no use at the call site. The binding is
the author saying the loss is intended:

```osprey
fn record(sale) ![Db] = {
    let _ = perform Db.record(sale)
    print("recorded")
}
```

```osprey-ml
record sale =
    _ = perform Db.record sale
    print "recorded"
```

The check runs against the final substitution, so a type still open where the
statement is written is judged at the type it eventually resolved to. A type
variable that never resolves is accepted: nothing proves it is not `Unit`.

## Flat binding with `use` [BLOCK-USE]

Fallible steps run one after another. Without help, each step nests inside the
callback of the one before it, and the code drifts right with a run of closing
brackets. A `use` statement keeps such code flat: it hands **the rest of its
block** to a call as that call's last argument, a callback.

```osprey
fn order(id) = {
    use customer <- andThen(findCustomer(id))
    use _ <- andThen(checkCredit(customer))
    use invoice <- andThen(bill(customer))
    Success { value: invoice.total }
}
```

```osprey-ml
order id =
    use customer <- andThen (findCustomer id)
    use _ <- andThen (checkCredit customer)
    use invoice <- andThen (bill customer)
    Success(value = invoice.total)
```

`use p <- f(a, b)` is sugar for `f(a, b, fn(p) => <rest of the block>)`, and the
ML `use p <- f (a, b)` for `f (a, b, \p => <rest>)`. The parser does the whole
rewrite: the checker and code generator see only the ordinary call and lambda,
so the IR is byte-identical to the nested form written by hand.

- `use _ <- f(...)` is a guard step: its value is discarded.
- `use <- f(...)` passes a callback that takes no argument, such as a resource
  scope: `use <- withLock(lock)`.
- `use a, b <- f(...)` passes a callback with two parameters.
- A `use` works in any block: a function body, a match arm, a handler arm or a
  lambda.

`use` is a keyword in both flavors, so it is no longer a valid name for a
variable, parameter or function. A method may still be called `use`:
`app.use(log)` is a method call.

### Rejections [BLOCK-USE-REJECT]

Each is a compile error that names the fix:

- a `use` on the last line of its block, which leaves the callback no body;
- a `use` outside any block, such as at the top of a file;
- a right-hand side that is not a call, such as `use x <- 42`;
- a callee that passes its callback a different number of values than the
  `use` binds: "`andThen` passes 1 value to its callback, but this callback takes
  2". The same message reports a written lambda of the wrong arity.

### Lexing [LEX-USE-ARROW]

`<-` is one token only after `use` and its binders. Everywhere else it is still
`<` followed by `-`, so `x<-1` compares `x` with `-1` exactly as before.
