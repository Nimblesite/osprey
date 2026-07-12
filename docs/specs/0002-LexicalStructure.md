# Lexical Structure

- [Identifiers](#identifiers)
- [Keywords](#keywords)
- [Literals](#literals)
- [Operators](#operators)
- [Delimiters](#delimiters)

> **Flavor layer — surface (CST).**  Lexing is a flavor-internal, below-the-AST concern: tokens are a CST artifact that never reach the shared core, which sees only the canonical `osprey_ast::Program` after lowering, so the semantics are flavor-blind at the [FLAVOR-BOUNDARY]. This chapter shows BOTH flavors: the Default (`.osp`) spelling — whose lexical grammar is owned by `crates/osprey-syntax/src/default/` — and, where the surface differs, the ML (`.ospml`) twin shown inline alongside it (`osprey-ml` blocks). The ML flavor has its OWN offside-rule layout lexer (`crates/osprey-syntax/src/ml/lexer.rs`) that derives `INDENT`/`DEDENT`/`NEWLINE` from an explicit indent stack ([FLAVOR-ML-LAYOUT] in [ML Flavor Syntax](0024-MLFlavorSyntax.md)). Lexical structure differs per flavor; both feed lowering into the one shared core. See [Language Flavors](0023-LanguageFlavors.md).

## Identifiers

Start with letter or underscore, followed by letters, digits, or underscores.
```
ID := [a-zA-Z_][a-zA-Z0-9_]*
```

## Keywords

```
fn let mut type match extern import module where
effect perform handle in resume
spawn await yield select
```

**Contextual keywords.** `out` and `in` mark variance inside a
type-parameter list only (`type Source<out T>`, `effect Emit<in T>` —
[TYPE-VARIANCE-DECL](0004-TypeSystem.md#generics-and-variance)). `out` is an
ordinary identifier everywhere else; `in` remains the hard keyword of
`handle … in` and is merely *accepted* in type-parameter position.

## Literals

### Integer Literals
```
INTEGER := [0-9]+
```

**Examples:**
```osprey
let count = 42
let negative = -17
let zero = 0
```

```osprey-ml
count = 42
negative = -17
zero = 0
```

### Float Literals
```
FLOAT := [0-9]+ '.' [0-9]+ ([eE] [+-]? [0-9]+)?
       | [0-9]+ [eE] [+-]? [0-9]+
```

**Examples:**
```osprey
let pi = 3.14159
let temperature = -273.15
let scientific = 6.022e23
let small = 1.5e-10
```

```osprey-ml
pi = 3.14159
temperature = -273.15
scientific = 6.022e23
small = 1.5e-10
```

**Type Inference:**
- Integer literals without decimal point infer to `int`
- Literals with decimal point or scientific notation infer to `float`

### String Literals
```
STRING := '"' (CHAR | ESCAPE_SEQUENCE)* '"'
ESCAPE_SEQUENCE := '\n' | '\t' | '\r' | '\\' | '\"'
```

### Interpolated String Literals
```
INTERPOLATED_STRING := '"' (CHAR | INTERPOLATION)* '"'
INTERPOLATION := '${' EXPRESSION '}'
```

### List Literals
```
LIST := '[' (expression (',' expression)*)? ']'
```

```osprey
let numbers = [1, 2, 3, 4]
let names   = ["Alice", "Bob", "Charlie"]
let pair    = [x, y]
```

```osprey-ml
numbers = [1, 2, 3, 4]
names   = ["Alice", "Bob", "Charlie"]
pair    = [x, y]
```

## Operators

### Arithmetic Operators

`+`, `-`, `*`, `/`, `%`. All arithmetic returns `Result`; full signatures and semantics are in [Error Handling](0013-ErrorHandling.md).

### Comparison Operators
- `==` Equality
- `!=` Inequality  
- `<` Less than
- `>` Greater than
- `<=` Less than or equal
- `>=` Greater than or equal

### Logical Operators
- `&&` Logical AND (short-circuit evaluation)
- `||` Logical OR (short-circuit evaluation)
- `!` Logical NOT

### Assignment Operator
- `=` Assignment

### Other Operators
- `->` Function return type
- `=>` Lambda body and match arm
- `|` Union type separator
- `|>` Pipe
- `!` Effect-set marker on a function type

## Delimiters

- `(` `)` Parentheses
- `{` `}` Braces
- `[` `]` Brackets
- `,` Comma
- `:` Colon
- `;` Semicolon
- `.` Dot