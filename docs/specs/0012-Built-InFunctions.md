# Built-in Functions

Reference for built-in functions available in every Osprey program. Structured
fallible operations use `Result`; low-level handle and status APIs document
their integer returns explicitly. See [Error Handling](0013-ErrorHandling.md).

Built-ins are shared by both language flavors. Examples use the Default
surface unless an ML example clarifies different call syntax.

## Basic I/O Functions

### `print(value: printable) -> Unit` — [BUILTIN-PRINT]

`printable` is documentation shorthand, not a user-declared type. It includes
`int`, `float`, `bool`, `string`, `Unit`, explicitly erased `any`, and
`Result<T, E>` when both payloads are printable (with `Error` and `MathError`
accepted as error payloads). Concrete records, collections, functions,
iterators, fibers, channels, and pointers are rejected. Results render as
`Success(value)` or `Error(message)`; `Unit` renders as `0`; `print` appends a
newline. An explicitly erased `any` value is a compatibility exception, not a
dynamic formatter: if it hides an aggregate, the raw pointer-sized value is
rendered rather than the aggregate's contents.

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

### `toString(value: printable) -> string` — [BUILTIN-TOSTRING]
Uses the same accepted types and formatting as `print`, without writing output.

## Testing Functions

```osprey
test(name: string, body: fn() -> a) -> Unit
expect(actual: any, expected: any) -> Unit
check(label: string, expected: any, actual: any) -> Unit
```

Their behavior, shadowing rule, TAP output, filtering, discovery, coverage, and
editor integration are specified in [Testing Framework](0027-TestingFramework.md).

## Numeric Functions

### `abs(n: int) -> int` — [BUILTIN-ABS]
Returns the absolute value using the language's signed 64-bit wrapping
arithmetic. Because `2^63` is not representable, `abs(-9223372036854775808)`
returns `-9223372036854775808`.

### `intDiv(a: int, b: int) -> Result<int, Error>` — [BUILTIN-INTDIV]
Truncates toward zero. A zero divisor returns `Error("division by zero")`;
`intDiv(-9223372036854775808, -1)` returns `Error("integer overflow")`;
all other inputs return `Success(quotient)`. The `/` operator instead returns
`float`. The `Success` payload auto-unwraps only in the contexts listed under
[Result Auto-Unwrapping](0004-TypeSystem.md#result-auto-unwrapping).

```osprey
intDiv(7, 2)        // Success(3)
intDiv(255643, 10)  // Success(25564)
intDiv(5, 0)        // Error — "division by zero"
intDiv(-9223372036854775808, -1) // Error — "integer overflow"
fn half(n) -> int = intDiv(n, 2)   // 3 — the declared return unwraps the Result
```

```osprey-ml
intDiv (7, 2)        // Success(3)
intDiv (255643, 10)  // Success(25564)
intDiv (5, 0)        // Error — "division by zero"
intDiv (-9223372036854775808, -1) // Error — "integer overflow"

half : int -> int                  // signature is load-bearing: it unwraps
half n = intDiv (n, 2)             // 3
```

Without the declared return type, `half` infers `Result<int, Error>` and
`half(7)` renders `Success(3)`.

### `checkedAdd` / `checkedSub` / `checkedMul` — [BUILTIN-CHECKED-ARITH]
Each has signature `(a: int, b: int) -> Result<int, Error>`. Overflow-checked
integer addition, subtraction, and multiplication, lowering to
`llvm.sadd.with.overflow`, `llvm.ssub.with.overflow`, and
`llvm.smul.with.overflow` respectively. An overflowing operation returns
`Error`; otherwise `Success(result)`. These carry the overflow
guarantee that the `+ - *` operators do not: those return plain scalars and wrap
two's complement
([ARITH-PLAIN](0013-ErrorHandling.md#arithmetic-and-result--arith-plain)).
Like `intDiv`, the `Success` payload auto-unwraps at value sites.

```osprey
checkedAdd(2, 3)                      // Success(5)
checkedMul(4294967296, 4294967296)    // Error — "integer overflow"
fn twice(n) -> int = checkedMul(n, 2)   // declared return unwraps, as for intDiv
```

```osprey-ml
checkedAdd (2, 3)                      // Success(5)
checkedMul (4294967296, 4294967296)    // Error — "integer overflow"

twice : int -> int
twice n = checkedMul (n, 2)
```

### `random() -> int` — [BUILTIN-RANDOM]
A cryptographically-secure uniform random non-negative integer in `[0, 2^63-1]`,
drawn fresh from the operating system's CSPRNG (`arc4random_buf` on macOS/BSD,
`getrandom(2)` on Linux, falling back to `/dev/urandom`). It carries no userspace
seed or state, so calls are not reproducible.

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

### `randomBelow(n: int) -> Result<int, Error>` — [BUILTIN-RANDOM-BELOW]
A cryptographically-secure uniform random integer in the half-open range
`[0, n)`. The result is **unbiased**: it is drawn by rejection sampling, so every
value in the range is equally likely (a plain `random() % n` is not). A
non-positive `n` returns `Error`; otherwise `Success(value)` with
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

Strings are immutable, NUL-terminated UTF-8 byte sequences. String operations
return new values and do not mutate their arguments.

### Rules

Total operations return plain values; invalid indices, arguments, or parses
return `Result`. The subject is the first argument so calls compose with `|>`.
Except for the explicit UTF-8 cursor functions, lengths and indices are byte
based. Case conversion and whitespace handling cover ASCII.

### Calling Style — [BUILTIN-STRING-UFCS]

String functions support pipe, direct-call, and Default-flavor method syntax.

```osprey
// Pipe chain
"  Hello, World  " |> trim |> toLowerCase |> split(", ")

// Direct call
toLowerCase(trim("  Hello  "))

// Method-call (UFCS) — sugar, equivalent to the direct form
"  Hello  ".trim().toLowerCase()
```

```osprey-ml
// Pipe chain
"  Hello, World  " |> trim |> toLowerCase |> split ", "

// Direct call
toLowerCase (trim "  Hello  ")

// Chained UFCS (`.trim().toLowerCase()`) has no ML surface; use the pipe form:
"  Hello  " |> trim |> toLowerCase
```

All three desugar to the same call. Rules:

- **Pipe (`x |> f`)** rewrites to `f(x)`. With extra args, `x |> f(a, b)` becomes `f(x, a, b)`. A bare identifier on the right (`x |> f`) is auto-promoted to a call — no parens needed for single-arg functions. See [Iterators](0010-LoopConstructsAndFunctionalIterators.md#pipe-operator--builtin-iter-pipe).
- **UFCS (`x.f(args)`)** rewrites to `f(x, args)`. **Parens are required** to disambiguate from field access — `x.f` always means field access, never a method call. If a record has a field named `f`, field access wins; UFCS is the fallback.
- **Direct call** is ordinary function application.

Multi-argument functions in this spec are documented subject-first (e.g. `split(s: string, separator: string)`) so all three forms work uniformly.

### Inspection (total) — [BUILTIN-STRING-INSPECTION]

#### `length(s: string) -> int` — [BUILTIN-STRING-LENGTH]
Returns the number of bytes. It is equivalent to `byteLength` for strings.

#### `isEmpty(s: string) -> bool` — [BUILTIN-STRING-ISEMPTY]
True iff `length(s) == 0`. Equivalent to `length(s) == 0` but constant-time.
The same names accept `List<T>` and `Map<string, V>` as described under
[Collection Functions](#collection-functions); no other receiver type is
accepted.

### Search (total) — [BUILTIN-STRING-SEARCH]

#### `contains(s: string, needle: string) -> bool` — [BUILTIN-STRING-CONTAINS]
True if `needle` occurs anywhere in `s`. An empty `needle` returns `true`.

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

#### `indexOf(s: string, needle: string) -> Result<int, Error>` — [BUILTIN-STRING-INDEXOF]
Returns the byte index of the first occurrence of `needle`, or
`Error` with `"indexOf: substring not found"` if absent. An empty `needle`
returns `Success { value: 0 }`.

### Cursor Access (total, O(1)) — [BUILTIN-STRING-CURSOR]

These primitives provide non-allocating access to UTF-8 bytes and codepoints.

#### `byteLength(s: string) -> int` — [BUILTIN-STRING-BYTELENGTH]
Byte length of the underlying UTF-8 storage. Equivalent to `length(s)`. O(1).

#### `byteAt(s: string, i: int) -> Result<int, Error>` — [BUILTIN-STRING-BYTEAT]
Returns the UTF-8 byte at index `i` as an `int` in `[0, 255]`, or `Error` if
`i < 0` or `i >= byteLength(s)`. O(1). Does **not** allocate.

#### `codePointAt(s: string, byteIndex: int) -> Result<int, Error>` — [BUILTIN-STRING-CODEPOINTAT]
Decodes the UTF-8 codepoint starting at `byteIndex` and returns it as an `int`.
Returns `Error` if the index is out of range, does not land on a codepoint
boundary, or begins a truncated, overlong, surrogate, out-of-range, or otherwise
malformed UTF-8 sequence. O(1) (at most 4 bytes read). Pair with
`codePointWidth` to advance:

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

#### `codePointWidth(codepoint: int) -> Result<int, Error>` — [BUILTIN-STRING-CODEPOINTWIDTH]
Returns the number of UTF-8 bytes the codepoint encodes to (1–4), or `Error`
if `codepoint` is not a valid Unicode scalar value.

#### `fromCodePoint(codepoint: int) -> Result<string, Error>` — [BUILTIN-STRING-FROMCODEPOINT]
Builds a single-codepoint `string`. Inverse of `codePointAt`. Returns `Error`
for surrogates, values outside `0..0x10FFFF`, and U+0000, which the
NUL-terminated string ABI cannot represent.

### Substrings — [BUILTIN-STRING-SUBSTRINGS]

#### `substring(s: string, start: int, end: int) -> Result<string, Error>` — [BUILTIN-STRING-SUBSTRING]
Extracts bytes in `[start, end)`. Returns `Error` if
`start < 0`, `end > length(s)`, or `start > end`.

#### `take(s: string, n: int) -> string` — [BUILTIN-STRING-TAKE]
Returns at most the first `n` bytes. If `n <= 0`, returns `""`; if
`n >= length(s)`, returns `s`.

#### `drop(s: string, n: int) -> string` — [BUILTIN-STRING-DROP]
Returns `s` without its first `n` bytes, with the same clamping rules as
`take`.

### Splitting and Joining — [BUILTIN-STRING-LIST]

#### `split(s: string, separator: string) -> Result<List<string>, Error>` — [BUILTIN-STRING-SPLIT]
Splits `s` on every occurrence of `separator`. Returns `Error` with
`"split: separator must not be empty"` if `separator` is empty.

```osprey
match split("a,b,c", ",") {
    Success { value }   => forEachList(value, print)   // "a" "b" "c"
    Error   { message } => print("split error")
}
```

```osprey-ml
match split ("a,b,c", ",")
    Success value   => forEachList (value, print)   // "a" "b" "c"
    Error message   => print "split error"
```

#### `join(parts: List<string>, separator: string) -> string` — [BUILTIN-STRING-JOIN]
Concatenates `parts` with `separator` between each pair. Returns `""` if `parts` is empty.

#### `lines(s: string) -> List<string>` — [BUILTIN-STRING-LINES]
Splits on `"\n"`. A trailing newline does not produce an empty final element.

#### `words(s: string) -> List<string>` — [BUILTIN-STRING-WORDS]
Splits on runs of ASCII whitespace, dropping empty results.

### Transformation (total) — [BUILTIN-STRING-TRANSFORM]

#### `toUpperCase(s: string) -> string` — [BUILTIN-STRING-TOUPPERCASE]
#### `toLowerCase(s: string) -> string` — [BUILTIN-STRING-TOLOWERCASE]
ASCII case conversion. Other bytes are copied unchanged.

#### `trim(s: string) -> string` — [BUILTIN-STRING-TRIM]
#### `trimStart(s: string) -> string` — [BUILTIN-STRING-TRIMSTART]
#### `trimEnd(s: string) -> string` — [BUILTIN-STRING-TRIMEND]
Remove leading, trailing, or both runs of ASCII whitespace.

#### `replace(s: string, needle: string, replacement: string) -> Result<string, Error>` — [BUILTIN-STRING-REPLACE]
Replaces every occurrence of `needle` with `replacement`. Returns `Error` if
`needle` is empty.

#### `repeat(s: string, n: int) -> Result<string, Error>` — [BUILTIN-STRING-REPEAT]
Concatenates `s` with itself `n` times. Returns `Error` if `n < 0`.
`repeat(s, 0) == ""`.

#### `reverse(s: string) -> string` — [BUILTIN-STRING-REVERSE]
Reverses byte order.

#### `padStart(s: string, targetLength: int, fill: string) -> Result<string, Error>` — [BUILTIN-STRING-PADSTART]
#### `padEnd(s: string, targetLength: int, fill: string) -> Result<string, Error>` — [BUILTIN-STRING-PADEND]
Pads `s` on the left or right with repeated bytes from `fill` until it reaches
`targetLength` bytes. Returns `s` unchanged if already long enough and `Error`
if `fill` is empty.

### Parsing — [BUILTIN-STRING-PARSING]

#### `parseInt(s: string) -> Result<int, Error>` — [BUILTIN-STRING-PARSEINT]
Parses a base-10 signed integer. Leading/trailing whitespace is rejected;
callers must `trim` first. Returns `Error` on invalid or out-of-range input.

#### `parseFloat(s: string) -> Result<float, Error>` — [BUILTIN-STRING-PARSEFLOAT]
Parses a finite base-10 number with an optional sign, decimal point, and
decimal exponent. At least one digit is required; surrounding whitespace,
NaN, infinity, hexadecimal floats, malformed exponents, and non-finite results
return `Error`.

### Concatenation Operator — [BUILTIN-STRING-CONCAT]

The `+` operator on two `string` values returns `string` directly. String concatenation cannot fail and is never `Result`-wrapped.

```osprey
let greeting = "Hello, " + name + "!"
```

```osprey-ml
greeting = "Hello, " + name + "!"
```

## File System Functions — [BUILTIN-FILE]

### `writeFile(path: string, content: string) -> Result<int, Error>`
Writes or replaces a file and returns the number of bytes written.

### `readFile(path: string) -> Result<string, Error>`
Reads a complete file.

## Process Operations — [BUILTIN-PROCESS]

### `spawnProcess(command: string, callback: fn(int, int, string) -> Unit) -> Result<int, Error>`
Starts a process and returns its handle. The callback receives the handle,
event kind (`1` stdout, `2` stderr, `3` exit), and event text.

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
Waits for process completion and returns the exit code, or `-1` if the handle
is outside the valid range or has no process.

### `cleanupProcess(processId: int) -> Unit`
Releases process resources.

## JSON Document Functions — [BUILTIN-JSON]

```osprey
jsonParse(text: string) -> Result<int, Error>
jsonGet(document: int, path: string) -> Result<string, Error>
jsonLength(document: int, path: string) -> int
jsonFree(document: int) -> Result<int, Error>
```

`jsonParse` returns a positive opaque document handle. Paths use dotted object
keys and bracketed array indices, such as `user.items[0].name`; `""` addresses
the root. `jsonGet` converts a string, number, boolean, or null scalar to a
string and returns `Error` for an invalid path, handle, array, or object.
`jsonLength` returns an array length or object member count and returns `-1`
for an invalid path, handle, or scalar. A successful handle must be released
once with `jsonFree`; an invalid handle or double free returns `Error`.

## Terminal Functions — [BUILTIN-TERM]

```osprey
termReadKey() -> Result<string, Error>
termRawMode(enabled: int) -> Unit
termCols() -> int
termRows() -> int
termClear() -> int
termMoveCursor(row: int, column: int) -> int
termHideCursor() -> int
termShowCursor() -> int
```

On POSIX terminals, `termRawMode(1)` disables canonical input and echo and
enters the alternate screen; `termRawMode(0)` restores the saved mode and
screen. Its native status is not exposed. `termReadKey` returns normalized key
names such as `Enter`, `Up`, or `Ctrl-C`, or the literal input byte.
`termCols` and `termRows` return `-1` when the terminal size is unavailable.
The remaining functions write ANSI control sequences and return `0`; cursor
coordinates below `1` are clamped to `1`. Windows implementations return
`Error` from `termReadKey`, do nothing for raw mode, and return `-1` from the
integer functions.

## Collection Functions

Collection operations return new values without changing their inputs. Except
for `length` and `isEmpty`, public names are prefixed with `list` or `map`.

### Common (`List` and `Map`)

#### `length(list: List<T>) -> int` &nbsp; / &nbsp; `length(map: Map<string, V>) -> int` — [BUILTIN-COLLECTION-LENGTH]
Returns the element count. `listLength` and `mapLength` are equivalent
type-specific spellings.

#### `isEmpty(list: List<T>) -> bool` &nbsp; / &nbsp; `isEmpty(map: Map<string, V>) -> bool` — [BUILTIN-COLLECTION-ISEMPTY]
Returns whether the element count is zero. The receiver type selects string,
list, or map behavior for both common functions.

### `List<T>` — [BUILTIN-LIST]

`List()` creates an empty list. List literals create populated lists.

#### `listGet(list: List<T>, index: int) -> Result<T, Error>` — [BUILTIN-LIST-GET]
Equivalent to `list[index]`. An out-of-range index returns `Error`.

#### `listPrepend(list: List<T>, value: T) -> List<T>` — [BUILTIN-LIST-PREPEND]
Returns a list with `value` at the front.

#### `listAppend(list: List<T>, value: T) -> List<T>` — [BUILTIN-LIST-APPEND]
Returns a list with `value` at the end.

#### `listConcat(left: List<T>, right: List<T>) -> List<T>` — [BUILTIN-LIST-CONCAT]
Concatenates two lists. `left + right` is equivalent.

#### `listReverse(list: List<T>) -> List<T>` — [BUILTIN-LIST-REVERSE]
Returns the elements in reverse order.

#### `listContains(list: List<T>, value: T) -> bool` — [BUILTIN-LIST-CONTAINS]
Strings compare by content. Scalar values compare by value; managed handles
such as nested lists and records compare by identity.

#### `forEachList(list: List<T>, function: fn(T) -> Unit) -> Unit` — [BUILTIN-LIST-FOREACH]
Calls `function` once per element in index order.

### `Map<string, V>` — [BUILTIN-MAP]

`Map()` and map literals create string-keyed maps. Map iteration order is
unspecified.

#### `mapGet(map: Map<string, V>, key: string) -> Result<V, Error>` — [BUILTIN-MAP-GET]
Equivalent to `map[key]`. A missing key returns `Error`.

#### `mapContains(map: Map<string, V>, key: string) -> bool` — [BUILTIN-MAP-CONTAINS]
Returns whether `key` is present.

#### `mapSet(map: Map<string, V>, key: string, value: V) -> Map<string, V>` — [BUILTIN-MAP-SET]
Returns a map with `key` bound to `value`, replacing any prior binding.

#### `mapRemove(map: Map<string, V>, key: string) -> Map<string, V>` — [BUILTIN-MAP-REMOVE]
Returns a map without `key`. A missing key leaves the map unchanged.

#### `mapMerge(left: Map<string, V>, right: Map<string, V>) -> Map<string, V>` — [BUILTIN-MAP-MERGE]
Returns the right-biased union. `left + right` is equivalent.

#### `mapKeys(map: Map<string, V>) -> List<string>` — [BUILTIN-MAP-KEYS]
Returns all keys in unspecified order.

#### `mapValues(map: Map<string, V>) -> List<V>` — [BUILTIN-MAP-VALUES]
Returns all values in the same traversal order as `mapKeys`.

## Iterators and Pipe

`range`, `forEach`, `map`, `filter`, `fold`, and `|>` are documented in
[Iterators and Iteration](0010-LoopConstructsAndFunctionalIterators.md).

## HTTP

See [HTTP](0014-HTTP.md).

## WebSockets

See [WebSockets](0015-WebSockets.md).

## Fibers and Channels

`spawn`, `await`, `send`, `recv`, `yield`, `Fiber<T>`, `Channel<T>` are documented in [Fibers and Concurrency](0011-LightweightFibersAndConcurrency.md).
