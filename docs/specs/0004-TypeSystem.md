# Type System

- [Hindley-Milner Inference](#hindley-milner-inference)
- [Generics and Variance](#generics-and-variance)
- [Built-in Types](#built-in-types)
- [Result Preservation](#result-preservation)
- [Function Types](#function-types)
- [Record Types](#record-types)
- [Union Types](#union-types)
- [Collection Types](#collection-types)
- [Built-in Error Types](#built-in-error-types)
- [The `any` Type](#the-any-type--type-any)
- [Type Annotations](#type-annotations--type-annotation-check)

## Hindley-Milner Inference

Osprey uses Hindley-Milner inference over the canonical AST produced by either
surface syntax ([FLAVOR-BOUNDARY]). Examples show both surfaces where their
spellings differ.

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

`add` follows [ARITH-CHECKED]
([Error Handling](0013-ErrorHandling.md#arithmetic-and-result--arith-checked)):
integer `+ - *` return `Result<int, MathError>`. With a `float` operand, the
integer is promoted and the IEEE-754 operation returns plain `float`.

Record fields and foreign declarations include types as part of their syntax;
annotations on bindings and functions constrain the inferred type.

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

Inference produces polymorphic variables (`<T>`, `<A>`, …), not `any`. The `any` type is opt-in; see [The `any` Type](#the-any-type--type-any).

## Generics and Variance

> **Flavor layer — shared core.** Both surfaces lower to the same
> variance-carrying `TypeParam` nodes ([FLAVOR-BOUNDARY]); the ML spellings are
> specified in [ML Flavor Syntax](0024-MLFlavorSyntax.md#generics-flavor-ml-generics).

`[TYPE-GENERICS-DECL]` **Type declarations bind type parameters; constructions
may pin them explicitly.** `type Pair<T, U> = …` binds `T`/`U` across every
variant field. A construction site may apply explicit type arguments —
`Pair<int, string> { first: 1, second: "a" }` — which unify with the
instantiation the fields would otherwise infer; an argument that contradicts a
field is a type error.

`[GENERICS-CTOR-ARITY]` **Explicit constructor type arguments must match the
declaration's arity.** `Box<int> { v: 1 }` against `type Box<T>` is well-formed;
`Box<int, string> { v: 1 }` is rejected with
`takes 1 type argument(s), got 2`. Writing the arguments out is a contract with
the declaration, so a count mismatch is an error rather than a silently ignored
annotation.

`[TYPE-GENERICS-FN]` **Functions bind type parameters with `fn name<T, …>`.**
A binder makes every use of `T` in the signature the SAME inference variable;
without it, `T` in an annotation names a nominal type. The binder is
load-bearing exactly when a parameter must relate two or more positions
(`fn pick<T>(first: T, second: T)` pins both arguments to one type) or when a
caller must pin an otherwise-unconstrained variable. HM inference is
unchanged: unannotated functions stay implicitly polymorphic, and a
polymorphic function is monomorphised independently at each call site.
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
out in **exact unification**. There is no `Result<T, E>`-to-`T` coercion at any
depth or direct value site: it would erase a failure and accept a value with
the wrong representation. Function returns also match exactly, so a
`Feed<(int) -> Result<int, Error>>` does not match a
`Feed<(int) -> int>` slot.

Built-in constructors' declared variance: `Result<out T, out E>`,
`List<out T>`, `Fiber<out T>`, `Map<K, out V>` (keys invariant); `Channel<T>`
and `Ptr` are invariant. Function types are structurally contravariant in
parameters and covariant in returns.

## Built-in Types

Primitive spellings are case-sensitive.

| Type             | Description                                                        |
| ---------------- | ------------------------------------------------------------------ |
| `int`            | 64-bit signed integer (LLVM `i64`)                                 |
| `float`          | 64-bit IEEE 754 (LLVM `double`)                                    |
| `string`         | UTF-8 encoded                                                      |
| `bool`           | `true` \| `false`                                                  |
| `Unit`           | The single value `()`; the return type of a function with no result|
| `any`            | Erased compatibility value; no runtime type tests                  |
| `Result<T, E>`   | Error-handling sum type (see [Error Handling](0013-ErrorHandling.md)) |
| `List<T>`        | Immutable sequential collection                                    |
| `Map<K, V>`      | Immutable key/value collection                                     |
| `Iterator<T>`    | Opaque range pipeline (see [Iterators](0010-LoopConstructsAndFunctionalIterators.md)) |

Mixed numeric arithmetic promotes `int` to `float`. Integer `+`, `-`, `*`, and
unary `-` return `Result<int, MathError>`; `/` and `%` return
`Result<_, MathError>`. Floating-point `+`, `-`, `*`, and unary `-` return plain
`float` ([ARITH-CHECKED](0013-ErrorHandling.md#arithmetic-and-result--arith-checked)).

## Result Preservation

A fallible expression has type `Result<T, E>`, and the compiler never
implicitly erases that wrapper
([FAILURE-EXPLICIT](0001-Introduction.md#failure-safety--failure-explicit)).
Every consuming position — arguments, bindings, plain-`T` returns, comparisons,
function-value calls — preserves the `Result` or is rejected; interpolation
displays the complete `Success` or `Error` value. Callers obtain the payload
only through an exhaustive `match` or an explicit `?:` fallback. The sole
compositional exception is failure-preserving arithmetic chaining
([Chaining Arithmetic](0013-ErrorHandling.md#chaining-arithmetic)), which
flattens compatible `Result<T, MathError>` chains to one `Result` and never
yields a plain number.

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

let doubler: (int) -> Result<int, MathError> = fn(x: int) => x * 2

fn createAdder(n: int) -> (int) -> Result<int, MathError> = fn(x: int) => x + n
```

```osprey-ml
applyFunction : (int, (int) -> int) -> int
applyFunction (value, transform) = transform value

doubler : int -> Result<int, MathError>
doubler = \x => x * 2

createAdder : int -> int -> Result<int, MathError>
createAdder n = \x => x + n
```

Multi-argument call syntax (named arguments are required for two or more parameters) is in [Function Calls](0005-FunctionCalls.md).

### Closures — [TYPE-FN-CLOSURE]

A lambda (`fn(...) => expr` or `|x| => expr`) captures every free identifier from its enclosing lexical scope by reference to its value at capture time. Captured bindings are immutable, so by-reference and by-value capture are observationally identical and the implementation MAY choose either. A captured binding outlives the surrounding stack frame: a closure returned from a function remains callable and continues to read the captured values.

```osprey
fn makeAdder(n: int) -> (int) -> Result<int, MathError> = fn(x: int) => x + n

let add5    = makeAdder(5)
let add10   = makeAdder(10)
print(add5(3))     // Success(8)
print(add10(3))    // Success(13)

let prefix  = "hello "
let greet   = fn(name: string) => prefix + name              // captures prefix
print(greet("world"))                                         // "hello world"
```

```osprey-ml
makeAdder : int -> (int) -> Result<int, MathError>
makeAdder n = \(x : int) => x + n               // captures n

add5    = makeAdder 5
add10   = makeAdder 10
print (add5 3)     // Success(8)
print (add10 3)    // Success(13)

prefix  = "hello "
greet   = \(name : string) => prefix + name     // captures prefix
print (greet "world")                                         // "hello world"
```

Closures and named functions are interchangeable wherever their complete
function types match, including iterator callbacks and record fields. A
`Result<T, E>` returned through a function-value call remains a `Result<T, E>`
and must be handled explicitly ([Result Preservation](#result-preservation)).

### Higher-order calls — [TYPE-FN-HIGHER-ORDER]

Any expression with a function type is callable. The callee may be a local,
record field, returned closure, or another call expression; it need not be a
top-level function name. Chained application evaluates one function result per
call, so `makeAdder(1)(2)` calls the closure returned by `makeAdder(1)`.

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

### Field Access — [TYPE-FIELD-ACCESS-NON-RECORD]

Direct field access is permitted only on a record value. A `Result` or union
must be matched to a concrete payload before field access. Because `any` has no
runtime type tag, it cannot be narrowed for field access.

Field access on a type that can never carry fields — `int`, `float`, `string`,
`bool`, `Unit` — is rejected by the type checker with
`cannot access field '<field>' on non-struct type <type>`, naming the offending
source line. The check is deliberately narrow: `any` unifies with records, a
collection's element may be a record, and an unresolved type variable may still
infer to one, so none of those are rejected here. Without the check, codegen
emitted invalid LLVM and the failure surfaced from `clang` against a temporary
`.ll` file instead of the user's source.

```osprey
let n = person.name        // ok

// Result: match before access
match personResult {
    Success { value }   => print(value.name)
    Error   { message } => print(message)
}

// Union: discriminate first
let area = match shape {
    Circle    { radius }         => 3.14 * radius * radius
    Rectangle { width, height }  => (width * height) ?: 0
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
        Rectangle width height => (width * height) ?: 0
```

Codegen resolves a **named-field** payload by name, never by declaration order, so reordering fields in a `type` cannot silently rebind a pattern. A **positionally-declared** variant ([TYPE-UNION-POSITIONAL](0003-Syntax.md#type-declarations)) has no field names to resolve against and is the one case resolved by index — the binder in column *i* binds payload slot *i*.

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

A union type (also "sum type", "tagged union", "discriminated union") declares a closed set of named variants. Each variant is nullary (no payload), carries a record-style named payload, or carries a positional payload ([TYPE-UNION-POSITIONAL](0003-Syntax.md#type-declarations)). Grammar in [Syntax](0003-Syntax.md#type-declarations); pattern-matching rules in [Pattern Matching](0007-PatternMatching.md).

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

A variant's payload MAY reference the union type itself, either directly or through a built-in collection. Recursive payloads represent trees such as ASTs, file trees, scene graphs, and parsed JSON.

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

A recursive union is laid out indirectly — variant payloads referencing the same type, or containing a `List<Self>` / `Map<K, Self>`, MUST be stored behind a pointer so the type's size is finite. Construction, pattern-matching, and field access use the same syntax as other variants. Mutually recursive unions follow the same rule.

## Collection Types

`List<T>` and `Map<K, V>` are immutable runtime collections. Collection
operations return a new value and leave their inputs unchanged. Their builtin
signatures are listed in [Built-in Functions](0012-Built-InFunctions.md#collection-functions).

### `List<T>` — [TYPE-LIST]

`List<T>` is a homogeneous indexed sequence. Index access is bounds-checked
and returns `Result<T, Error>`.

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
let withSix  = listAppend(numbers, 6)
let reversed = listReverse(numbers)
let combined = numbers + [6, 7, 8]
forEachList(numbers, fn(x) => print(toString(x)))
```

`+` is equivalent to `listConcat`. `listAppend`, `listPrepend`,
`listReverse`, and concatenation return new lists.

#### Patterns — [TYPE-LIST-PATTERNS]

```osprey
fn classify(xs: List<int>) -> string = match xs {
    []                 => "empty"
    [single]           => "one"
    [first, second]    => "two"
    [head, ...tail]    => "many starting with ${head}"
}
```

A list pattern matches exactly the listed length unless its final element is a
rest binder (`...name`). The rest binder receives the remaining `List<T>`.

### `Map<K, V>` — [TYPE-MAP]

`Map<K, V>` is an associative collection. The constructors and map
literals create string-keyed maps, so their concrete public type is
`Map<string, V>`. Iteration order is unspecified.

#### Literals — [TYPE-MAP-LITERAL]

```osprey
let ages = {
    "Alice":   25,
    "Bob":     30,
    "Charlie": 35
}                                                 // Map<string, int>
```

The ML spelling is `["Alice" => 25, "Bob" => 30]`. Use `Map()` in Default
syntax or `[=>]` in ML syntax for an empty map.

```osprey
let scores = Map()
```

Entries are inserted left to right; the last value wins when a literal repeats
a key.

#### Lookup — [TYPE-MAP-LOOKUP]

Index lookup returns `Result<V, Error>`:

```osprey
match ages["Alice"] {
    Success { value }   => print(toString(value))
    Error   { message } => print(message)
}
```

#### Operations — [TYPE-MAP-OPS]

All operations return a new map and never mutate the receiver.

```osprey
let updated    = mapSet(ages, "Alice", 26)
let withoutBob = mapRemove(ages, "Bob")
let merged     = ages + { "Dave": 28 }
let names      = mapKeys(ages)
let values     = mapValues(ages)
```

`mapMerge` and map `+` are right-biased: the right map wins on duplicate keys.

## Built-in Error Types

| Type        | Used by |
| ----------- | ------- |
| `MathError` | Checked numeric operators and `abs` |
| `Error`     | Fallible builtins, including parsing, checked arithmetic, collection lookup, files, and processes |

`Success` and `Error` are the constructors of `Result<T, E>` (see [Error Handling](0013-ErrorHandling.md)).

## The `any` Type — [TYPE-ANY]

`any` is an erased compatibility type. It unifies with every other type, so an
`any` parameter accepts values of different static types:

```osprey
fn ignore(value: any) -> string = "ignored"

let a = ignore(42)
let b = ignore("text")
```

`any` does not carry a runtime type tag and does not provide dynamic type tests.
Code that consumes its representation must already know what was passed. It is
used mainly at heterogeneous builtin and foreign-function boundaries. In
particular, `print` and `toString` cannot recover an aggregate hidden behind
`any`; they render its raw pointer-sized representation rather than its fields.

## Type Annotations — [TYPE-ANNOTATION-CHECK]

An annotation constrains inference and is checked against the expression. A
primitive spelling is case-sensitive, so `Int` is not `int`: an unknown
capitalized name is a nominal type, and assigning an `int` to a variable
annotated `Int` is a type mismatch rather than a silent alias.

```osprey
let xs: List<int> = []
fn half(n: int) -> Result<int, Error> = intDiv(n, 2)
```

Writing `-> int` for `half` would be a type error; a return annotation cannot
erase the body's `Result` ([Result Preservation](#result-preservation)).
