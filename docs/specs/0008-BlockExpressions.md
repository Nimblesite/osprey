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
a `int` value cannot be discarded; bind it with `let`, or remove the statement
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
