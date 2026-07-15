---
layout: page
title: "Built-in Functions"
description: "Osprey Language Specification: Built-in Functions"
date: 2026-07-15
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0012-built-infunctions/"
---

# Built-in Functions

Reference for built-in functions available in every Osprey program. Operations that can fail return `Result`; see [Error Handling](/spec/0013-errorhandling/).

> **Flavor layer — shared core (AST and above).**  The built-in function set is shared core: the *same* functions exist in every flavor, and a call to any of them lowers to the canonical `Expr::Call` node regardless of source surface. Only the call *spelling* is a flavor concern — the Default surface writes `toString(x)`, the ML flavor uses whitespace application `toString x` — and that difference is erased at lowering, so nothing here depends on which flavor produced the program. The Default spelling is shown throughout; see [Language Flavors](/spec/0023-languageflavors/) and [ML Flavor Syntax](/spec/0024-mlflavorsyntax/) for the surface mapping.

## Basic I/O Functions

```osprey
print(value: int | string | bool) -> int
```
Prints values to standard output with automatic type conversion.

```osprey
print("Hello World")
print(42)
print(true)
```

```osprey-ml
print "Hello World"
print 42
print true
```

### `input() -> string` — [BUILTIN-INPUT]
Reads one line from standard input (without its trailing newline) and returns it
as a string. At end-of-file — including when stdin is empty or not connected —
it returns the empty string `""` rather than blocking or failing. Parse it with
`parseInt`/`parseFloat` when a number is wanted.

```osprey
let line = input()                 // "" if there is no input
let n    = parseInt(input()) ?: 0  // a number, or 0 when absent/unparseable
```

```osprey-ml
line = input ()                    // "" if there is no input
n =
    match parseInt (input ())      // a number, or 0 when absent/unparseable
        Success value => value
        Error _       => 0
```

### `toString(value: int | string | bool) -> string`
Converts any value to its string representation.

## Testing Functions — [TESTING-BUILTINS]

The built-in testing framework. Normative rules — TAP output, exit codes,
filtering, discovery, the `osprey test` runner, and the VS Code Test Explorer
— live in [Testing Framework](/spec/0027-testingframework/); the three functions
are listed here for completeness.

### `test(name: string, body: fn() -> Unit) -> Unit` — [TESTING-BUILTIN-TEST]
Runs `body` as one named test case and prints a TAP result line. A program
that uses any testing built-in exits non-zero when a case failed.

### `expect(actual: any, expected: any) -> Unit` — [TESTING-BUILTIN-EXPECT]
Equality assertion, Jest argument order. Canonical-string equality with
`Result` auto-unwrap; a mismatch marks the enclosing case failed and prints a
diagnostic without aborting the case.

### `check(label: string, expected: any, actual: any) -> Unit` — [TESTING-BUILTIN-CHECK]
Labeled equality assertion, Alcotest argument order (expected before actual).

```osprey
fn add(a, b) = a + b
test("addition works", fn() => expect(add(2, 3), 5))
```

```osprey-ml
add (a, b) = a + b
test "addition works" (\() => check "sum" 5 (add (2, 3)))
```

Unlike other built-ins, these three names are shadowable: a user-defined
`test`/`expect`/`check` function replaces the built-in
([TESTING-SHADOWING]).

## Numeric Functions

### `abs(n: int) -> int`
Absolute value of an integer.

### `intDiv(a: int, b: int) -> Result<int, MathError>` — [BUILTIN-INTDIV]
Truncating integer division (rounds toward zero), divide-by-zero checked. The
`/` operator is **float-only** by the [Type System](/spec/0004-typesystem/) spec
(`int / int` promotes to `float`); `intDiv` is its integer sibling. A zero
divisor returns `Error(MathError)`; otherwise `Success(quotient)`. Like the
`+ - * %` operators, the `Success` payload auto-unwraps at value sites
(interpolation, comparison, arguments, `Result`-typed function returns).

```osprey
intDiv(7, 2)        // Success(3)
intDiv(255643, 10)  // Success(25564)
intDiv(5, 0)        // Error(MathError) — "division by zero"
fn half(n) = intDiv(n, 2)   // -> int (Result auto-unwraps at the typed return)
```

```osprey-ml
intDiv (7, 2)        // Success(3)
intDiv (255643, 10)  // Success(25564)
intDiv (5, 0)        // Error(MathError) — "division by zero"
half n = intDiv (n, 2)   // -> int (Result auto-unwraps at the typed return)
```

### `random() -> int` — [BUILTIN-RANDOM]
A cryptographically-secure uniform random non-negative integer in `[0, 2^63-1]`,
drawn fresh from the operating system's CSPRNG (`arc4random_buf` on macOS/BSD,
`getrandom(2)` on Linux, falling back to `/dev/urandom`). It carries no userspace
seed or state, so the stream is unpredictable and never reproducible — suitable
for security-sensitive use as well as randomized inputs.

```osprey
let token = random()        // e.g. 7240982340198 (varies every call)
fn coinFlip() = randomBelow(2) ?: 0   // 0 or 1
```

```osprey-ml
token = random ()        // e.g. 7240982340198 (varies every call)
coinFlip () =
    match randomBelow 2   // 0 or 1
        Success value => value
        Error _       => 0
```

### `randomBelow(n: int) -> Result<int, MathError>` — [BUILTIN-RANDOM-BELOW]
A cryptographically-secure uniform random integer in the half-open range
`[0, n)`. The result is **unbiased**: it is drawn by rejection sampling, so every
value in the range is equally likely (a plain `random() % n` is not). A
non-positive `n` returns `Error(MathError)`; otherwise `Success(value)` with
`0 <= value < n`. Compose for an arbitrary range: `lo + (randomBelow(hi - lo) ?: 0)`.

```osprey
let die = randomBelow(6) ?: 0          // a fair face 0..5
match randomBelow(0) { Success { value } => value  Error { message } => 0 - 1 }  // Error
```

```osprey-ml
die =
    match randomBelow 6           // a fair face 0..5
        Success value => value
        Error _       => 0
match randomBelow 0
    Success value => value
    Error message => 0 - 1       // Error
```

## String Functions

Strings are immutable UTF-8 sequences. Every function listed here is **pure**: it returns a new value and never mutates its arguments.

### Design Principles

These rules govern the entire string API. They are drawn from idiomatic FP string libraries — primarily Elm's `String` module and Haskell's `Data.Text` — and adapted to Osprey's `Result`-only error model (Osprey has no `Maybe`/`Option`; see [Error Handling](/spec/0013-errorhandling/)).

1. **Total functions return plain values.** Operations that cannot fail on any well-formed UTF-8 input (e.g. `length`, `toUpperCase`, `trim`, `contains`) return their result directly. They do **not** wrap in `Result`. This matches Elm (`String.length : String -> Int`) and Haskell (`Data.Text.length :: Text -> Int`).
2. **Partial functions return `Result<T, StringError>`.** Operations with inputs that can be invalid (`substring` with out-of-range indices, `parseInt` on non-numeric input, `split` with an empty separator) return `Result`.
3. **Subject-first argument order.** The string being operated on is the first parameter, enabling `myString |> trim |> toLowerCase` with the pipe operator (see [Iterators](/spec/0010-loopconstructsandfunctionaliterators/)).
4. **No silent Unicode surprises (target behaviour).** Case conversion follows Unicode simple case mapping; lengths and indices are codepoint counts, not byte counts. This matches Haskell `Data.Text` and Elm `String`. **Implementation status:** the v1 runtime counts bytes (`strlen`-based) and uses ASCII-only `tolower`/`toupper`. UTF-8-aware rewrites build on the cursor primitives in [Cursor Access](#cursor-access-total-o1--builtin-string-cursor) ([BUILTIN-STRING-CURSOR]), which have shipped.
5. **No character (`Char`) type yet.** Higher-order operations over individual characters (`map`, `filter`, `foldl`, `any`, `all`) are intentionally **deferred** until Osprey introduces a `Char` type.

### Calling Style

String functions can be called three ways. **Pipe (`|>`) is the preferred form** and the one used throughout this document.

```osprey
// Preferred — pipe chain, reads top-to-bottom
"  Hello, World  " |> trim |> toLowerCase |> split(", ")

// Direct call — fine for single operations
toLowerCase(trim("  Hello  "))

// Method-call (UFCS) — sugar, equivalent to the direct form
"  Hello  ".trim().toLowerCase()
```

```osprey-ml
// Preferred — pipe chain, reads top-to-bottom
"  Hello, World  " |> trim |> toLowerCase |> split ", "

// Direct call — fine for single operations
toLowerCase (trim "  Hello  ")

// Chained UFCS (`.trim().toLowerCase()`) has no ML surface; use the pipe form:
"  Hello  " |> trim |> toLowerCase
```

All three desugar to the same call. Rules:

- **Pipe (`x |> f`)** rewrites to `f(x)`. With extra args, `x |> f(a, b)` becomes `f(x, a, b)`. A bare identifier on the right (`x |> f`) is auto-promoted to a call — no parens needed for single-arg functions. See [Iterators](/spec/0010-loopconstructsandfunctionaliterators/#pipe-operator).
- **UFCS (`x.f(args)`)** rewrites to `f(x, args)`. **Parens are required** to disambiguate from field access — `x.f` always means field access, never a method call. If a record has a field named `f`, field access wins; UFCS is the fallback.
- **Direct call** is plain function application; nothing magic.

Multi-argument functions in this spec are documented subject-first (e.g. `split(s: string, separator: string)`) so all three forms work uniformly.

### `StringError`

```osprey
type StringError =
    | IndexOutOfRange { index: int, length: int }
    | InvalidArgument { message: string }
    | NotFound
    | ParseFailed { input: string }
```

### Inspection (total)

#### `length(s: string) -> int`
Returns the number of Unicode codepoints. `length("héllo") == 5`.

#### `isEmpty(s: string) -> bool`
True iff `length(s) == 0`. Equivalent to `length(s) == 0` but constant-time.

### Search (total)

#### `contains(s: string, needle: string) -> bool`
True if `needle` occurs anywhere in `s`. An empty `needle` returns `true` (matches every position; consistent with Elm and Java).

```osprey
contains("hello world", "world")  // true
contains("hello", "")             // true
```

```osprey-ml
contains ("hello world", "world")  // true
contains ("hello", "")             // true
```

#### `startsWith(s: string, prefix: string) -> bool`
#### `endsWith(s: string, suffix: string) -> bool`

```osprey
"GET /api/users" |> startsWith("GET ")   // true
"image.png"      |> endsWith(".png")     // true
```

```osprey-ml
"GET /api/users" |> startsWith "GET "   // true
"image.png"      |> endsWith ".png"     // true
```

#### `indexOf(s: string, needle: string) -> Result<int, StringError>`
Returns the codepoint index of the first occurrence of `needle`, or `Error(NotFound)` if absent. An empty `needle` returns `Success { value: 0 }`.

### Cursor Access (total, O(1)) — [BUILTIN-STRING-CURSOR]

These primitives expose `string` as a random-access byte/codepoint buffer without allocating. They exist so user-written parsers (JSON, query strings, CSV, log formats) can run in linear time instead of the O(n²) imposed by chaining `substring`/`take`/`drop`. They are the lowest-level string operations in the language; everything above is implementable in pure Osprey on top of them.

#### `byteLength(s: string) -> int`
Byte length of the underlying UTF-8 storage. Equal to `length(s)` only for ASCII strings. O(1).

#### `byteAt(s: string, i: int) -> Result<int, StringError>`
Returns the UTF-8 byte at index `i` as an `int` in `[0, 255]`, or `Error(IndexOutOfRange)` if `i < 0` or `i >= byteLength(s)`. O(1). Does **not** allocate.

#### `codePointAt(s: string, byteIndex: int) -> Result<int, StringError>`
Decodes the UTF-8 codepoint starting at `byteIndex` and returns it as an `int`. Returns `Error(IndexOutOfRange)` if `byteIndex` is out of range, or `Error(InvalidArgument)` if it does not land on a codepoint boundary or the bytes are malformed. O(1) (at most 4 bytes read). Pair with `codePointWidth` to advance:

```osprey
type CharStep = { codePoint: int, nextIndex: int }

fn nextChar(s, i) = match codePointAt(s, i) {
    Success { value: cp } => match codePointWidth(cp) {
        Success { value: w } => Success { value: CharStep { codePoint: cp, nextIndex: i + w } }
        Error   { message }  => Error { message }
    }
    Error { message } => Error { message }
}
```

```osprey-ml
type CharStep =
    codePoint : int
    nextIndex : int

nextChar (s, i) =
    match codePointAt (s, i)
        Success cp =>
            match codePointWidth cp
                Success w => Success(value = CharStep(codePoint = cp, nextIndex = i + w))
                Error message => Error(message = message)
        Error message => Error(message = message)
```

#### `codePointWidth(codepoint: int) -> Result<int, StringError>`
Returns the number of UTF-8 bytes the codepoint encodes to (1–4), or `Error(InvalidArgument)` if `codepoint` is not a valid Unicode scalar value.

#### `fromCodePoint(codepoint: int) -> Result<string, StringError>`
Builds a single-codepoint `string`. Inverse of `codePointAt`. `Error(InvalidArgument)` for invalid scalar values.

### Substrings

#### `substring(s: string, start: int, end: int) -> Result<string, StringError>`
Extracts codepoints in `[start, end)`. Returns `Error(IndexOutOfRange)` if `start < 0`, `end > length(s)`, or `start > end`.

#### `take(s: string, n: int) -> string`
Returns at most the first `n` codepoints. If `n <= 0`, returns `""`; if `n >= length(s)`, returns `s`. **Never fails** — clamping mirrors Elm `String.left`.

#### `drop(s: string, n: int) -> string`
Returns `s` without its first `n` codepoints, with the same clamping rules as `take`. Mirrors Elm `String.dropLeft`.

### Splitting and Joining

#### `split(s: string, separator: string) -> Result<List<string>, StringError>`
Splits `s` on every occurrence of `separator`. Returns `Error(InvalidArgument)` if `separator` is empty — matching Haskell `Data.Text.splitOn`, which rejects empty separators because the result would be ambiguous.

```osprey
match split("a,b,c", ",") {
    Success { value }   => forEach(value, print)   // "a" "b" "c"
    Error   { message } => print("split error")
}
```

```osprey-ml
match split ("a,b,c", ",")
    Success value   => forEach (value, print)   // "a" "b" "c"
    Error message   => print "split error"
```

#### `join(parts: List<string>, separator: string) -> string`
Concatenates `parts` with `separator` between each pair. Returns `""` if `parts` is empty.

#### `lines(s: string) -> List<string>`
Splits on `"\n"`. A trailing newline does not produce an empty final element (matches Haskell `Data.Text.lines`).

#### `words(s: string) -> List<string>`
Splits on runs of Unicode whitespace, dropping empty results.

### Transformation (total)

#### `toUpperCase(s: string) -> string`
#### `toLowerCase(s: string) -> string`
Unicode simple case mapping. May change codepoint length (e.g. German `ß` → `SS`); this is intentional and matches Haskell `Data.Text.toUpper`/`toLower`.

#### `trim(s: string) -> string`
#### `trimStart(s: string) -> string`
#### `trimEnd(s: string) -> string`
Remove leading/trailing/both runs of Unicode whitespace (per the Unicode `White_Space` property, matching Rust's `str::trim`).

#### `replace(s: string, needle: string, replacement: string) -> Result<string, StringError>`
Replaces **every** occurrence of `needle` with `replacement`. Returns `Error(InvalidArgument)` if `needle` is empty (same reasoning as `split`).

#### `repeat(s: string, n: int) -> Result<string, StringError>`
Concatenates `s` with itself `n` times. Returns `Error(InvalidArgument)` if `n < 0`. `repeat(s, 0) == ""`.

#### `reverse(s: string) -> string`
Reverses codepoint order. (Note: grapheme-cluster reversal is a future addition.)

#### `padStart(s: string, targetLength: int, fill: string) -> Result<string, StringError>`
#### `padEnd(s: string, targetLength: int, fill: string) -> Result<string, StringError>`
Pads `s` on the left/right with copies of `fill` until it reaches `targetLength` codepoints. Returns `s` unchanged if already long enough. Returns `Error(InvalidArgument)` if `fill` is empty.

### Parsing

#### `parseInt(s: string) -> Result<int, StringError>`
Parses a base-10 signed integer. Leading/trailing whitespace is rejected — callers must `trim` first. Returns `Error(ParseFailed)` on any non-numeric input (no silent zero-on-error like C's `atoi`).

#### `parseFloat(s: string) -> Result<float, StringError>`
Parses a base-10 floating-point number. Same strictness as `parseInt`.

### Concatenation Operator

The `+` operator on two `string` values returns `string` directly (not `Result`). String concatenation cannot fail.

```osprey
let greeting = "Hello, " + name + "!"
```

```osprey-ml
greeting = "Hello, " + name + "!"
```

### Example: parsing a query string

```osprey
type KeyValue = { key: string, value: string }

fn parsePair(pair) =
    match indexOf(pair, "=") {
        Success { value: i } => match substring(pair, 0, i) {
            Success { value: k } => match substring(pair, i + 1, length(pair)) {
                Success { value: v } => Success { value: KeyValue { key: k, value: v } }
                Error   { message }  => Error { message }
            }
            Error { message } => Error { message }
        }
        Error { message } => Error { message }
    }

match split("name=alice&age=30", "&") {
    Success { value: pairs } => forEach(pairs, fn(p) => parsePair(p) |> print)
    Error   { message }      => print("bad query")
}
```

### Sources

The API surface above is informed by the following FP-style string libraries:

- [Elm `String`](https://package.elm-lang.org/packages/elm/core/latest/String) — argument order, total/partial split, `take`/`drop`/`pad`/`trim` naming.
- [Haskell `Data.Text`](https://hackage.haskell.org/package/text/docs/Data-Text.html) — `splitOn` rejection of empty separators, Unicode case-mapping semantics, `lines`/`words` behaviour.
- [F# Core `String` module](https://fsharp.github.io/fsharp-core-docs/reference/fsharp-core-stringmodule.html) — pipe-friendly subject placement.
- [Elixir `String`](https://hexdocs.pm/elixir/String.html) — `trim_leading`/`trim_trailing` decomposition (adapted to `trimStart`/`trimEnd`).
- [Rust `str`](https://doc.rust-lang.org/std/primitive.str.html) — Unicode `White_Space` definition for `trim`.

## File System Functions

### `writeFile(path: string, content: string) -> Result<Success, string>`
Writes content to a file.

### `readFile(path: string) -> Result<string, string>`
Reads file content as string.

### `deleteFile(path: string) -> Result<Success, string>`
Deletes a file.

### `createDirectory(path: string) -> Result<Success, string>`
Creates a directory.

### `fileExists(path: string) -> bool`
Checks if file exists.

## Process Operations

### `spawnProcess(command: string, callback: fn(int, int, string) -> unit) -> Result<ProcessResult, string>`
Spawns an external process. The callback is invoked for each stdout/stderr line and on exit.

```osprey
fn processEventHandler(processID, eventType, data) = match eventType {
    1 => print("[STDOUT] ${data}")
    2 => print("[STDERR] ${data}")
    3 => print("[EXIT] Code: ${data}")
    _ => print("[UNKNOWN] ${data}")
}

let result = spawnProcess("echo 'Hello'", processEventHandler)
```

### `awaitProcess(processId: int) -> int`
Waits for process completion and returns the exit code.

### `cleanupProcess(processId: int) -> unit`
Releases process resources.

## Collection Functions

Reference for builtins over `List<T>` and `Map<K, V>` (defined in [Type System — Collection Types](/spec/0004-typesystem/#collection-types)). All functions are **pure** — they never mutate; "modifying" operations return a new collection that shares structure with the original. Operations that can fail return `Result`; total operations return their value directly. Subject-first argument order — the collection being operated on is the first parameter, enabling `xs |> filter(p) |> length(...)`.

### Design Principles

The collection API follows the same rules as the string API ([Design Principles](#design-principles)) and is adapted to Osprey's `Result`-only error model. In addition:

1. **Subset-matching for Map patterns.** A map pattern matches any superset of its listed entries (matches Elm and Erlang/Elixir).
2. **No iteration order for Maps.** Programs that need a deterministic order MUST sort the result of `keys` or `entries`.
3. **No `Set<T>` yet.** Use `Map<K, unit>` for set-like semantics; a first-class `Set<T>` is deferred to a future revision.

### Common (`List` and `Map`)

#### `length(list: List<T>) -> int` &nbsp; / &nbsp; `length(map: Map<K, V>) -> int`
Number of elements. Constant time on both representations.

#### `isEmpty(list: List<T>) -> bool` &nbsp; / &nbsp; `isEmpty(map: Map<K, V>) -> bool`
True iff `length` is `0`. Constant time.

### `List<T>`

Backed by an immutable bitmapped vector trie (see [TYPE-LIST](/spec/0004-typesystem/#listt--type-list)). Index access is `O(log₃₂ n)`.

#### `get(list: List<T>, index: int) -> Result<T, IndexError>`
Same as `list[index]`. Returns `Error(OutOfBounds)` if `index < 0` or `index >= length(list)`.

#### `head(list: List<T>) -> Result<T, IndexError>`
First element, or `Error(OutOfBounds)` if empty.

#### `tail(list: List<T>) -> List<T>`
All elements except the first. `tail([]) == []` (total — never errors).

#### `prepend(list: List<T>, value: T) -> List<T>`
Returns a new list with `value` at the front.

#### `append(list: List<T>, value: T) -> List<T>`
Returns a new list with `value` at the end.

#### `concat(left: List<T>, right: List<T>) -> List<T>`
Returns `left ++ right`. Same as `left + right`. `O(n + m)` for the baseline trie; `O(log n)` if upgraded to an RRB-tree.

#### `reverse(list: List<T>) -> List<T>`
New list in reverse order. `O(n)`.

#### `contains(list: List<T>, value: T) -> bool`
True iff some element of `list` is structurally equal to `value`. `O(n)`.

#### `indexOf(list: List<T>, value: T) -> Result<int, IndexError>`
First index of `value`, or `Error(NotFound)`.

### `Map<K, V>`

Backed by a HAMT with branching factor 32 (see [TYPE-MAP](/spec/0004-typesystem/#mapk-v--type-map)). Lookup/insert/remove are `O(log₃₂ n)` expected.

#### `get(map: Map<K, V>, key: K) -> Result<V, IndexError>`
Same as `map[key]`. Returns `Error(NotFound)` if `key` is absent.

#### `contains(map: Map<K, V>, key: K) -> bool`
True iff `key` is present.

#### `set(map: Map<K, V>, key: K, value: V) -> Map<K, V>`
Returns a new map with `key` bound to `value`, replacing any prior binding.

#### `remove(map: Map<K, V>, key: K) -> Map<K, V>`
Returns a new map without `key`. If `key` is absent, returns `map` (total — never errors).

#### `update(map: Map<K, V>, key: K, fn: fn(Result<V, IndexError>) -> Result<V, IndexError>) -> Map<K, V>`
Apply `fn` to the current binding (or `Error(NotFound)`). If `fn` returns `Success { value }`, the key is set; if it returns `Error(NotFound)`, the key is removed. Mirrors Elm's `Dict.update : comparable -> (Maybe v -> Maybe v) -> Dict comparable v -> Dict comparable v`.

#### `merge(left: Map<K, V>, right: Map<K, V>) -> Map<K, V>`
Right-biased union — `right` wins on key conflicts. Same as `left + right`.

#### `keys(map: Map<K, V>) -> List<K>`
All keys. Iteration order is **unspecified**.

#### `values(map: Map<K, V>) -> List<V>`
All values, in the same order as `keys(map)`.

#### `entries(map: Map<K, V>) -> List<Entry<K, V>>`
All key/value entries, in the same order as `keys(map)`. An entry is the record
`type Entry<K, V> = { key: K, value: V }` (Osprey has no tuple type, so a pair is
a two-field record, not a `(K, V)` tuple).

#### `mapValues(map: Map<K, V>, fn: fn(V) -> W) -> Map<K, W>`
Apply `fn` to every value, preserving keys.

#### `mapKeys(map: Map<K, V>, fn: fn(K) -> K2) -> Map<K2, V>`
Apply `fn` to every key. If `fn` produces duplicate keys, the **last** wins (consistent with `+`).

#### `filterEntries(map: Map<K, V>, fn: fn(K, V) -> bool) -> Map<K, V>`
Keep entries where `fn(k, v)` is true.

#### `foldEntries(map: Map<K, V>, initial: U, function: fn(U, K, V) -> U) -> U`
Reduce over entries. Iteration order is unspecified — `function` MUST be commutative if order matters.

#### `zipToMap(keys: List<K>, values: List<V>) -> Result<Map<K, V>, IndexError>`
Build a map from parallel lists. `Error(InvalidArgument)` if lengths differ. Duplicate keys: the last wins.

#### `groupBy(items: List<T>, function: fn(T) -> K) -> Map<K, List<T>>`
Group `items` into buckets keyed by `function(item)`. Within each bucket, items appear in their original order.

## Iterators and Pipe

`range`, `forEach`, `map`, `filter`, `fold`, and `|>` are documented in [Iterators and Iteration](/spec/0010-loopconstructsandfunctionaliterators/). Lists and maps are `Iterable`; map iteration yields `Entry<K, V>` records (`{ key, value }`, the same elements as `entries(map)`) — not tuples, since Osprey has no tuple type.

## HTTP

See [HTTP](/spec/0014-http/).

## WebSockets

See [WebSockets](/spec/0015-websockets/).

## Fibers and Channels

`spawn`, `await`, `send`, `recv`, `yield`, `Fiber<T>`, `Channel<T>` are documented in [Fibers and Concurrency](/spec/0011-lightweightfibersandconcurrency/).