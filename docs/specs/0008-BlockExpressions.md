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
