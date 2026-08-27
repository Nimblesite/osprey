# Lexical Structure

Default source is tokenized by `tree-sitter-osprey`; ML source uses the lexer in
`crates/osprey-syntax/src/ml/lexer.rs`, which also inserts `INDENT`, `DEDENT`,
and `NEWLINE` tokens. Both frontends share string unescaping and interpolation
scanning.

## Identifiers

Default identifiers use ASCII letters and underscores:

```ebnf
identifier ::= [a-zA-Z_][a-zA-Z0-9_]*
```

The ML lexer accepts Unicode alphabetic characters in the same leading and
continuation positions, plus `_`; digits remain continuation-only.

## Keywords

The Default grammar reserves the words used by its declarations and
expressions, including `let`, `mut`, `fn`, `extern`, `type`, `where`, `effect`,
`perform`, `handle`, `in`, `resume`, `spawn`, `await`, `yield`, `send`, `recv`,
`select`, `match`, `if`, `else`, `import`, `namespace`, `module`, `signature`,
`export`, `opaque`, `state`, `as`, `true`, and `false`.

ML reserves the corresponding words its surface uses. It deliberately does not
reserve Default-only `let`, `fn`, `if`, or `else`. `handler` and `do` are
reserved in ML solely so the frontend can reject the unsupported first-class
handler syntax with a specific diagnostic.

`in` and `out` are variance markers inside type-parameter declarations
([TYPE-VARIANCE-DECL](0004-TypeSystem.md#generics-and-variance)). Outside that
position, `out` is an identifier; `in` also separates a handler from its body.

## Literals

### Numbers

```ebnf
integer ::= [0-9]+
float   ::= [0-9]+ "." [0-9]+
```

Negative values apply unary `-`; the sign is not part of the token. Integer
literals infer as `int` and decimal literals as `float`. Exponent notation,
numeric separators, and non-decimal bases are not accepted.

### Booleans and strings

Boolean literals are `true` and `false`. Strings are double-quoted. Shared
unescaping recognizes `\n`, `\r`, `\t`, `\e`, `\0`, `\"`, and `\\`; an
unknown escape is preserved verbatim. Default strings may contain a source
newline. The ML lexer rejects a source newline in string text, but permits one
inside an interpolation expression.

`${ expression }` introduces interpolation in either flavor. See
[String Interpolation](0006-StringInterpolation.md).

### Lists

```ebnf
list ::= "[" (expression ("," expression)*)? "]"
```

The list elements must unify to one element type. Map and record literal forms
are defined in [Type System](0004-TypeSystem.md).

## Operators

- Arithmetic: `+`, `-`, `*`, `/`, `%`. Integer `+`, `-`, and `*`, integer
  unary `-`, and `/` and `%` return a checked `Result`; floating-point `+`,
  `-`, `*`, and unary `-` remain plain IEEE-754 operations
  ([ARITH-CHECKED](0013-ErrorHandling.md#arithmetic-and-result--arith-checked)).
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`.
- Boolean: `&&`, `||`, `!`. `&&` and `||` short-circuit.
- Calls and data access: `()`, `.`, `[]`.
- Flow and matching: `|>`, `=>`, `? :`, `?:`.
- Types and effects: `:`, `->`, `< >`, and `!` on an effect set.
- Namespace qualification: `::`.

Default uses `=` both to initialize a `let`/`mut` declaration and to reassign a
previously declared `mut` binding. ML uses `=` for a binding and `:=` for
reassignment. A `mut` binding is a cell for **handler-owned effect state**, not a
general imperative variable; where its reassignment is valid is defined in
[Bindings](0003-Syntax.md#bindings). The checker accepts reassignment only in an
effect handler arm.

`?:` is a single token and is matched before bare `?` or `:`. Its semantics are
defined by [PATTERN-RESULT-DEFAULT](0007-PatternMatching.md#result-default---pattern-result-default).

## Statement Boundaries

Default has no statement terminator: a statement ends where its line ends. The
grammar spells that as a zero-width `_statement_break` token, produced by the
external scanner in `tree-sitter-osprey/src/scanner.c`.

### The rule [LEX-STATEMENT-BREAK]

A statement ends at the first newline, `//` comment, `}` closing its block or
namespace body, or end of file that follows it. Two statements on one line are a
syntax error, because nothing would separate them:

```osprey
let r = add 2 3     // rejected: Default has no juxtaposition application
print("a") print("b")   // rejected: two statements, one line
```

Without this rule nothing delimited a statement, and the greedy parse chose the
boundary silently. `let r = add 2 3` became `let r = add` followed by the orphan
expression-statements `2` and `3`; inside a block the orphan was absorbed by the
trailing block value, so `{ let r = double 5 }` evaluated to the **argument** 5
rather than applying `double`. Both are well-formed trees for source that means
a call, which is why the boundary is now a lexical decision rather than an
accident of precedence.

A line whose first token can only continue the preceding expression does not
begin a statement, so an expression may still be laid out across lines:

```osprey
let total = buffer
    |> gpuMap(scale)
    |> gpuMap(clamp)

type Json = JNull
    | JBool { value: bool }
    | JNum { value: int }
```

The continuing tokens are `*`, `%`, `/`, `<`, `>`, `?`, `:`, `.`, `|`, `|>`,
`||`, `&&`, `==`, `!=`, `<=`, `>=`, and `::`. `+`, `-`, and `!` are deliberately
absent: each has a prefix reading, so a line opening with one starts a new
statement whose discarded value is then reported by
[BLOCK-DISCARD](0008-BlockExpressions.md#discarded-values--block-discard). Write
a continued sum with the operator trailing the first line.

ML needs no such marker. Its lexer emits explicit `NEWLINE`, `INDENT`, and
`DEDENT` tokens, so layout already delimits its statements.

## Comments

Default uses `//` line comments and `///` documentation comments. ML accepts
`//` line comments, nested `(* ... *)` block comments, and `(** ... *)`
documentation comments.
