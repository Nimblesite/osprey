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

- Arithmetic: `+`, `-`, `*`, `/`, `%`. `+`, `-`, and `*` return plain numbers;
  `/` and `%` return a `Result` and reject a zero divisor ([ARITH-PLAIN]).
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
defined by [PATTERN-RESULT-DEFAULT](0007-PatternMatching.md#result-default--pattern-result-default).

## Comments

Default uses `//` line comments and `///` documentation comments. ML accepts
`//` line comments, nested `(* ... *)` block comments, and `(** ... *)`
documentation comments.
