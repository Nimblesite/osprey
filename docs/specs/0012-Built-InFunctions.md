# Built-in Functions

Reference for built-in functions available in every Osprey program. Operations that can fail return `Result`; see [Error Handling](0013-ErrorHandling.md).

> **Flavor layer — shared core (AST and above).**  The built-in function set is shared core: the *same* functions exist in every flavor, and a call to any of them lowers to the canonical `Expr::Call` node regardless of source surface. Only the call *spelling* is a flavor concern — the Default surface writes `toString(x)`, the ML flavor uses whitespace application `toString x` — and that difference is erased at lowering, so nothing here depends on which flavor produced the program. The Default spelling is shown throughout; see [Language Flavors](0023-LanguageFlavors.md) and [ML Flavor Syntax](0024-MLFlavorSyntax.md) for the surface mapping.

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
— live in [Testing Framework](0027-TestingFramework.md); the three functions
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
`/` operator is **float-only** by the [Type System](0004-TypeSystem.md) spec
(`int / int` promotes to `float`); `intDiv` is its integer sibling. A zero
divisor returns `Error(MathError)`; otherwise `Success(quotient)`. Like the
`/` and `%` operators, the `Success` payload auto-unwraps in the contexts
listed under [Result Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping);
under [ARITH-PLAIN](0013-ErrorHandling.md#arithmetic-and-result--arith-plain)
`+ - *` return plain scalars, so there is no `Result` to unwrap.

```osprey
intDiv(7, 2)        // Success(3)
intDiv(255643, 10)  // Success(25564)
intDiv(5, 0)        // Error(MathError) — "division by zero"
fn half(n) -> int = intDiv(n, 2)   // 3 — the declared return unwraps the Result
```

```osprey-ml
intDiv (7, 2)        // Success(3)
intDiv (255643, 10)  // Success(25564)
intDiv (5, 0)        // Error(MathError) — "division by zero"

half : int -> int                  // signature is load-bearing: it unwraps
half n = intDiv (n, 2)             // 3
```

The return type is required for the unwrap. Without it — `fn half(n) = intDiv(n, 2)` — the
function infers `Result<int, MathError>` and `half(7)` renders `Success(3)`; the
declared type is what makes the boundary an auto-unwrap context ([Result
Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping)).

### `checkedAdd` / `checkedSub` / `checkedMul` — [BUILTIN-CHECKED-ARITH]
Each has signature `(a: int, b: int) -> Result<int, MathError>`. Overflow-checked
integer addition, subtraction, and multiplication, lowering to
`llvm.sadd.with.overflow`, `llvm.ssub.with.overflow`, and
`llvm.smul.with.overflow` respectively. An overflowing operation returns
`Error(MathError)`; otherwise `Success(result)`. These carry the overflow
guarantee that the `+ - *` operators do not: those return plain scalars and wrap
two's complement
([ARITH-PLAIN](0013-ErrorHandling.md#arithmetic-and-result--arith-plain)).
Like `intDiv`, the `Success` payload auto-unwraps at value sites.

```osprey
checkedAdd(2, 3)                      // Success(5)
checkedMul(4294967296, 4294967296)    // Error(MathError) — "integer overflow"
fn twice(n) -> int = checkedMul(n, 2)   // declared return unwraps, as for intDiv
```

```osprey-ml
checkedAdd (2, 3)                      // Success(5)
checkedMul (4294967296, 4294967296)    // Error(MathError) — "integer overflow"

twice : int -> int
twice n = checkedMul (n, 2)
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

### Design Principles — [BUILTIN-STRING-DESIGN]

These rules govern the entire string API. They are drawn from idiomatic FP string libraries — primarily Elm's `String` module and Haskell's `Data.Text` — and adapted to Osprey's `Result`-only error model (Osprey has no `Maybe`/`Option`; see [Error Handling](0013-ErrorHandling.md)).

1. **Total functions return plain values.** Operations that cannot fail on any well-formed UTF-8 input (e.g. `length`, `toUpperCase`, `trim`, `contains`) return their result directly. They do **not** wrap in `Result`. This matches Elm (`String.length : String -> Int`) and Haskell (`Data.Text.length :: Text -> Int`).
2. **Partial functions return `Result<T, StringError>`.** Operations with inputs that can be invalid (`substring` with out-of-range indices, `parseInt` on non-numeric input, `split` with an empty separator) return `Result`.
3. **Subject-first argument order.** The string being operated on is the first parameter, enabling `myString |> trim |> toLowerCase` with the pipe operator (see [Iterators](0010-LoopConstructsAndFunctionalIterators.md)).
4. **No silent Unicode surprises (target behaviour).** Case conversion follows Unicode simple case mapping; lengths and indices are codepoint counts, not byte counts. This matches Haskell `Data.Text` and Elm `String`. **Implementation status:** the v1 runtime counts bytes (`strlen`-based) and uses ASCII-only `tolower`/`toupper`. UTF-8-aware rewrites build on the cursor primitives in [Cursor Access](#cursor-access-total-o1--builtin-string-cursor) ([BUILTIN-STRING-CURSOR]), which have shipped.
5. **No character (`Char`) type yet.** Higher-order operations over individual characters (`map`, `filter`, `foldl`, `any`, `all`) are intentionally **deferred** until Osprey introduces a `Char` type.

### Calling Style — [BUILTIN-STRING-UFCS]

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

- **Pipe (`x |> f`)** rewrites to `f(x)`. With extra args, `x |> f(a, b)` becomes `f(x, a, b)`. A bare identifier on the right (`x |> f`) is auto-promoted to a call — no parens needed for single-arg functions. See [Iterators](0010-LoopConstructsAndFunctionalIterators.md#pipe-operator--builtin-iter-pipe).
- **UFCS (`x.f(args)`)** rewrites to `f(x, args)`. **Parens are required** to disambiguate from field access — `x.f` always means field access, never a method call. If a record has a field named `f`, field access wins; UFCS is the fallback.
- **Direct call** is plain function application; nothing magic.

Multi-argument functions in this spec are documented subject-first (e.g. `split(s: string, separator: string)`) so all three forms work uniformly.

### `StringError` — [BUILTIN-STRING-ERROR]

```osprey
type StringError =
    | IndexOutOfRange { index: int, length: int }
    | InvalidArgument { message: string }
    | NotFound
    | ParseFailed { input: string }
```

### Inspection (total) — [BUILTIN-STRING-INSPECTION]

#### `length(s: string) -> int` — [BUILTIN-STRING-LENGTH]
Returns the number of Unicode codepoints. `length("héllo") == 5`.

#### `isEmpty(s: string) -> bool` — [BUILTIN-STRING-ISEMPTY]
True iff `length(s) == 0`. Equivalent to `length(s) == 0` but constant-time.

### Search (total) — [BUILTIN-STRING-SEARCH]

#### `contains(s: string, needle: string) -> bool` — [BUILTIN-STRING-CONTAINS]
True if `needle` occurs anywhere in `s`. An empty `needle` returns `true` (matches every position; consistent with Elm and Java).

```osprey
contains("hello world", "world")  // true
contains("hello", "")             // true
```

```osprey-ml
contains ("hello world", "world")  // true
contains ("hello", "")             // true
```

#### `startsWith(s: string, prefix: string) -> bool` — [BUILTIN-STRING-STARTSWITH]
#### `endsWith(s: string, suffix: string) -> bool` — [BUILTIN-STRING-ENDSWITH]

```osprey
"GET /api/users" |> startsWith("GET ")   // true
"image.png"      |> endsWith(".png")     // true
```

```osprey-ml
"GET /api/users" |> startsWith "GET "   // true
"image.png"      |> endsWith ".png"     // true
```

#### `indexOf(s: string, needle: string) -> Result<int, StringError>` — [BUILTIN-STRING-INDEXOF]
Returns the codepoint index of the first occurrence of `needle`, or `Error(NotFound)` if absent. An empty `needle` returns `Success { value: 0 }`.

### Cursor Access (total, O(1)) — [BUILTIN-STRING-CURSOR]

These primitives expose `string` as a random-access byte/codepoint buffer without allocating. They exist so user-written parsers (JSON, query strings, CSV, log formats) can run in linear time instead of the O(n²) imposed by chaining `substring`/`take`/`drop`. They are the lowest-level string operations in the language; everything above is implementable in pure Osprey on top of them.

#### `byteLength(s: string) -> int` — [BUILTIN-STRING-BYTELENGTH]
Byte length of the underlying UTF-8 storage. Equal to `length(s)` only for ASCII strings. O(1).

#### `byteAt(s: string, i: int) -> Result<int, StringError>` — [BUILTIN-STRING-BYTEAT]
Returns the UTF-8 byte at index `i` as an `int` in `[0, 255]`, or `Error(IndexOutOfRange)` if `i < 0` or `i >= byteLength(s)`. O(1). Does **not** allocate.

#### `codePointAt(s: string, byteIndex: int) -> Result<int, StringError>` — [BUILTIN-STRING-CODEPOINTAT]
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

#### `codePointWidth(codepoint: int) -> Result<int, StringError>` — [BUILTIN-STRING-CODEPOINTWIDTH]
Returns the number of UTF-8 bytes the codepoint encodes to (1–4), or `Error(InvalidArgument)` if `codepoint` is not a valid Unicode scalar value.

#### `fromCodePoint(codepoint: int) -> Result<string, StringError>` — [BUILTIN-STRING-FROMCODEPOINT]
Builds a single-codepoint `string`. Inverse of `codePointAt`. `Error(InvalidArgument)` for invalid scalar values.

### Substrings — [BUILTIN-STRING-SUBSTRINGS]

#### `substring(s: string, start: int, end: int) -> Result<string, StringError>` — [BUILTIN-STRING-SUBSTRING]
Extracts codepoints in `[start, end)`. Returns `Error(IndexOutOfRange)` if `start < 0`, `end > length(s)`, or `start > end`.

#### `take(s: string, n: int) -> string` — [BUILTIN-STRING-TAKE]
Returns at most the first `n` codepoints. If `n <= 0`, returns `""`; if `n >= length(s)`, returns `s`. **Never fails** — clamping mirrors Elm `String.left`.

#### `drop(s: string, n: int) -> string` — [BUILTIN-STRING-DROP]
Returns `s` without its first `n` codepoints, with the same clamping rules as `take`. Mirrors Elm `String.dropLeft`.

### Splitting and Joining — [BUILTIN-STRING-LIST]

#### `split(s: string, separator: string) -> Result<List<string>, StringError>` — [BUILTIN-STRING-SPLIT]
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

#### `join(parts: List<string>, separator: string) -> string` — [BUILTIN-STRING-JOIN]
Concatenates `parts` with `separator` between each pair. Returns `""` if `parts` is empty.

#### `lines(s: string) -> List<string>` — [BUILTIN-STRING-LINES]
Splits on `"\n"`. A trailing newline does not produce an empty final element (matches Haskell `Data.Text.lines`).

#### `words(s: string) -> List<string>` — [BUILTIN-STRING-WORDS]
Splits on runs of Unicode whitespace, dropping empty results.

### Transformation (total) — [BUILTIN-STRING-TRANSFORM]

#### `toUpperCase(s: string) -> string` — [BUILTIN-STRING-TOUPPERCASE]
#### `toLowerCase(s: string) -> string` — [BUILTIN-STRING-TOLOWERCASE]
Unicode simple case mapping. May change codepoint length (e.g. German `ß` → `SS`); this is intentional and matches Haskell `Data.Text.toUpper`/`toLower`.

#### `trim(s: string) -> string` — [BUILTIN-STRING-TRIM]
#### `trimStart(s: string) -> string` — [BUILTIN-STRING-TRIMSTART]
#### `trimEnd(s: string) -> string` — [BUILTIN-STRING-TRIMEND]
Remove leading/trailing/both runs of Unicode whitespace (per the Unicode `White_Space` property, matching Rust's `str::trim`).

#### `replace(s: string, needle: string, replacement: string) -> Result<string, StringError>` — [BUILTIN-STRING-REPLACE]
Replaces **every** occurrence of `needle` with `replacement`. Returns `Error(InvalidArgument)` if `needle` is empty (same reasoning as `split`).

#### `repeat(s: string, n: int) -> Result<string, StringError>` — [BUILTIN-STRING-REPEAT]
Concatenates `s` with itself `n` times. Returns `Error(InvalidArgument)` if `n < 0`. `repeat(s, 0) == ""`.

#### `reverse(s: string) -> string` — [BUILTIN-STRING-REVERSE]
Reverses codepoint order. (Note: grapheme-cluster reversal is a future addition.)

#### `padStart(s: string, targetLength: int, fill: string) -> Result<string, StringError>` — [BUILTIN-STRING-PADSTART]
#### `padEnd(s: string, targetLength: int, fill: string) -> Result<string, StringError>` — [BUILTIN-STRING-PADEND]
Pads `s` on the left/right with copies of `fill` until it reaches `targetLength` codepoints. Returns `s` unchanged if already long enough. Returns `Error(InvalidArgument)` if `fill` is empty.

### Parsing — [BUILTIN-STRING-PARSING]

#### `parseInt(s: string) -> Result<int, StringError>` — [BUILTIN-STRING-PARSEINT]
Parses a base-10 signed integer. Leading/trailing whitespace is rejected — callers must `trim` first. Returns `Error(ParseFailed)` on any non-numeric input (no silent zero-on-error like C's `atoi`).

#### `parseFloat(s: string) -> Result<float, StringError>` — [BUILTIN-STRING-PARSEFLOAT]
Parses a base-10 floating-point number. Same strictness as `parseInt`.

### Concatenation Operator — [BUILTIN-STRING-CONCAT]

The `+` operator on two `string` values returns `string` directly. String concatenation cannot fail and is never `Result`-wrapped.

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

## Collection Functions — [BUILTIN-COLLECTIONS]

Reference for builtins over `List<T>` and `Map<K, V>` (defined in [Type System — Collection Types](0004-TypeSystem.md#collection-types)). All functions are **pure** — they never mutate; "modifying" operations return a new collection that shares structure with the original. Operations that can fail return `Result`; total operations return their value directly. Subject-first argument order — the collection being operated on is the first parameter, enabling `xs |> filter(p) |> length(...)`.

**Implementation status.** The bare names in this section (`get`, `append`,
`set`, `keys`, …) are the **normative target surface** — the spelling programs
should eventually use. Today only `length` and `isEmpty` carry the collection
behaviour under their bare names: they are receiver-directed, one spelling over
`string`, `List<T>` and `Map<K, V>` ([BUILTIN-COLLECTION-LENGTH],
[BUILTIN-COLLECTION-ISEMPTY]). Every other operation that exists at all ships
under a **prefixed** spelling instead — `listGet`, `listAppend`, `listPrepend`,
`listConcat`, `listReverse`, `listContains`, `listLength`, `mapGet`, `mapSet`,
`mapRemove`, `mapMerge`, `mapContains`, `mapLength`, `mapKeys`, `mapValues` —
and the bare name reaches no collection operation (`contains`, `reverse` and
`indexOf` do resolve, but to the **string** functions of the section above, so a
collection argument is a type error). Exposing the bare names requires receiver-directed overload
resolution, tracked in [plan
0004](../plans/0004-collection-stdlib-completion.md). Each entry below names the
spelling that ships; entries marked **specified; not yet implemented** have no
implementation under any spelling.

### Design Principles

The collection API follows the same rules as the string API ([Design Principles](#design-principles--builtin-string-design)) and is adapted to Osprey's `Result`-only error model. In addition:

1. **Subset-matching for Map patterns.** A map pattern matches any superset of its listed entries (matches Elm and Erlang/Elixir).
2. **No iteration order for Maps.** Programs that need a deterministic order MUST sort the result of `keys` or `entries`.
3. **No `Set<T>` yet.** Use `Map<K, unit>` for set-like semantics; a first-class `Set<T>` is deferred to a future revision.

### Common (`List` and `Map`) — [BUILTIN-COLLECTION-COMMON]

#### `length(list: List<T>) -> int` &nbsp; / &nbsp; `length(map: Map<K, V>) -> int` — [BUILTIN-COLLECTION-LENGTH]
Number of elements. Constant time on both representations. **Implemented under
this bare name:** the receiver's inferred type selects the list, map or string
runtime, so `length` is the one spelling that already behaves as specified.

#### `isEmpty(list: List<T>) -> bool` &nbsp; / &nbsp; `isEmpty(map: Map<K, V>) -> bool` — [BUILTIN-COLLECTION-ISEMPTY]
True iff `length` is `0`. Constant time. **Implemented under this bare name**,
receiver-directed exactly like `length`.

### `List<T>` — [BUILTIN-LIST]

Backed by an immutable bitmapped vector trie (see [TYPE-LIST](0004-TypeSystem.md#listt--type-list)). Index access is `O(log₃₂ n)`.

#### `get(list: List<T>, index: int) -> Result<T, IndexError>` — [BUILTIN-LIST-GET]
Same as `list[index]`. Returns `Error(OutOfBounds)` if `index < 0` or `index >= length(list)`. Ships as `listGet`; the `list[index]` form resolves today.

#### `head(list: List<T>) -> Result<T, IndexError>` — [BUILTIN-LIST-HEAD]
First element, or `Error(OutOfBounds)` if empty. **Specified; not yet implemented.**

#### `tail(list: List<T>) -> List<T>` — [BUILTIN-LIST-TAIL]
All elements except the first. `tail([]) == []` (total — never errors). **Specified; not yet implemented.** The `[head, ...tail]` list *pattern* is a separate, shipped feature ([TYPE-LIST-PATTERNS](0004-TypeSystem.md#patterns--type-list-patterns)) — it does not make this function exist.

#### `prepend(list: List<T>, value: T) -> List<T>` — [BUILTIN-LIST-PREPEND]
Returns a new list with `value` at the front. Ships as `listPrepend`.

#### `append(list: List<T>, value: T) -> List<T>` — [BUILTIN-LIST-APPEND]
Returns a new list with `value` at the end. Ships as `listAppend`.

#### `concat(left: List<T>, right: List<T>) -> List<T>` — [BUILTIN-LIST-CONCAT]
Returns `left ++ right`. Same as `left + right`. `O(n + m)` for the baseline trie; `O(log n)` if upgraded to an RRB-tree. Ships as `listConcat`; the `+` operator on two lists resolves today.

#### `reverse(list: List<T>) -> List<T>` — [BUILTIN-LIST-REVERSE]
New list in reverse order. `O(n)`. Ships as `listReverse`. (The bare `reverse` currently resolves to the **string** reverse, not this one.)

#### `contains(list: List<T>, value: T) -> bool` — [BUILTIN-LIST-CONTAINS]
True iff some element of `list` is structurally equal to `value`. `O(n)`. Ships as `listContains`. (The bare `contains` currently resolves to the **string** contains, not this one.)

#### `indexOf(list: List<T>, value: T) -> Result<int, IndexError>` — [BUILTIN-LIST-INDEXOF]
First index of `value`, or `Error(NotFound)`. **Specified; not yet implemented** under any spelling — the `indexOf` that resolves today is the string one (`indexOf(s: string, needle: string)`).

### `Map<K, V>` — [BUILTIN-MAP]

Backed by a HAMT with branching factor 32 (see [TYPE-MAP](0004-TypeSystem.md#mapk-v--type-map)). Lookup/insert/remove are `O(log₃₂ n)` expected.

#### `get(map: Map<K, V>, key: K) -> Result<V, IndexError>` — [BUILTIN-MAP-GET]
Same as `map[key]`. Returns `Error(NotFound)` if `key` is absent. Ships as `mapGet`; the `map[key]` form resolves today.

#### `contains(map: Map<K, V>, key: K) -> bool` — [BUILTIN-MAP-CONTAINS]
True iff `key` is present. Ships as `mapContains`.

#### `set(map: Map<K, V>, key: K, value: V) -> Map<K, V>` — [BUILTIN-MAP-SET]
Returns a new map with `key` bound to `value`, replacing any prior binding. Ships as `mapSet`.

#### `remove(map: Map<K, V>, key: K) -> Map<K, V>` — [BUILTIN-MAP-REMOVE]
Returns a new map without `key`. If `key` is absent, returns `map` (total — never errors). Ships as `mapRemove`.

#### `update(map: Map<K, V>, key: K, fn: fn(Result<V, IndexError>) -> Result<V, IndexError>) -> Map<K, V>` — [BUILTIN-MAP-UPDATE]
Apply `fn` to the current binding (or `Error(NotFound)`). If `fn` returns `Success { value }`, the key is set; if it returns `Error(NotFound)`, the key is removed. Mirrors Elm's `Dict.update : comparable -> (Maybe v -> Maybe v) -> Dict comparable v -> Dict comparable v`. **Specified; not yet implemented.**

#### `merge(left: Map<K, V>, right: Map<K, V>) -> Map<K, V>` — [BUILTIN-MAP-MERGE]
Right-biased union — `right` wins on key conflicts. Same as `left + right`. Ships as `mapMerge`; the `+` operator on two maps resolves today.

#### `keys(map: Map<K, V>) -> List<K>` — [BUILTIN-MAP-KEYS]
All keys. Iteration order is **unspecified**. Ships as `mapKeys`.

> **Name collision — `keys`/`values` vs. `mapKeys`/`mapValues`.** The shipped
> arity-1 `mapKeys(map) -> List<K>` and `mapValues(map) -> List<V>` builtins
> *are* these two accessors. The names they occupy are the ones this section
> also gives to the arity-2 transformers [BUILTIN-MAP-MAPKEYS] and
> [BUILTIN-MAP-MAPVALUES] below, which do something entirely different
> (`Map -> Map`, not `Map -> List`). Both meanings cannot keep the spelling.
> The resolution: when the bare surface lands, the shipped accessors are
> renamed to `keys` / `values`, freeing `mapKeys` / `mapValues` for the
> transformers. Until then, `mapKeys`/`mapValues` in a working program mean the
> accessors.

#### `values(map: Map<K, V>) -> List<V>` — [BUILTIN-MAP-VALUES]
All values, in the same order as `keys(map)`. Ships as `mapValues` — see the name collision noted under [BUILTIN-MAP-KEYS].

#### `entries(map: Map<K, V>) -> List<Entry<K, V>>` — [BUILTIN-MAP-ENTRIES]
All key/value entries, in the same order as `keys(map)`. An entry is the record
`type Entry<K, V> = { key: K, value: V }` (Osprey has no tuple type, so a pair is
a two-field record, not a `(K, V)` tuple). **Specified; not yet implemented.**

#### `mapValues(map: Map<K, V>, fn: fn(V) -> W) -> Map<K, W>` — [BUILTIN-MAP-MAPVALUES]
Apply `fn` to every value, preserving keys. **Specified; not yet implemented** — the shipped `mapValues` is the arity-1 accessor [BUILTIN-MAP-VALUES], not this transformer.

#### `mapKeys(map: Map<K, V>, fn: fn(K) -> K2) -> Map<K2, V>` — [BUILTIN-MAP-MAPKEYS]
Apply `fn` to every key. If `fn` produces duplicate keys, the **last** wins (consistent with `+`). **Specified; not yet implemented** — the shipped `mapKeys` is the arity-1 accessor [BUILTIN-MAP-KEYS], not this transformer.

#### `filterEntries(map: Map<K, V>, fn: fn(K, V) -> bool) -> Map<K, V>` — [BUILTIN-MAP-FILTERENTRIES]
Keep entries where `fn(k, v)` is true. **Specified; not yet implemented.**

#### `foldEntries(map: Map<K, V>, initial: U, function: fn(U, K, V) -> U) -> U` — [BUILTIN-MAP-FOLDENTRIES]
Reduce over entries. Iteration order is unspecified — `function` MUST be commutative if order matters. **Specified; not yet implemented.**

#### `zipToMap(keys: List<K>, values: List<V>) -> Result<Map<K, V>, IndexError>` — [BUILTIN-MAP-ZIPTOMAP]
Build a map from parallel lists. `Error(InvalidArgument)` if lengths differ. Duplicate keys: the last wins. **Specified; not yet implemented.**

#### `groupBy(items: List<T>, function: fn(T) -> K) -> Map<K, List<T>>` — [BUILTIN-MAP-GROUPBY]
Group `items` into buckets keyed by `function(item)`. Within each bucket, items appear in their original order. **Specified; not yet implemented.**

## Iterators and Pipe

`range`, `forEach`, `map`, `filter`, `fold`, and `|>` are documented in [Iterators and Iteration](0010-LoopConstructsAndFunctionalIterators.md). Lists and maps are `Iterable`; map iteration yields `Entry<K, V>` records (`{ key, value }`, the same elements as `entries(map)`) — not tuples, since Osprey has no tuple type.

## HTTP

See [HTTP](0014-HTTP.md).

## WebSockets

See [WebSockets](0015-WebSockets.md).

## Fibers and Channels

`spawn`, `await`, `send`, `recv`, `yield`, `Fiber<T>`, `Channel<T>` are documented in [Fibers and Concurrency](0011-LightweightFibersAndConcurrency.md).