# Function Calls

Ordinary Default calls lower to one `Expr::Call`. Dotted calls retain their
receiver until its type determines field access or UFCS fallback
([BUILTIN-STRING-UFCS](0012-Built-InFunctions.md#calling-style--builtin-string-ufcs)).
For a generic receiver this selection occurs at each instantiation.
ML whitespace application and
uncurried grouping lower to the same node shapes as described in
[FLAVOR-CURRY](0023-LanguageFlavors.md#currying-canonicalisation) and
[FLAVOR-ML-CALL](0024-MLFlavorSyntax.md).

## Type arguments [TYPE-GENERICS-APPLY]

A call to a function that declares type parameters may pin them at the call
site: `identity<int>(5)`, ML `identity<int> 5`. The written list is positional,
must match the declared binder count, and is checked against the instantiation
the value arguments infer — the full contract is
[TYPE-GENERICS-APPLY](0004-TypeSystem.md#generics-and-variance), the grammar is
[0003 §Call-site type arguments](0003-Syntax.md#call-site-type-arguments--type-generics-apply).

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

A declaration call is a call whose callee resolves at that source site to a function or extern declaration. Its named values are reordered to the declaration's parameter order before evaluation. The grammar does not permit positional and named arguments in one argument list. Unknown and duplicate names on declaration calls are not rejected consistently; a named declaration call must use each declared name exactly once.

In a UFCS call such as `receiver.f(second: value)`, the receiver supplies the
first declared parameter. Written names supply the remaining parameters in
declaration order. The implicit receiver is preserved even though the written
argument list is named.

A function-value call has ordered parameter types but no parameter-name contract. A callable record field, function parameter, local function value, or returned or computed function is called by slot: positional values fill slots in written order, and labels on a fully named argument list do not change that order. For example, a field of type `(int, int) -> int` called as `record.f(second: 20, first: 10)` receives `20` in its first slot and `10` in its second slot. The actual callback's formal parameter names do not change these slots. A selected record field uses this rule even when a same-named free function exists; UFCS fallback uses the declaration rule above. Type checking, effect analysis, and code generation must use the same selected slot order.

The distinction belongs to the source call site. Learning a function value's runtime target during optimization does not grant that value a declaration's parameter-name contract. This is a semantic requirement for specializations and inlining, not an additional form of name-based dispatch.

These two callbacks have the same function type despite their different formal names. The field calls both pass `20` in the first slot and `10` in the second:

```osprey
type Picker = { choose: (int, int) -> int }
fn first(first: int, second: int) = first
fn last(second: int, first: int) = first
let left = Picker { choose: first }
let right = Picker { choose: last }
print("${left.choose(second: 20, first: 10)} ${right.choose(second: 20, first: 10)}")
// 20 10
```

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
