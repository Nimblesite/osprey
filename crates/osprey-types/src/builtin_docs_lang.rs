//! Built-in documentation data (language & collections). Generated companion to
//! `builtins.rs`: every entry's prose pairs with the type scheme of the
//! same name. Edit prose here; edit types in `builtins.rs`. The parity
//! test in `builtin_docs.rs` guarantees the two stay in lockstep.
//!
//! Param order and count MUST match the builtin's real arity.

use crate::builtin_docs::BuiltinDoc;

/// Builds one [`BuiltinDoc`] table entry from its prose. Parameters are a
/// bracketed series of `"name" => "description"` pairs, expanding to the exact
/// `BuiltinDoc { .. }` / `ParamDoc { .. }` literals once written out by hand, so
/// every entry in this file and `builtin_docs_sys` stays a single terse call.
macro_rules! builtin_doc {
    ($name:expr, $summary:expr, [$($pn:expr => $pd:expr),* $(,)?], $example:expr $(,)?) => {
        $crate::builtin_docs::BuiltinDoc {
            name: $name,
            summary: $summary,
            params: &[$($crate::builtin_docs::ParamDoc { name: $pn, description: $pd }),*],
            example: $example,
        }
    };
}

pub(crate) use builtin_doc;

/// `core` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static CORE: &[BuiltinDoc] = &[
    builtin_doc!(
        "print",
        "Writes a supported scalar or Result representation followed by a newline. Unit renders as 0.",
        ["value" => "The value to print"],
        "print(\"Hello, World!\")  // Prints: Hello, World!\nprint(42)             // Prints: 42\nprint(true)           // Prints: true",
    ),
    builtin_doc!(
        "input",
        "Reads a string from the user's input.",
        [],
        "let userInput = input()\nprint(userInput)",
    ),
    builtin_doc!(
        "toString",
        "Formats the same scalar and Result values accepted by print without writing output. Unit renders as 0.",
        ["value" => "The value to convert to string"],
        "let str = toString(42)\nprint(str)  // Prints: 42",
    ),
    builtin_doc!(
        "length",
        "Returns a string's byte length or a List/Map element count.",
        ["s" => "The string, List, or Map to measure"],
        "let len = length(\"hello\")  // 5",
    ),
    builtin_doc!(
        "sleep",
        "Pauses execution for the specified number of milliseconds.",
        ["milliseconds" => "Number of milliseconds to sleep"],
        "sleep(1000)  // Sleep for 1 second\nprint(\"Awake!\")",
    ),
    builtin_doc!(
        "range",
        "Creates an iterator that generates numbers from start to end (exclusive).",
        ["start" => "The starting number (inclusive)", "end" => "The ending number (exclusive)"],
        "forEach(range(0, 5), fn(x) => print(x))  // Prints: 0, 1, 2, 3, 4",
    ),
    builtin_doc!(
        "abs",
        "Returns Result<int, MathError>. INT64_MIN yields Error because its positive magnitude is not representable.",
        ["value" => "The integer whose magnitude to take"],
        "let d = abs(0 - 5)  // 5",
    ),
    builtin_doc!(
        "intDiv",
        "Truncating integer division. Zero returns Error(division by zero); INT64_MIN / -1 returns Error(integer overflow).",
        ["a" => "The dividend", "b" => "The divisor"],
        "fn half(n) -> int = intDiv(n, 2)  // half(7) == 3",
    ),
    builtin_doc!(
        "toFloat",
        "Widens an int to a float, rounding to nearest even. Exact for magnitudes up to 2^53. Total, so it returns a bare float rather than a Result. This is the explicit element conversion GPU kernels use; there is no implicit widening at a buffer boundary.",
        ["value" => "The integer to widen"],
        "let xs = gpuIota(1000) |> gpuMap(toFloat)  // a float buffer 0.0 .. 999.0",
    ),
    builtin_doc!(
        "checkedAdd",
        "Named overflow-checked integer addition, returning Result<int, Error>.",
        ["a" => "The first addend", "b" => "The second addend"],
        "let t = checkedAdd(a: 9223372036854775807, b: 1) ?: 0  // 0 — overflow reported",
    ),
    builtin_doc!(
        "checkedSub",
        "Named overflow-checked integer subtraction, returning Result<int, Error>.",
        ["a" => "The minuend", "b" => "The subtrahend"],
        "let d = checkedSub(a: 10, b: 4) ?: 0  // 6",
    ),
    builtin_doc!(
        "checkedMul",
        "Named overflow-checked integer multiplication, returning Result<int, Error>.",
        ["a" => "The first factor", "b" => "The second factor"],
        "let p = checkedMul(a: 6, b: 7) ?: 0  // 42",
    ),
    builtin_doc!(
        "random",
        "A cryptographically-secure uniform random non-negative integer (0 .. 2^63-1), drawn fresh from the OS entropy source. Unseeded and unpredictable.",
        [],
        "let big = random()  // e.g. 7240982340198",
    ),
    builtin_doc!(
        "randomBelow",
        "A cryptographically-secure uniform random integer in [0, n), unbiased by rejection sampling. Returns Result<int, Error> when n is positive and Error otherwise.",
        ["n" => "Exclusive upper bound; must be positive"],
        "let d = randomBelow(6) ?: 0  // a fair die face 0..5",
    ),
];

/// Testing framework built-in documentation [TESTING-BUILTINS]
/// (docs/specs/0027-TestingFramework.md). Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static TESTING: &[BuiltinDoc] = &[
    builtin_doc!(
        "test",
        "Runs `body` as one named test case and prints a TAP result line. A case fails when any assertion inside it fails; the program exits non-zero if any case failed.",
        ["name" => "The test case's name", "body" => "A zero-parameter function containing the case's assertions"],
        "test(\"addition works\", fn() => expect(2 + 3, 5))",
    ),
    builtin_doc!(
        "expect",
        "Asserts two values are equal by canonical rendering. Success payloads compare by value; Errors remain visible as Error(message). On mismatch, marks the enclosing test failed and prints a diagnostic; execution continues.",
        ["actual" => "The computed value", "expected" => "The value it should equal"],
        "test(\"doubling\", fn() => expect(21 * 2, 42))",
    ),
    builtin_doc!(
        "expectAll",
        "Runs every boolean in a list literal as an independent soft assertion. All conditions run even when an earlier one fails.",
        ["conditions" => "A non-empty list literal of boolean conditions"],
        "expectAll([total == 42, name == \"Ada\", ready])",
    ),
    builtin_doc!(
        "expectTrue",
        "Asserts that a boolean expression is true.",
        ["actual" => "The boolean condition being asserted"],
        "expectTrue(total > 0)",
    ),
    builtin_doc!(
        "expectFalse",
        "Asserts that a boolean expression is false.",
        ["actual" => "The boolean condition being asserted"],
        "expectFalse(isEmpty(items))",
    ),
    builtin_doc!(
        "check",
        "Asserts expected equals actual and includes label in a mismatch diagnostic. Execution continues after a mismatch.",
        ["label" => "A short description of what is being checked", "expected" => "The value the actual must equal", "actual" => "The computed value"],
        "test(\"doubling\", fn() => check(\"double\", 42, 21 * 2))",
    ),
    builtin_doc!(
        "checkAll",
        "Runs every boolean in a list literal as an independent labeled soft assertion. All conditions run even when an earlier one fails.",
        ["label" => "A short label shared by the group", "conditions" => "A non-empty list literal of boolean conditions"],
        "checkAll(\"order state\", [total == 42, itemCount == 3, paid])",
    ),
    builtin_doc!(
        "checkTrue",
        "Labeled assertion that a boolean expression is true.",
        ["label" => "A short description", "actual" => "The boolean condition being asserted"],
        "checkTrue(\"positive total\", total > 0)",
    ),
    builtin_doc!(
        "checkFalse",
        "Labeled assertion that a boolean expression is false.",
        ["label" => "A short description", "actual" => "The boolean condition being asserted"],
        "checkFalse(\"cart is empty\", isEmpty(items))",
    ),
];

/// `strings` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static STRINGS: &[BuiltinDoc] = &[
    builtin_doc!(
        "contains",
        "True if needle appears anywhere in s. Empty needle returns true.",
        ["s" => "The string to search in", "needle" => "The substring to search for"],
        "let found = contains(\"hello world\", \"world\")  // true",
    ),
    builtin_doc!(
        "startsWith",
        "True if s begins with prefix.",
        ["s" => "The string to test", "prefix" => "The prefix to look for"],
        "startsWith(\"GET /api\", \"GET \")  // true",
    ),
    builtin_doc!(
        "endsWith",
        "True if s ends with suffix.",
        ["s" => "The string to test", "suffix" => "The suffix to look for"],
        "endsWith(\"image.png\", \".png\")  // true",
    ),
    builtin_doc!(
        "indexOf",
        "Returns the byte index of needle's first occurrence, or Error with 'indexOf: substring not found'.",
        ["s" => "The string to search in", "needle" => "The substring to locate"],
        "match indexOf(\"foo=bar\", \"=\") { Success { value } => print(value) ... }",
    ),
    builtin_doc!(
        "split",
        "Splits s on separator. An empty separator returns Error.",
        ["s" => "The string to split", "separator" => "Non-empty separator"],
        "split(\"a,b,c\", \",\")  // Success { value: [\"a\",\"b\",\"c\"] }",
    ),
    builtin_doc!(
        "join",
        "Concatenates parts with separator between each pair.",
        ["parts" => "Strings to join", "separator" => "Separator string"],
        "join([\"a\",\"b\",\"c\"], \"-\")  // \"a-b-c\"",
    ),
    builtin_doc!(
        "parseInt",
        "Strict base-10 signed-int parser. No whitespace tolerance.",
        ["s" => "The string to parse"],
        "parseInt(\"42\")  // Success { value: 42 }",
    ),
    builtin_doc!(
        "lines",
        "Splits on '\\n'. A trailing newline does not produce an empty entry.",
        ["s" => "The string to split"],
        "lines(\"a\\\nb\\\nc\")  // [\"a\",\"b\",\"c\"]",
    ),
    builtin_doc!(
        "words",
        "Splits on runs of whitespace; empty results dropped.",
        ["s" => "The string to split"],
        "words(\"a  b\\\\tc\")  // [\"a\",\"b\",\"c\"]",
    ),
    builtin_doc!(
        "replace",
        "Replaces every occurrence of needle. An empty needle returns Error.",
        ["s" => "The source string", "needle" => "The substring to find", "replacement" => "The replacement string"],
        "replace(\"a-b-c\", \"-\", \"_\")  // Success { value: \"a_b_c\" }",
    ),
    builtin_doc!(
        "repeat",
        "Concatenates s with itself n times. A negative n returns Error.",
        ["s" => "The string to repeat", "n" => "Repeat count, must be >= 0"],
        "repeat(\"ab\", 3)  // Success { value: \"ababab\" }",
    ),
    builtin_doc!(
        "substring",
        "Extracts s[start, end). Invalid or inverted bounds return Error.",
        ["s" => "The source string", "start" => "Starting index (inclusive)", "end" => "Ending index (exclusive)"],
        "substring(\"hello\", 1, 4)  // Success { value: \"ell\" }",
    ),
    builtin_doc!(
        "take",
        "Returns at most the first n bytes of s. Clamps; never fails.",
        ["s" => "The source string", "n" => "How many bytes to take"],
        "take(\"hello\", 3)  // \"hel\"",
    ),
    builtin_doc!(
        "drop",
        "Returns s without its first n bytes. Clamps; never fails.",
        ["s" => "The source string", "n" => "How many bytes to drop"],
        "drop(\"hello\", 3)  // \"lo\"",
    ),
    builtin_doc!(
        "isEmpty",
        "True if a string has zero bytes or a List/Map has zero elements.",
        ["s" => "The string, List, or Map to test"],
        "let blank = isEmpty(\"\")  // true",
    ),
    builtin_doc!(
        "parseFloat",
        "Strict finite base-10 floating-point parser. Rejects surrounding whitespace, NaN/infinity spellings, and hexadecimal floats.",
        ["s" => "The string to parse"],
        "parseFloat(\"3.14\")  // Success { value: 3.14 }",
    ),
    builtin_doc!(
        "padStart",
        "Pads s on the left with copies of fill to reach targetLength bytes.",
        ["s" => "The string to pad", "targetLength" => "Desired total length", "fill" => "Padding string (non-empty)"],
        "padStart(\"7\", 3, \"0\")  // Success { value: \"007\" }",
    ),
    builtin_doc!(
        "padEnd",
        "Pads s on the right with copies of fill to reach targetLength bytes.",
        ["s" => "The string to pad", "targetLength" => "Desired total length", "fill" => "Padding string (non-empty)"],
        "padEnd(\"7\", 3, \".\")  // Success { value: \"7..\" }",
    ),
    builtin_doc!(
        "byteLength",
        "Returns the number of bytes in the string's UTF-8 encoding.",
        ["text" => "The string to measure"],
        "let n = byteLength(\"héllo\")  // 6",
    ),
    builtin_doc!(
        "byteAt",
        "Returns the byte at the given index (0-255), or an error if the index is out of range.",
        ["text" => "The string to read from", "index" => "Zero-based byte offset"],
        "match byteAt(\"hi\", 0) {\n  Success { value } => print(\"byte: ${value}\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "codePointAt",
        "Returns the Unicode code point that begins at the given byte index. Fails on an invalid index or malformed UTF-8.",
        ["text" => "The string to read from", "index" => "Byte offset where the code point starts"],
        "match codePointAt(\"héllo\", 1) {\n  Success { value } => print(\"U+${value}\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "codePointWidth",
        "Returns how many bytes the given Unicode code point occupies in UTF-8 (1-4).",
        ["codePoint" => "The Unicode scalar value"],
        "match codePointWidth(233) {\n  Success { value } => print(\"${value} bytes\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "fromCodePoint",
        "Returns the single-character string for a nonzero Unicode scalar, or an error for U+0000 and non-scalars.",
        ["codePoint" => "The nonzero Unicode scalar value to encode"],
        "match fromCodePoint(233) {\n  Success { value } => print(value)  // é\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "toUpperCase",
        "Converts ASCII letters to uppercase; other bytes are unchanged.",
        ["s" => "The string to transform"],
        "toUpperCase(\"hello\")  // \"HELLO\"",
    ),
    builtin_doc!(
        "toLowerCase",
        "ASCII-aware lowercase.",
        ["s" => "The string to transform"],
        "toLowerCase(\"HELLO\")  // \"hello\"",
    ),
    builtin_doc!(
        "trim",
        "Removes leading and trailing whitespace.",
        ["s" => "The string to trim"],
        "trim(\"  hi  \")  // \"hi\"",
    ),
    builtin_doc!(
        "trimStart",
        "Removes leading whitespace.",
        ["s" => "The string to trim"],
        "trimStart(\"  hi  \")  // \"hi  \"",
    ),
    builtin_doc!(
        "trimEnd",
        "Removes trailing whitespace.",
        ["s" => "The string to trim"],
        "trimEnd(\"  hi  \")  // \"  hi\"",
    ),
    builtin_doc!(
        "reverse",
        "Reverses byte order.",
        ["s" => "The string to reverse"],
        "reverse(\"abc\")  // \"cba\"",
    ),
];

/// `functional` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static FUNCTIONAL: &[BuiltinDoc] = &[
    builtin_doc!(
        "forEach",
        "Applies a function to each element in an iterator.",
        ["iterator" => "The iterator to process", "function" => "The function to apply to each element"],
        "forEach(range(1, 4), fn(x) => print(x * 2 ?: 0))  // Prints: 2, 4, 6",
    ),
    builtin_doc!(
        "map",
        "Transforms each element in an iterator using a function, returning a new iterator.",
        ["iterator" => "The iterator to transform", "fn" => "The transformation function"],
        "let doubled = map(range(1, 4), fn(x) => x * 2 ?: 0)\nforEach(doubled, print)  // Prints: 2, 4, 6",
    ),
    builtin_doc!(
        "filter",
        "Filters elements in an iterator based on a predicate function.",
        ["iterator" => "The iterator to filter", "predicate" => "The predicate function that returns true for elements to keep"],
        "let evens = filter(range(1, 6), fn(x) => (x % 2 ?: 1) == 0)\nforEach(evens, print)  // Prints: 2, 4",
    ),
    builtin_doc!(
        "fold",
        "Reduces an iterator to a single value by repeatedly applying a function.",
        ["iterator" => "The iterator to reduce", "initial" => "The initial value for the accumulator", "fn" => "The reduction function that takes (accumulator, current) and returns new accumulator"],
        "range(1, 5) |> fold(0, add)  // sum: 0+1+2+3+4 = 10",
    ),
];

/// `lists` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
///
/// List surface from docs/specs/0012-Built-InFunctions.md:
/// [BUILTIN-LIST],
/// [BUILTIN-LIST-GET], [BUILTIN-LIST-APPEND], [BUILTIN-LIST-PREPEND],
/// [BUILTIN-LIST-CONCAT], [BUILTIN-LIST-REVERSE], [BUILTIN-LIST-CONTAINS] and
/// [BUILTIN-COLLECTION-LENGTH].
pub(crate) static LISTS: &[BuiltinDoc] = &[
    builtin_doc!(
        "List",
        "Creates a new empty list.",
        [],
        "let myList = List()\nprint(\"Created empty list\")",
    ),
    builtin_doc!(
        "listAppend",
        "Returns a new list with value at the end. Amortized O(1).",
        ["list" => "The list", "value" => "Value to append"],
        "listAppend([1, 2], 3)  // [1, 2, 3]",
    ),
    builtin_doc!(
        "listPrepend",
        "Returns a new list with value at the front. O(n).",
        ["list" => "The list", "value" => "Value to prepend"],
        "listPrepend([2, 3], 1)  // [1, 2, 3]",
    ),
    builtin_doc!(
        "listConcat",
        "Returns left ++ right. Same as left + right.",
        ["left" => "Left operand", "right" => "Right operand"],
        "listConcat([1, 2], [3, 4])  // [1, 2, 3, 4]",
    ),
    builtin_doc!(
        "listReverse",
        "Returns a new list in reverse order.",
        ["list" => "The list"],
        "listReverse([1, 2, 3])  // [3, 2, 1]",
    ),
    builtin_doc!(
        "listLength",
        "Returns the number of elements in a list. O(1).",
        ["list" => "The list"],
        "listLength([1, 2, 3])  // 3",
    ),
    builtin_doc!(
        "listGet",
        "Returns the element at the given index, or an error if the index is out of range.",
        ["list" => "The list to read from", "index" => "Zero-based element index"],
        "match listGet(myList, 0) {\n  Success { value } => print(value)\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "listContains",
        "Linear search. Strings compare by content, scalars by value, and managed handles by identity.",
        ["list" => "The list", "value" => "Value to find"],
        "listContains([1, 2, 3], 2)  // true",
    ),
    builtin_doc!(
        "forEachList",
        "Applies function to every list element in index order.",
        ["list" => "The list", "function" => "Function applied per element"],
        "forEachList(xs, print)",
    ),
];

/// `maps` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
///
/// Map surface from docs/specs/0012-Built-InFunctions.md:
/// [BUILTIN-MAP],
/// [BUILTIN-MAP-GET], [BUILTIN-MAP-SET], [BUILTIN-MAP-REMOVE],
/// [BUILTIN-MAP-MERGE], [BUILTIN-MAP-CONTAINS] and [BUILTIN-COLLECTION-LENGTH].
/// The `mapKeys` / `mapValues` entries below are the arity-1 accessors
/// [BUILTIN-MAP-KEYS] / [BUILTIN-MAP-VALUES].
pub(crate) static MAPS: &[BuiltinDoc] = &[
    builtin_doc!(
        "Map",
        "Creates a new empty string-keyed map.",
        [],
        "let m = Map()",
    ),
    builtin_doc!(
        "mapSet",
        "Returns a new map with key bound to value (replaces prior binding).",
        ["map" => "The map", "key" => "Key", "value" => "Value"],
        "mapSet({\"a\": 1}, \"b\", 2)  // {\"a\": 1, \"b\": 2}",
    ),
    builtin_doc!(
        "mapGet",
        "Returns the value associated with the key, or an error if the key is absent.",
        ["map" => "The map to look up in", "key" => "The key to find"],
        "match mapGet(scores, \"alice\") {\n  Success { value } => print(value)\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "mapRemove",
        "Returns a new map without key. No-op if key is absent.",
        ["map" => "The map", "key" => "Key"],
        "mapRemove({\"a\": 1, \"b\": 2}, \"a\")  // {\"b\": 2}",
    ),
    builtin_doc!(
        "mapMerge",
        "Right-biased union. Same as left + right.",
        ["left" => "Left", "right" => "Right"],
        "mapMerge({\"a\": 1}, {\"b\": 2})  // {\"a\": 1, \"b\": 2}",
    ),
    builtin_doc!(
        "mapContains",
        "True iff key is present in map.",
        ["map" => "The map", "key" => "Key to find"],
        "mapContains({\"a\": 1}, \"a\")  // true",
    ),
    builtin_doc!(
        "mapLength",
        "Returns the number of entries in a map. O(1).",
        ["map" => "The map"],
        "mapLength({\"a\": 1, \"b\": 2})  // 2",
    ),
    builtin_doc!(
        "mapKeys",
        "All keys of the map as a list. Order unspecified.",
        ["map" => "The map"],
        "mapKeys(m)  // List<string>",
    ),
    builtin_doc!(
        "mapValues",
        "All values of the map as a list. Order matches mapKeys.",
        ["map" => "The map"],
        "mapValues(m)  // List<V>",
    ),
];

/// GPU computation built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
///
/// GPU surface from docs/specs/0034-GPUComputation.md: [GPU-BUFFER-FROM-LIST],
/// [GPU-BUFFER-TO-LIST], [GPU-BUFFER-LENGTH], [GPU-MAP], [GPU-FOLD],
/// [GPU-ZIPWITH], [GPU-IOTA], [GPU-GET], [GPU-SCAN], [GPU-FILTER],
/// [GPU-DEVICE].
pub(crate) static GPU: &[BuiltinDoc] = &[
    builtin_doc!(
        "toGpu",
        "Copies a list of scalars (int, float, or bool) into a dense GpuBuffer.",
        ["list" => "The scalar list to copy into a buffer"],
        "let buf = toGpu([1, 2, 3, 4])",
    ),
    builtin_doc!(
        "fromGpu",
        "Materializes a GpuBuffer back into a host list.",
        ["buffer" => "The buffer to copy back to a list"],
        "fromGpu(toGpu([1, 2])) |> forEachList(print)  // Prints: 1, 2",
    ),
    builtin_doc!(
        "gpuLength",
        "Returns a GpuBuffer's element count. O(1).",
        ["buffer" => "The buffer to measure"],
        "gpuLength(toGpu([1, 2, 3]))  // 3",
    ),
    builtin_doc!(
        "gpuMap",
        "Applies a pure kernel to every buffer element independently. The compiler rejects a kernel that performs any effect.",
        ["buffer" => "The source buffer", "kernel" => "The pure per-element function"],
        "toGpu([1, 2, 3]) |> gpuMap(fn(x) => (x * x) ?: 0)",
    ),
    builtin_doc!(
        "gpuFold",
        "Reduces a buffer to one scalar with a pure combine function. Use an associative combine: a device backend may reassociate.",
        ["buffer" => "The buffer to reduce", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "toGpu([1, 2, 3]) |> gpuFold(0, fn(a, x) => (a + x) ?: a)",
    ),
    builtin_doc!(
        "gpuZipWith",
        "Combines two buffers elementwise with a pure binary kernel. The result takes the shorter operand's length.",
        ["a" => "The left buffer", "b" => "The right buffer", "kernel" => "The pure (a, b) element function"],
        "gpuZipWith(xs, ys, fn(x, y) => (x * y) ?: 0)  // elementwise product",
    ),
    builtin_doc!(
        "gpuIota",
        "Builds the index buffer [0, n). Gather, stencil, and matrix addressing start here.",
        ["n" => "The element count"],
        "gpuIota(4) |> fromGpu()  // [0, 1, 2, 3]",
    ),
    builtin_doc!(
        "gpuGet",
        "Bounds-checked read of one element at the buffer's element type. Out of bounds returns Error.",
        ["buffer" => "The buffer to read", "index" => "The element index"],
        "gpuGet(toGpu([10, 20]), 1) ?: 0  // 20",
    ),
    builtin_doc!(
        "gpuScan",
        "Inclusive prefix scan: element i is combine folded through element i. Use an associative combine: a device backend may run it work-efficiently in parallel.",
        ["buffer" => "The buffer to scan", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "toGpu([1, 2, 3]) |> gpuScan(0, fn(a, x) => (a + x) ?: a)  // 1, 3, 6",
    ),
    builtin_doc!(
        "gpuFilter",
        "Stream compaction: keeps the elements a pure predicate accepts, preserving order.",
        ["buffer" => "The source buffer", "predicate" => "The pure element predicate"],
        "toGpu([1, 2, 3, 4]) |> gpuFilter(fn(x) => (x % 2 ?: 0) == 0)",
    ),
    builtin_doc!(
        "gpuDevice",
        "Returns the active GPU execution backend's name. The host backend reports \"host\"; device backends report names like \"cuda:0\".",
        [],
        "print(gpuDevice())  // host",
    ),
];
