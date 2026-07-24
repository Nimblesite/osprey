# Function Calls

> **Flavor layer — surface (CST) only.** Arity, naming, and saturation are
> checked on the shared `Expr::Call`. Default `f()`, `f(x)`, and
> `f(x: a, y: b)` each lower to one call node. ML whitespace application
> `f a b` curries by default and lowers to nested calls; uncurried `f (a, b)`
> lowers to one multi-argument call, matching Default `f(x: a, y: b)`. The
> parenthesised comma-list groups arguments; it is not a tuple. See
> [FLAVOR-CURRY](0023-LanguageFlavors.md#currying-canonicalisation) and
> [FLAVOR-ML-CALL](0024-MLFlavorSyntax.md).

## Named Arguments Requirement

Functions with more than one parameter must be called with named arguments.

### Valid Function Calls

```osprey
// Zero parameters
fn getValue() = 42
let value = getValue()

// Single parameter - positional allowed
fn double(x) = x * 2
let result = double(5)

// Multiple parameters - named arguments required
fn add(x, y) = x + y
let sum = add(x: 10, y: 20)

// Order doesn't matter with named arguments
let sum2 = add(y: 20, x: 10)

```

```osprey-ml
// Zero parameters
getValue () = 42
value = getValue ()

// Single parameter - positional allowed
double x = x * 2
result = double 5

// Multiple parameters - uncurried call
add (x, y) = x + y
sum = add (10, 20)

// Order is positional in the uncurried form
sum2 = add (10, 20)

```

### Invalid Function Calls

```osprey
// ERROR: Multi-parameter function with positional arguments
fn add(x, y) = x + y
let sum = add(10, 20)  // Compilation error

// ERROR: Mixed positional and named arguments
let sum = add(10, y: 20)  // Compilation error
```

## Rules

1. Zero parameters: empty parentheses, `f()`.
2. One parameter: positional or named.
3. Two or more parameters: every argument must be named. Mixing positional and named arguments is a compilation error.
4. **Built-in functions** ([Built-in Functions](0012-Built-InFunctions.md)) are exempt: they take positional arguments in subject-first order — `split("a,b,c", ",")`, `fold(xs, 0, add)` — so the pipe can supply the subject as the first argument: `xs |> fold(0, add)`.
5. **Positional constructors** ([TYPE-UNION-POSITIONAL](0003-Syntax.md#type-declarations)) are exempt: a variant declared `Node(Tree, Tree)` is built `Node(left, right)` in slot order, because a positional payload has no field names to supply. The same variant declared with named fields keeps rule 3. Status: specified; not yet implemented.

Argument order at the call site is independent of declaration order; the compiler reorders by name.

Default `add(x: 10, y: 20)` and ML `add (10, 20)` lower alike. Default
explicit curry `add(10)(20)` corresponds to ML whitespace `add 10 20`; each
pair has the same call shape and IR.
