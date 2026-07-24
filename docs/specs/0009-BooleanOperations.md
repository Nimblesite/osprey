# Boolean Operations

Osprey has no `if`/`else` statement. Conditional logic is written as a `match` on a boolean (which forces both arms to be considered) or as the ternary shorthand `cond ? then : else`, which desugars to the same `match`. The ternary is defined in [Pattern Matching](0007-PatternMatching.md#ternary-match-syntactic-sugar).

> **Flavor layer — shared core.** `&&`, `||`, and comparisons lower to
> `Expr::Binary`; `!` lowers to `Expr::Unary`; conditionals lower to
> `Expr::Match`. Default braces and the
> [ML offside form](0024-MLFlavorSyntax.md) share these semantics.

```osprey
let status = match isValid {
    true  => "Success"
    false => "Failure"
}

let max = match a > b {
    true  => a
    false => b
}
```

```osprey-ml
status = match isValid
    true  => "Success"
    false => "Failure"

max = match a > b
    true  => a
    false => b
```

Nested matches handle compound conditions:

```osprey
let category = match score >= 90 {
    true  => match score == 100 {
        true  => "Perfect"
        false => "Excellent"
    }
    false => match score >= 70 {
        true  => "Good"
        false => "Needs Improvement"
    }
}
```

```osprey-ml
category = match score >= 90
    true  => match score == 100
        true  => "Perfect"
        false => "Excellent"
    false => match score >= 70
        true  => "Good"
        false => "Needs Improvement"
```

## Boolean Operators

`&&`, `||`, and `!` are short-circuiting; `==`, `!=`, `<`, `>`, `<=`, `>=` produce booleans. See [Lexical Structure](0002-LexicalStructure.md) for the full operator list.

```osprey
let isAdult       = age >= 18
let hasPermission = isAdult && isAuthorized
let canAccess     = hasPermission || isAdmin
let isBlocked     = !isActive
let validUser     = !isBanned && (isVerified || hasInvite)
```

```osprey-ml
isAdult       = age >= 18
hasPermission = isAdult && isAuthorized
canAccess     = hasPermission || isAdmin
isBlocked     = !isActive
validUser     = !isBanned && (isVerified || hasInvite)
```
