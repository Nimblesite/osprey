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
        "Prints a value to the console. Automatically converts the value to a string representation.",
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
        "Converts a value to its string representation.",
        ["value" => "The value to convert to string"],
        "let str = toString(42)\nprint(str)  // Prints: 42",
    ),
    builtin_doc!(
        "length",
        "Returns the byte length of a string. Total — never fails.",
        ["s" => "The string to measure"],
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
        "forEach(range(0, 5), fn(x) { print(x) })  // Prints: 0, 1, 2, 3, 4",
    ),
    builtin_doc!(
        "abs",
        "Returns the absolute value of an integer.",
        ["value" => "The integer whose magnitude to take"],
        "let d = abs(0 - 5)  // 5",
    ),
    builtin_doc!(
        "intDiv",
        "Truncating integer division (rounds toward zero), divide-by-zero checked. The `/` operator is float-only; this is its integer sibling, returning Result<int, MathError>.",
        ["a" => "The dividend", "b" => "The divisor (zero yields Error)"],
        "fn half(n) = intDiv(n, 2)  // intDiv(7, 2) == 3",
    ),
    builtin_doc!(
        "random",
        "A cryptographically-secure uniform random non-negative integer (0 .. 2^63-1), drawn fresh from the OS entropy source. Unseeded and unpredictable.",
        [],
        "let big = random()  // e.g. 7240982340198",
    ),
    builtin_doc!(
        "randomBelow",
        "A cryptographically-secure uniform random integer in [0, n), unbiased by rejection sampling. Returns Result<int, MathError> — Error when n <= 0.",
        ["n" => "Exclusive upper bound; must be positive"],
        "let d = randomBelow(6) ?: 0  // a fair die face 0..5",
    ),
    builtin_doc!(
        "not",
        "Returns the logical negation of a boolean.",
        ["value" => "The boolean to negate"],
        "let off = not(true)  // false",
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
        "Asserts two values are equal (canonical-string equality, Results auto-unwrapped). On mismatch, marks the enclosing test failed and prints a diagnostic; execution continues.",
        ["actual" => "The computed value", "expected" => "The value it should equal"],
        "test(\"doubling\", fn() => expect(21 * 2, 42))",
    ),
    builtin_doc!(
        "check",
        "Labeled equality assertion in Alcotest argument order (expected before actual). Behaves exactly like expect, with the label in the failure diagnostic.",
        ["label" => "A short description of what is being checked", "expected" => "The value the actual must equal", "actual" => "The computed value"],
        "test(\"doubling\", fn() => check(\"double\", 42, 21 * 2))",
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
        "Returns byte-index of first occurrence of needle, or Error(NotFound).",
        ["s" => "The string to search in", "needle" => "The substring to locate"],
        "match indexOf(\"foo=bar\", \"=\") { Success { value } => print(value) ... }",
    ),
    builtin_doc!(
        "split",
        "Splits s on separator. Error(InvalidArgument) on empty separator.",
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
        "Replaces every occurrence of needle. Error(InvalidArgument) on empty needle.",
        ["s" => "The source string", "needle" => "The substring to find", "replacement" => "The replacement string"],
        "replace(\"a-b-c\", \"-\", \"_\")  // Success { value: \"a_b_c\" }",
    ),
    builtin_doc!(
        "repeat",
        "Concatenates s with itself n times. Error(InvalidArgument) on negative n.",
        ["s" => "The string to repeat", "n" => "Repeat count, must be >= 0"],
        "repeat(\"ab\", 3)  // Success { value: \"ababab\" }",
    ),
    builtin_doc!(
        "substring",
        "Extracts s[start, end). Returns Error(IndexOutOfRange) if start<0, end>len, or start>end.",
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
        "True if string has zero length.",
        ["s" => "The string to test"],
        "let blank = isEmpty(\"\")  // true",
    ),
    builtin_doc!(
        "parseFloat",
        "Strict base-10 floating-point parser. No whitespace tolerance.",
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
        "Returns the single-character string for a Unicode code point, or an error if it is not a valid scalar value.",
        ["codePoint" => "The Unicode scalar value to encode"],
        "match fromCodePoint(233) {\n  Success { value } => print(value)  // é\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "toUpperCase",
        "ASCII-aware uppercase. Unicode simple case mapping is a future addition.",
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
        "Reverses byte order. Grapheme-cluster reversal is future work.",
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
        "forEach(range(1, 4), fn(x) { print(x * 2) })  // Prints: 2, 4, 6",
    ),
    builtin_doc!(
        "map",
        "Transforms each element in an iterator using a function, returning a new iterator.",
        ["iterator" => "The iterator to transform", "fn" => "The transformation function"],
        "let doubled = map(range(1, 4), fn(x) { x * 2 })\nforEach(doubled, print)  // Prints: 2, 4, 6",
    ),
    builtin_doc!(
        "filter",
        "Filters elements in an iterator based on a predicate function.",
        ["iterator" => "The iterator to filter", "predicate" => "The predicate function that returns true for elements to keep"],
        "let evens = filter(range(1, 6), fn(x) { x % 2 == 0 })\nforEach(evens, print)  // Prints: 2, 4",
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
pub(crate) static LISTS: &[BuiltinDoc] = &[
    builtin_doc!(
        "List",
        "Creates a new empty list.",
        [],
        "let myList = List()\nprint(\"Created empty list\")",
    ),
    builtin_doc!(
        "listAppend",
        "Returns a new list with value at the end. O(log32 n) amortised.",
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
        "True iff some element equals value. O(n).",
        ["list" => "The list", "value" => "Value to find"],
        "listContains([1, 2, 3], 2)  // true",
    ),
    builtin_doc!(
        "forEachList",
        "Apply function to every element of list. Phase 7 of collections plan.",
        ["list" => "The list", "function" => "Function applied per element"],
        "forEachList(xs, print)",
    ),
];

/// `maps` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static MAPS: &[BuiltinDoc] = &[
    builtin_doc!(
        "Map",
        "Creates a new, empty persistent map.",
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
        "mapKeys(m)  // List<K>",
    ),
    builtin_doc!(
        "mapValues",
        "All values of the map as a list. Order matches mapKeys.",
        ["map" => "The map"],
        "mapValues(m)  // List<V>",
    ),
];
