# Function Calls

Default calls lower to one `Expr::Call`. ML whitespace application and
uncurried grouping lower to the same node shapes as described in
[FLAVOR-CURRY](0023-LanguageFlavors.md#currying-canonicalisation) and
[FLAVOR-ML-CALL](0024-MLFlavorSyntax.md).

## Argument forms [CALL-ARGUMENTS]

Default accepts positional calls at every arity:

```osprey
fn now() = 42
fn double(x) = x * 2
fn add(x, y) = x + y

let a = now()
let b = double(5)
let c = add(10, 20)
```

A call may instead name every supplied argument:

```osprey
let c = add(y: 20, x: 10)
```

For a known function or extern, named values are reordered to the declaration's
parameter order. The grammar does not permit positional and named arguments in
one argument list. Unknown and duplicate argument names are not rejected
consistently; a named call must use each declared name exactly once.

The ML equivalent of the flat two-parameter function is uncurried application:

```osprey-ml
add (x, y) = x + y
c = add (10, 20)
```

`add(10)(20)` is not partial application of a flat Default function. A curried
Default function must explicitly return a function; ML whitespace application
is curry-by-default.

Built-ins use the positional order in their signatures. A positional union
variant such as `Node(Tree, Tree)` is also constructed in slot order. Named
record and union payloads use field construction (`Point { x: 1, y: 2 }`), not
call arguments.
