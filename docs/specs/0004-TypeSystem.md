# Type System

- [Hindley-Milner Inference](#hindley-milner-inference)
- [Generics and Variance](#generics-and-variance)
- [Built-in Types](#built-in-types)
- [Result Auto-Unwrapping](#result-auto-unwrapping)
- [Function Types](#function-types)
- [Record Types](#record-types)
- [Union Types](#union-types)
- [Validated Records (`where`)](#validated-records-where)
- [Collection Types](#collection-types)
- [Built-in Error Types](#built-in-error-types)
- [The `any` Type](#the-any-type)
- [Type Annotation Requirements](#type-annotation-requirements)

## Hindley-Milner Inference

Osprey uses Hindley-Milner inference. Every well-typed expression has a unique most general type, inference always terminates, and a successful type-check guarantees no runtime type errors.

> **Flavor layer — shared core (AST and above).**  Type inference runs on the canonical `osprey_ast::Program` *after* lowering, so it is entirely flavor-blind ([FLAVOR-BOUNDARY], [FLAVOR-LAYER]). Nothing in this chapter inspects which surface produced a program: the type checker (`osprey-types`) consumes only the canonical AST. The samples below use the Default surface (`.osp`), but the ML flavor (`.ospml`, see [ML Flavor Syntax](0024-MLFlavorSyntax.md)) lowers to identical ASTs and obeys these inference rules unchanged. See [Language Flavors](0023-LanguageFlavors.md).

Type annotations are optional everywhere they can be inferred:

```osprey
fn identity(x)         = x                       // <T>(T) -> T
fn add(a, b)           = a + b                   // (int, int) -> Result<int, MathError>
fn greet(name)         = "Hello, " + name        // (string) -> string
fn makeUser(n, a)      = User { name: n, age: a }  // (string, int) -> User
fn getName(u)          = u.name                  // (User) -> string
fn twice(f, x)         = f(f(x))                 // <T>((T) -> T, T) -> T
fn compose(f, g)       = fn(x) => f(g(x))        // <A,B,C>((B)->C,(A)->B) -> (A)->C
```

```osprey-ml
identity x       = x                        // <T>(T) -> T
add (a, b)       = a + b                     // (int, int) -> Result<int, MathError>
greet name       = "Hello, " + name          // (string) -> string
makeUser (n, a)  =
    User
        name = n
        age = a                             // (string, int) -> User
getName u        = u.name                    // (User) -> string
twice (f, x)     = f (f x)                   // <T>((T) -> T, T) -> T
compose (f, g)   = \x => f (g x)             // <A,B,C>((B)->C,(A)->B) -> (A)->C
```

`[TYPE-NO-REDUNDANT-ANNOTATION]` **Optional is not the whole rule: an annotation
the checker can infer is *redundant*, and redundant symbols are forbidden.**
Osprey is terse — **neither flavor is verbose** — so any type symbol the compiler
would derive anyway MUST be omitted. This is normative style, not taste:

- **Never** annotate a function parameter whose type is inferable from the body
  or a call site (`fn add(a, b) = a + b`, never `fn add(a: int, b: int)`).
- **Never** write a function return type the checker can infer
  (`fn isEven(x) = (x % 2) == 0`, never `… -> bool`).
- **Never** annotate a `let`/lambda binding the checker can infer
  (`let n = 0`; `|x| => x * 2`).

The rule is identical in both flavors: an ML signature line (`add : int -> int`)
is just as redundant when the body fixes the type, and must be dropped too.

Keep an annotation **only** when the checker genuinely cannot infer it — the
narrow, load-bearing set: an empty literal with no context
(`let xs: List<int> = []`, [TYPE-LIST](#list-t--type-list)); the ambiguous empty
map (`{}` at an ambiguous position, [TYPE-MAP](#map-k-v--type-map)); an `extern`
boundary; an unconstrained polymorphic variable a caller must pin; or a return
type that is *load-bearing* because it forces `Result<T, E>` to auto-unwrap to
`T` at the function boundary ([Result Auto-Unwrapping](#result-auto-unwrapping)).
Declared type-parameter binders (`fn pick<T>(…)`, `type Source<out T>`,
`effect State<T>`) are **declaration sites, not inference sites** — a binder
introduces the parameter the annotations refer to (and carries its variance),
so it is never redundant where the parameter is actually used
([Generics and Variance](#generics-and-variance)); a binder whose parameter
appears nowhere in the signature IS redundant and must go.
Record and union field declarations (`type Point = { x: int, y: int }`,
`Circle { radius: int }`) are **definition sites, not inference sites** — their
`field: Type` annotations *define* the type and are always required, never
redundant; the rule above never touches them. The same holds for `extern`
parameter signatures, which the FFI boundary requires
([Foreign Function Interface](0019-ForeignFunctionInterface.md)).
If deleting an annotation still type-checks and produces identical IR, it was
redundant — delete it.

This rule is **machine-enforced** by the analyzer's first lint,
[`[ANALYZER-REDUNDANT-SYMBOL]`](0020-LanguageServerAndEditors.md#redundant-symbols-analyzer-redundant-symbol),
which flags every redundant annotation and offers a one-keystroke autofix that
deletes it.

A polymorphic function is monomorphised independently at each call site:

```osprey
let i = identity(42)        // identity<int>
let s = identity("hello")   // identity<string>
```

```osprey-ml
i = identity 42          // identity<int>
s = identity "hello"     // identity<string>
```

### Record Type Unification

Two record types unify iff they have the same set of field names and corresponding field types unify. Field order is irrelevant in both declaration and construction.

```
unify(R1, R2) :=
    if names(R1) ≠ names(R2) then FAIL
    else for each f ∈ names(R1): unify(typeOf(R1, f), typeOf(R2, f))
```

### Polymorphic Variables vs `any`

Inference produces polymorphic variables (`<T>`, `<A>`, …), not `any`. The `any` type is opt-in; see [The `any` Type](#the-any-type).

## Generics and Variance

> **Flavor layer — shared core.** Both surfaces lower to the same
> variance-carrying `TypeParam` nodes ([FLAVOR-BOUNDARY]); the ML spellings are
> specified in [ML Flavor Syntax](0024-MLFlavorSyntax.md#generics-flavor-ml-generics).

`[TYPE-GENERICS-DECL]` **Type declarations bind type parameters; constructions
may pin them explicitly.** `type Pair<T, U> = …` binds `T`/`U` across every
variant field. A construction site may apply explicit type arguments —
`Pair<int, string> { first: 1, second: "a" }` — which unify with the
instantiation the fields would otherwise infer; an argument that contradicts a
field is a type error. Explicit construction arguments follow
[TYPE-NO-REDUNDANT-ANNOTATION]: write them only when the fields alone cannot
pin the instantiation.

`[TYPE-GENERICS-FN]` **Functions bind type parameters with `fn name<T, …>`.**
A binder makes every use of `T` in the signature the SAME inference variable;
without it, `T` in an annotation names a nominal type. The binder is
load-bearing exactly when a parameter must relate two or more positions
(`fn pick<T>(first: T, second: T)` pins both arguments to one type) or when a
caller must pin an otherwise-unconstrained variable. HM inference is
unchanged: unannotated functions stay implicitly polymorphic, and a
polymorphic function is still monomorphised independently at each call site.
Variance markers are **not** permitted on function binders (variance is
declaration-site on types and effects only — [TYPE-VARIANCE-DECL]).

```osprey
fn pick<T>(first: T, second: T) = first
let n = pick(10, 20)
let s = pick("left", "right")
```

```osprey-ml
pick<T> : (T, T) -> T
pick (first, second) = first
n = pick (10, 20)
s = pick ("left", "right")
```

In the ML flavor the binder lives on the signature line (`pick<T> : …`); a
binding without a signature cannot declare type parameters.

`[TYPE-VARIANCE-DECL]` **Type parameters declare variance at the declaration
site**: `out T` (covariant — `T` only flows out), `in T` (contravariant — `T`
only flows in), unannotated (invariant — exact match). `out` and `in` are
contextual keywords, reserved only inside type-parameter lists
([Lexical Structure](0002-LexicalStructure.md#keywords)).

```osprey
type Feed<out T> = Feed { supply: T } | Dry
type Gate<in T>  = Gate { admit: (T) -> bool } | Open
```

```osprey-ml
type Feed out T =
    Feed
        supply : T
    Dry
type Gate in T =
    Gate
        admit : T -> bool
    Open
```

`[TYPE-VARIANCE-POSITIONS]` **Variance is position-checked.** Walking a
declaration's field (or effect-operation) types: fields and function results
are OUTPUT positions; function parameters flip the polarity (INPUT); a nested
constructor's argument composes the position with that constructor's declared
variance (an invariant argument position demands both directions, so only
invariant parameters may sit there). A covariant parameter in an input
position, or a contravariant parameter in an output position, is a compile
error. Effect operations check the same way: operation parameters are inputs,
operation results outputs ([Algebraic Effects](0017-AlgebraicEffects.md#generic-effects)).

`[TYPE-VARIANCE-ASSIGN]` **Variance directs assignability structurally, and
the leaves match exactly.** Plain HM unification is untouched — every
well-typed expression keeps a principal type. At *assignment sites* (call
arguments, annotated bindings, return positions), a variance-declared
constructor's arguments are matched directionally: covariant (`out`)
arguments recurse expected-accepts-actual, contravariant (`in`) arguments
recurse with the roles flipped, invariant arguments unify exactly. The
recursion continues only through variance-declared constructors and bottoms
out in **exact unification** — the coercive
[Result auto-unwrap](#result-auto-unwrapping) never applies under a
container, because it is a representation-changing coercion the compiler
emits only at direct value sites; admitting it inside a payload would accept
values whose stored representation is wrong. Function-typed payloads keep
their flexibility representation-safely: unification itself normalizes
function returns through `Result` (the function-value ABI strips the
wrapper), so a `Feed<(int) -> Result<int, MathError>>` matches a
`Feed<(int) -> int)` slot.

Built-in constructors' declared variance: `Result<out T, out E>`,
`List<out T>`, `Fiber<out T>`, `Map<K, out V>` (keys invariant); `Channel<T>`
and `Ptr` are invariant. Function types are structurally contravariant in
parameters and covariant in returns, as before.

## Built-in Types

All primitive types are lowercase.

| Type             | Description                                                        |
| ---------------- | ------------------------------------------------------------------ |
| `int`            | 64-bit signed integer (LLVM `i64`)                                 |
| `float`          | 64-bit IEEE 754 (LLVM `double`)                                    |
| `string`         | UTF-8 encoded                                                      |
| `bool`           | `true` \| `false`                                                  |
| `unit`           | The single value `()`; the return type of a function with no result|
| `any`            | Erased value; access requires pattern matching                     |
| `Result<T, E>`   | Error-handling sum type (see [Error Handling](0013-ErrorHandling.md)) |
| `List<T>`        | Immutable sequential collection                                    |
| `Map<K, V>`      | Immutable key/value collection                                     |

`int` and `float` do not implicitly convert. Use `toFloat(int)` and `toInt(float)`.

Arithmetic returns `Result<T, MathError>`. The full operator-by-operand table and chaining rules are in [Error Handling](0013-ErrorHandling.md#arithmetic-returns-result).

## Result Auto-Unwrapping

A bare arithmetic expression has type `Result<T, MathError>`. The compiler auto-unwraps the inner `Result` in five contexts so authors do not write nested `match` chains:

1. **Nested arithmetic.** `(10 + 5) * 2` has type `Result<int, MathError>`, not `Result<Result<int, MathError>, MathError>`. If any sub-expression errors, the chain errors.
2. **User function arguments.** Passing a `Result`-typed expression to a function that expects the underlying type unwraps it. `double(add(a: 5, b: 3))` is well-typed when `add` returns `Result<int, MathError>` and `double` expects `int`.
3. **Fiber operations.** `spawn`, `await`, `send`, and `recv` unwrap `Result` arguments before storing them.
4. **Function-value calls.** A `Result` returned through a function value unwraps at the call site ([TYPE-FN-CLOSURE]).
5. **String interpolation.** `${expr}` renders the success payload (see [String Interpolation](0006-StringInterpolation.md)).

Auto-unwrap does **not** apply to:

- `toString`. `toString(add(a: 5, b: 3))` produces `"Success(8)"`, never `"8"`. Use `toString` to inspect the `Result` itself.
- A function's declared return type. `fn add(x, y) = x + y` returns `Result<int, MathError>`; `fn compute() -> int = 5` returns `int`.

The top-level value of an arithmetic expression must still be either matched or stored as a `Result`.

## Function Types

```ebnf
functionType ::= "(" (type ("," type)*)? ")" "->" type
```

```osprey
(int) -> int
(int, string) -> bool
() -> string
(string) -> (int) -> bool          // higher-order
```

```osprey
fn applyFunction(value: int, transform: (int) -> int) -> int = transform(value)

let doubler: (int) -> int = fn(x: int) => x * 2

fn createAdder(n: int) -> (int) -> int = fn(x: int) => x + n
```

```osprey-ml
applyFunction : (int, (int) -> int) -> int
applyFunction (value, transform) = transform value

doubler : (int) -> int
doubler = \x => x * 2

createAdder : int -> (int) -> int
createAdder n = \x => x + n
```

Multi-argument call syntax (named arguments are required for two or more parameters) is in [Function Calls](0005-FunctionCalls.md).

### Closures — [TYPE-FN-CLOSURE]

A lambda (`fn(...) => expr` or `|x| => expr`) captures every free identifier from its enclosing lexical scope by reference to its value at capture time. Captured bindings are immutable, so by-reference and by-value capture are observationally identical and the implementation MAY choose either. A captured binding outlives the surrounding stack frame: a closure returned from a function remains callable and continues to read the captured values.

```osprey
fn makeAdder(n: int) -> (int) -> int = fn(x: int) => x + n   // captures n

let add5    = makeAdder(5)
let add10   = makeAdder(10)
print(add5(3))     // 8
print(add10(3))    // 13

let prefix  = "hello "
let greet   = fn(name: string) => prefix + name              // captures prefix
print(greet("world"))                                         // "hello world"
```

```osprey-ml
makeAdder : int -> (int) -> int
makeAdder n = \(x : int) => x + n               // captures n

add5    = makeAdder 5
add10   = makeAdder 10
print (add5 3)     // 8
print (add10 3)    // 13

prefix  = "hello "
greet   = \(name : string) => prefix + name     // captures prefix
print (greet "world")                                         // "hello world"
```

Closures and named functions are interchangeable wherever a function type is expected, including as higher-order arguments (`map`, `filter`, `fold`, `forEach`) and as the function field of records. A closure that captures no free variables is equivalent to a top-level function and the implementation SHOULD lower it to one. A `Result<T, E>` returned through a function-value call auto-unwraps to `T` (context 4 of [Result Auto-Unwrapping](#result-auto-unwrapping)).

## Record Types

```ebnf
recordType ::= "type" ID "=" "{" field ("," field)* "}" constraint?
field      ::= ID ":" type
constraint ::= "where" function_name
```

```osprey
type Point   = { x: int, y: int }
type Person  = { name: string, age: int, active: bool }
```

```osprey-ml
type Point =
    x : int
    y : int

type Person =
    name : string
    age : int
    active : bool
```

### Construction

```osprey
let point  = Point  { x: 10, y: 20 }
let person = Person { name: "Alice", age: 30, active: true }

// Field order at construction is irrelevant
let person2 = Person { active: true, name: "Bob", age: 22 }
```

```osprey-ml
point =
    Point
        x = 10
        y = 20
person =
    Person
        name = "Alice"
        age = 30
        active = true

// Field order at construction is irrelevant
person2 =
    Person
        active = true
        name = "Bob"
        age = 22
```

All fields are required. Missing or unknown fields, or type mismatches, are compilation errors.

### Field Access

Direct field access is permitted only on a record value. Field access on `any`, `Result`, or any union type requires `match` to narrow the value first.

```osprey
let n = person.name        // ok

// any: pattern-match
fn nameOf(v: any) -> string = match v {
    p: { name } => p.name
    _           => "unknown"
}

// Result: match before access
match personResult {
    Success { value }   => print(value.name)
    Error   { message } => print(message)
}

// Union: discriminate first
let area = match shape {
    Circle    { radius }         => 3.14 * radius * radius
    Rectangle { width, height }  => width * height
}
```

```osprey-ml
n = person.name        // ok

// Result: match before access
match personResult
    Success value => print value.name
    Error message => print message

// Union: discriminate first
area =
    match shape
        Circle radius => 3.14 * radius * radius
        Rectangle width height => width * height
```

> The `any` structural-narrowing arm (`p: { name } => p.name`) has no ML-flavor
> surface syntax; use the Default flavor for structural matching on `any`.

The compiler implementation must look up fields by name; positional access is forbidden in code generation.

### Immutability and Non-Destructive Update

Records cannot be modified. To produce a record that differs in some fields from an existing one, use the update form:

```osprey
let p2 = point  { x: 15 }                // y carried over
let p3 = person { age: 26, active: false }
```

```osprey-ml
p2 = point(x = 15)               // y carried over
p3 = person(age = 26, active = false)
```

### Nested Records

```osprey
type Address = { street: string, city: string, zipCode: string }
type Company = { name: string, address: Address }

let company = Company {
    name:    "Tech Corp",
    address: Address { street: "456 Tech Ave", city: "Sydney", zipCode: "2000" }
}

let companyCity = company.address.city
```

```osprey-ml
type Address =
    street : string
    city : string
    zipCode : string

type Company =
    name : string
    address : Address

company = Company(name = "Tech Corp", address = Address(street = "456 Tech Ave", city = "Sydney", zipCode = "2000"))

companyCity = company.address.city
```

## Union Types

A union type (also "sum type", "tagged union", "discriminated union") declares a closed set of named variants. Each variant is either nullary (no payload) or carries a record-style payload. Grammar in [Syntax](0003-Syntax.md#type-declarations); pattern-matching rules in [Pattern Matching](0007-PatternMatching.md).

```osprey
type Color  = Red | Green | Blue
type Shape  = Circle    { radius: float }
            | Rectangle { width:  float, height: float }
            | Triangle  { a: float, b: float, c: float }
```

```osprey-ml
type Color =
    Red
    Green
    Blue

type Shape =
    Circle
        radius : float
    Rectangle
        width : float
        height : float
    Triangle
        a : float
        b : float
        c : float
```

A union value carries a runtime discriminant identifying its variant; the compiler emits one branch per variant in any `match`. Field access on a union requires `match` to narrow it to a single variant first.

### Recursive Variants — [TYPE-UNION-REC]

A variant's payload MAY reference the union type itself, either directly or through a built-in collection. This is the foundation of every tree-shaped data structure (AST, file tree, scene graph, parsed JSON).

```osprey
type Tree = Leaf | Node { value: int, left: Tree, right: Tree }

type JsonValue =
    JNull
    | JBool { v: bool }
    | JNum  { v: float }
    | JStr  { v: string }
    | JArr  { items:   List<JsonValue> }
    | JObj  { entries: Map<string, JsonValue> }
```

```osprey-ml
type Tree =
    Leaf
    Node
        value : int
        left : Tree
        right : Tree

type JsonValue =
    JNull
    JBool
        v : bool
    JNum
        v : float
    JStr
        v : string
    JArr
        items : List<JsonValue>
    JObj
        entries : Map<string, JsonValue>
```

A recursive union is laid out indirectly — variant payloads referencing the same type, or containing a `List<Self>` / `Map<K, Self>`, MUST be stored behind a pointer so the type's size is finite. This requirement is invisible to the user: construction, pattern-matching, and field access read the same as for any other variant. Mutually recursive unions follow the same rule.

## Validated Records (`where`)

A `where` clause attaches a validation function to a record type. The constructor of a validated type returns `Result<T, string>` instead of `T`, and the validation function runs at construction time.

```osprey
type Product = {
    name:  string,
    price: int
} where validateProduct

fn validateProduct(p: Product) -> Result<Product, string> = match p.name {
    ""  => Error   { message: "name cannot be empty" }
    _   => match p.price {
        0 => Error   { message: "price must be positive" }
        _ => Success { value:   p }
    }
}

// Construction returns Result<Product, string>
let r = Product { name: "Widget", price: 100 }
match r {
    Success { value }   => print("ok: ${value.name}")
    Error   { message } => print("validation failed: ${message}")
}
```

```osprey-ml
type Product =
    name : string
    price : int

validateProduct : Product -> Result<Product, string>
validateProduct p =
    match p.name
        "" => Error(message = "name cannot be empty")
        _ =>
            match p.price
                0 => Error(message = "price must be positive")
                _ => Success(value = p)

// Construction returns Result<Product, string>
r = Product(name = "Widget", price = 100)
match r
    Success value => print "ok: ${value.name}"
    Error message => print "validation failed: ${message}"
```

Field access on a validated value is only legal after matching on the `Result`.

## Collection Types

`List<T>` and `Map<K, V>` are the two built-in collection types. Both are **immutable persistent** structures: every operation that would mutate returns a new collection that shares structure with the original. There is no mutable variant. `Set<T>` is reserved for a future revision and is **not** part of this spec; use `Map<K, unit>` if a set-like semantic is required in the meantime.

Builtin signatures referenced below are specified in [Built-in Functions](0012-Built-InFunctions.md) under "Collection Functions". Iterator operations (`map`, `filter`, `fold`, `forEach`) are specified in [Iterators and Iteration](0010-LoopConstructsAndFunctionalIterators.md) and work uniformly on lists, maps, and ranges via the implicit `Iterable` constraint.

### `List<T>` — [TYPE-LIST]

`List<T>` is an immutable, homogeneous, indexed sequence with structural sharing. The implementation MUST provide the asymptotic bounds listed under [Performance](#performance-type-list-perf); a bitmapped vector trie (branching factor 32) is the recommended baseline, with an upgrade path to an RRB-tree for O(log n) concatenation. Index access is bounds-checked and returns `Result<T, IndexError>`.

```osprey
let numbers = [1, 2, 3, 4, 5]            // List<int>
let names   = ["Alice", "Bob"]           // List<string>

// Empty literal cannot infer its element type unless the context provides it
let empty: List<int> = []                // ok
let total = sumOfInts([])                // ok if sumOfInts: (List<int>) -> int

match numbers[0] {
    Success { value }   => print(value)
    Error   { message } => print(message)
}
```

#### Operations — [TYPE-LIST-OPS]

```osprey
let doubled  = numbers |> map(fn(x) => x * 2)
let evens    = numbers |> filter(fn(x) => x % 2 == 0)
let total    = numbers |> fold(0, fn(acc, x) => acc + x)
let combined = numbers + [6, 7, 8]                       // concatenation produces a new list
numbers |> forEach(fn(x) => print(toString(x)))
```

The `+` operator is defined on `(List<T>, List<T>) -> List<T>` and returns a new list. Chains of `map`/`filter` terminated by `forEach`/`fold` are fused per [Stream Fusion](0010-LoopConstructsAndFunctionalIterators.md#stream-fusion); no intermediate list is materialised.

#### Patterns — [TYPE-LIST-PATTERNS]

```osprey
fn classify(xs: List<int>) -> string = match xs {
    []                 => "empty"
    [single]           => "one"
    [first, second]    => "two"
    [head, ...tail]    => "many starting with ${head}"
}
```

A list pattern matches a list of exactly the listed length unless the final element is a rest binder (`...name`). The rest binder is itself a `List<T>` and is `[]` when the underlying list has exactly the prefix length.

#### Comprehensions — [TYPE-LIST-COMP]

```osprey
let squares  = [x * x for x in range(1, 6)]               // [1, 4, 9, 16, 25]
let filtered = [x for x in numbers if x > 3]
let [head, ...tail] = numbers
```

Comprehensions desugar to `map` + `filter` over the source iterator and are subject to the same stream-fusion rules.

### `Map<K, V>` — [TYPE-MAP]

`Map<K, V>` is an immutable, persistent associative collection keyed by `K`. The implementation MUST provide the asymptotic bounds listed under [Performance](#performance-type-map-perf); a hash array mapped trie (HAMT, branching factor 32) per Bagwell (2000) is the recommended baseline. Iteration order is **unspecified** and MUST NOT be relied upon; programs that need a deterministic order MUST sort the result of `keys(map)` or `entries(map)`.

Keys MUST be of a type for which the runtime provides a total hash and equality. The set of permitted key types in this revision is `int`, `string`, and `bool`; structurally-compared records and unions are reserved for a later revision and will fail type-checking until then.

#### Literals — [TYPE-MAP-LITERAL]

```osprey
let ages = {
    "Alice":   25,
    "Bob":     30,
    "Charlie": 35
}                                                 // Map<string, int>
```

The empty map literal `{}` is parsed as a `Map<K, V>` literal **only** at expression positions where a block expression is disallowed (e.g. RHS of `=` in a `let`, function argument). At ambiguous positions an explicit type annotation is required:

```osprey
let scores: Map<string, int> = {}                 // ok: typed empty map
let always_a_block          = { 1 }               // block expression returning 1
```

Duplicate keys in a literal are a **compile-time error**.

#### Lookup — [TYPE-MAP-LOOKUP]

Index lookup returns `Result<V, IndexError>`:

```osprey
match ages["Alice"] {
    Success { value }   => print(toString(value))
    Error   { message } => print(message)
}
```

#### Operations — [TYPE-MAP-OPS]

All operations return a new map and never mutate the receiver.

```osprey
let bumped     = ages |> mapValues(fn(age) => age + 1)
let upper      = ages |> mapKeys(fn(name) => toUpperCase(name))
let thirties   = ages |> filterEntries(fn(k, v) => v >= 30)
let totalAge   = ages |> foldEntries(0, fn(acc, k, v) => acc + v)
let merged     = ages + { "Dave": 28 }                      // right-biased union
let updated    = set(ages, "Alice", 26)                     // single-key update
let withoutBob = remove(ages, "Bob")
```

Map-specific iterator forms (`filterEntries`, `foldEntries`, `mapValues`, `mapKeys`) take the key and value as separate arguments rather than as one packed value, mirroring Elm's `Dict.foldl : (comparable -> v -> b -> b) -> b -> Dict comparable v -> b`. Plain `map`/`filter`/`fold` from the iterator module operate on `entries(map)` and receive a single `Entry<K, V>` record (`{ key, value }`, defined with the Map builtins in [Built-In Functions](0012-Built-InFunctions.md)) per element — Osprey has no tuple type.

The `+` operator on `(Map<K, V>, Map<K, V>) -> Map<K, V>` is **right-biased** (the right-hand side wins on conflicting keys).

#### Patterns — [TYPE-MAP-PATTERNS]

A map pattern is **subset-matching**: it matches any map whose entries are a superset of the listed entries. Map patterns are distinguished from record patterns by the presence of string-literal (or int-literal) keys; record patterns use bare identifiers.

```osprey
fn analyze(p: Map<string, int>) -> string = match p {
    p when length(p) == 0                    => "none"
    { "Alice": age }                         => "only Alice (${age}) or Alice + others"
    { "Alice": _, "Bob": _ }                 => "contains both Alice and Bob"
    p when length(p) > 5                     => "large"
    _                                        => "other"
}
```

The literal `{}` is disallowed as a pattern (it would match every map). Match emptiness explicitly with a guard: `p when length(p) == 0`.

#### Conversions — [TYPE-MAP-CONV]

```osprey
let names    = keys(ages)                                  // List<string>, order unspecified
let agesList = values(ages)                                // List<int>,    order unspecified
let pairs    = entries(ages)                               // List<Entry<string, int>>
let m        = zipToMap(names, agesList)                   // Result<Map<K,V>, IndexError> if lengths differ
let byGrade  = groupBy(students, fn(s) => s.grade)         // Map<string, List<Student>>
```

`zipToMap` returns a `Result` because mismatched lengths are an error. `groupBy` preserves the relative order of items within each bucket.

### Performance

| Operation             | `List<T>` (bitmapped trie) | `Map<K, V>` (HAMT)                                |
| --------------------- | -------------------------- | ------------------------------------------------- |
| Index / lookup        | O(log₃₂ n)                 | O(log₃₂ n) expected, O(n) worst case under collisions |
| Insert / update       | O(log₃₂ n)                 | O(log₃₂ n) expected                               |
| Remove                | O(log₃₂ n)                 | O(log₃₂ n) expected                               |
| Concatenation (`+`)   | O(n + m)                   | O(min(n, m) · log₃₂ max(n, m))                    |
| `map` / `filter`      | O(n) (fused, no intermediate) | O(n) (fused, no intermediate)                  |
| `length`              | O(1)                       | O(1)                                              |
| Iteration             | O(n)                       | O(n), order unspecified                           |

A future revision MAY upgrade `List<T>` to an RRB-tree (Bagwell & Rompf 2011; Stucki & Rompf 2015) to bring concatenation to O(log n) without changing the API. Both collections use structural sharing — `O(log₃₂ n)` path-copying per update — so old versions remain valid in O(1) space relative to a modification.

## Built-in Error Types

| Type          | Used by                                              |
| ------------- | ---------------------------------------------------- |
| `MathError`   | Arithmetic (`DivisionByZero`, `Overflow`, `Underflow`)|
| `ParseError`  | String parsing                                       |
| `IndexError`  | List and string indexing (`OutOfBounds`)             |
| `StringError` | String operations that can fail (`length`, `substring`, `contains`) |
| `ChannelError`| Channel send/recv                                    |

`Success` and `Error` are the constructors of `Result<T, E>` (see [Error Handling](0013-ErrorHandling.md)).

## The `any` Type

`any` is an erased type. It exists so a function may receive a value whose type is not known statically — for example, parsed JSON or a foreign-function return value. Direct use of an `any` value is forbidden; the value must be narrowed by `match` first.

### Forbidden Operations

```osprey
fn processAny(v: any) -> int = v + 1                       // ❌ direct arithmetic
fn getLength(v: any) -> int = v.length                     // ❌ direct field access
let n: int = someAnyFunction()                             // ❌ implicit conversion
fn callIt(v: any) = someFunction(v)                        // ❌ pass to typed parameter
let s = toString(v)              // where v: any            // ❌ implicit conversion
```

Each of these produces a compilation error.

### Required Form

```osprey
fn process(v: any) -> int = match v {
    n: int    => n + 1
    s: string => length(s)
    _         => 0
}
```

Pattern syntax (`name: Type`, `name: { fields }`, `{ fields }`, `_`) is defined in [Pattern Matching](0007-PatternMatching.md).

### Exhaustiveness

A `match` on `any` must either cover every type the value may take or include a wildcard `_`:

```osprey
match v {
    n: int    => processInt(n)
    s: string => processString(s)
    _         => handleOther()
}
```

A wildcard arm that returns the matched value preserves the `any` type; to escape `any`, every arm (including the wildcard) must return the same concrete type.

### Compiler Errors

The compiler emits the following message strings on `any`-type misuse:

```
cannot use 'any' type directly in arithmetic operation - pattern matching required
cannot access field on 'any' type without pattern matching
cannot assign 'any' to 'TYPE' without pattern matching
cannot pass 'any' type to function expecting 'TYPE' - pattern matching required
cannot implicitly convert 'any' to 'TYPE' - use pattern matching to extract specific type
cannot access variable of type 'any' directly - pattern matching required
pattern matching on 'any' type must handle all possible types or include wildcard
```

### Documented Type Sets

When the compiler has information about which types an `any`-returning function may produce — for example, from an `extern` declaration or annotation — it rejects patterns that match impossible types and unreachable patterns:

```
pattern 'TYPE' is not a possible type for expression of documented types [TYPE1, TYPE2, ...]
unreachable pattern: 'TYPE' cannot occur based on context analysis
pattern matching includes impossible type 'TYPE' - check function documentation
```

## Type Annotation Requirements

Annotations are required when the compiler cannot infer a type:

- An empty literal (`[]`, `{}`) with no contextual type.
- A function whose return type is ambiguous (for example, a value returned from an `extern`).
- A polymorphic function whose type variables are not constrained by any argument.

```osprey
let xs: List<int> = []
fn parseValue<T>(input: string) -> Result<T, ParseError> = ...
```

The compiler emits an error in each case where inference is ambiguous; explicit annotations resolve them.
