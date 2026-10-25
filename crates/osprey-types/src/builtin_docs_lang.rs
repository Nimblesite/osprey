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

/// Testing framework built-in documentation [TESTING-BUILTINS]
/// (docs/specs/language/0027-TestingFramework.md). Prose only — types come from the
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
        "test(\"order total\", fn() => expectAll([wrapMul(6, 7) == 42, length(\"Ada\") == 3]))",
    ),
    builtin_doc!(
        "expectTrue",
        "Asserts that a boolean expression is true.",
        ["actual" => "The boolean condition being asserted"],
        "test(\"positive length\", fn() => expectTrue(length(\"Ada\") > 0))",
    ),
    builtin_doc!(
        "expectFalse",
        "Asserts that a boolean expression is false.",
        ["actual" => "The boolean condition being asserted"],
        "test(\"non-empty list\", fn() => expectFalse(isEmpty([1])))",
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
        "test(\"order state\", fn() => checkAll(\"order totals\", [wrapMul(6, 7) == 42, listLength([1, 2, 3]) == 3]))",
    ),
    builtin_doc!(
        "checkTrue",
        "Labeled assertion that a boolean expression is true.",
        ["label" => "A short description", "actual" => "The boolean condition being asserted"],
        "test(\"positive length\", fn() => checkTrue(\"name length\", length(\"Ada\") > 0))",
    ),
    builtin_doc!(
        "checkFalse",
        "Labeled assertion that a boolean expression is false.",
        ["label" => "A short description", "actual" => "The boolean condition being asserted"],
        "test(\"non-empty list\", fn() => checkFalse(\"has item\", isEmpty([1])))",
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
        "print(startsWith(\"GET /api\", \"GET \"))  // true",
    ),
    builtin_doc!(
        "endsWith",
        "True if s ends with suffix.",
        ["s" => "The string to test", "suffix" => "The suffix to look for"],
        "print(endsWith(\"image.png\", \".png\"))  // true",
    ),
    builtin_doc!(
        "indexOf",
        "Returns the byte index of needle's first occurrence, or Error with 'indexOf: substring not found'.",
        ["s" => "The string to search in", "needle" => "The substring to locate"],
        "match indexOf(\"foo=bar\", \"=\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "split",
        "Splits s on separator. An empty separator returns Error.",
        ["s" => "The string to split", "separator" => "Non-empty separator"],
        "match split(\"a,b,c\", \",\") {\n    Success { value } => print(join(value, \"-\"))\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "join",
        "Concatenates parts with separator between each pair.",
        ["parts" => "Strings to join", "separator" => "Separator string"],
        "print(join([\"a\",\"b\",\"c\"], \"-\"))  // \"a-b-c\"",
    ),
    builtin_doc!(
        "parseInt",
        "Strict base-10 signed-int parser. No whitespace tolerance.",
        ["s" => "The string to parse"],
        "match parseInt(\"42\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "lines",
        "Splits on '\\n'. A trailing newline does not produce an empty entry.",
        ["s" => "The string to split"],
        "let parts = lines(\"a\\nb\\nc\")\nprint(join(parts, \",\"))",
    ),
    builtin_doc!(
        "words",
        "Splits on runs of ASCII whitespace; empty results are dropped.",
        ["s" => "The string to split"],
        "print(join(words(\"a  b\\tc\"), \",\"))",
    ),
    builtin_doc!(
        "replace",
        "Replaces every occurrence of needle. An empty needle returns Error.",
        ["s" => "The source string", "needle" => "The substring to find", "replacement" => "The replacement string"],
        "match replace(\"a-b-c\", \"-\", \"_\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "repeat",
        "Concatenates s with itself n times. A negative n returns Error.",
        ["s" => "The string to repeat", "n" => "Repeat count, must be >= 0"],
        "match repeat(\"ab\", 3) {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "substring",
        "Extracts the byte interval s[start, end). Invalid or inverted bounds return Error.",
        ["s" => "The source string", "start" => "Starting byte offset (inclusive)", "end" => "Ending byte offset (exclusive)"],
        "match substring(\"hello\", 1, 4) {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "take",
        "Returns at most the first n bytes of s. Clamps; never fails.",
        ["s" => "The source string", "n" => "How many bytes to take"],
        "print(take(\"hello\", 3))  // \"hel\"",
    ),
    builtin_doc!(
        "drop",
        "Returns s without its first n bytes. Clamps; never fails.",
        ["s" => "The source string", "n" => "How many bytes to drop"],
        "print(drop(\"hello\", 3))  // \"lo\"",
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
        "match parseFloat(\"3.14\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "padStart",
        "Pads s on the left with copies of fill to reach targetLength bytes.",
        ["s" => "The string to pad", "targetLength" => "Desired total length", "fill" => "Padding string (non-empty)"],
        "match padStart(\"7\", 3, \"0\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "padEnd",
        "Pads s on the right with copies of fill to reach targetLength bytes.",
        ["s" => "The string to pad", "targetLength" => "Desired total length", "fill" => "Padding string (non-empty)"],
        "match padEnd(\"7\", 3, \".\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
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
        "match codePointAt(\"héllo\", 1) {\n  Success { value } => print(\"code point (decimal): ${value}\")\n  Error { message } => print(message)\n}",
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
        "print(toUpperCase(\"hello\"))  // \"HELLO\"",
    ),
    builtin_doc!(
        "toLowerCase",
        "ASCII-aware lowercase.",
        ["s" => "The string to transform"],
        "print(toLowerCase(\"HELLO\"))  // \"hello\"",
    ),
    builtin_doc!(
        "trim",
        "Removes leading and trailing ASCII whitespace.",
        ["s" => "The string to trim"],
        "print(trim(\"  hi  \"))  // \"hi\"",
    ),
    builtin_doc!(
        "trimStart",
        "Removes leading ASCII whitespace.",
        ["s" => "The string to trim"],
        "print(trimStart(\"  hi  \"))  // \"hi  \"",
    ),
    builtin_doc!(
        "trimEnd",
        "Removes trailing ASCII whitespace.",
        ["s" => "The string to trim"],
        "print(trimEnd(\"  hi  \"))  // \"  hi\"",
    ),
    builtin_doc!(
        "reverse",
        "Reverses byte order.",
        ["s" => "The string to reverse"],
        "print(reverse(\"abc\"))  // \"cba\"",
    ),
];

/// `functional` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static FUNCTIONAL: &[BuiltinDoc] = &[
    builtin_doc!(
        "forEach",
        "Applies a function to each element in an iterator.",
        ["iterator" => "The iterator to process", "function" => "The function to apply to each element"],
        "forEach(range(1, 4), fn(x) => print(satMul(x, 2)))  // Prints: 2, 4, 6",
    ),
    builtin_doc!(
        "map",
        "Transforms each element in an iterator using a function, returning a new iterator.",
        ["iterator" => "The iterator to transform", "fn" => "The transformation function"],
        "let doubled = map(range(1, 4), fn(x) => satMul(x, 2))\nforEach(doubled, print)  // Prints: 2, 4, 6",
    ),
    builtin_doc!(
        "filter",
        "Filters elements in an iterator based on a predicate function.",
        ["iterator" => "The iterator to filter", "predicate" => "The predicate function that returns true for elements to keep"],
        "let evens = filter(range(1, 6), fn(x) => x % 2 == 0)\nforEach(evens, print)  // Prints: 2, 4",
    ),
    builtin_doc!(
        "fold",
        "Reduces an iterator to a single value by repeatedly applying a function.",
        ["iterator" => "The iterator to reduce", "initial" => "The initial value for the accumulator", "fn" => "The reduction function that takes (accumulator, current) and returns new accumulator"],
        "print(range(1, 5) |> fold(0, wrapAdd))  // 10",
    ),
    builtin_doc!(
        "toList",
        "Runs an iterator pipeline once and collects every element it yields into a list, in order.",
        ["iterator" => "The iterator to collect"],
        "let evens = range(1, 7) |> filter(fn(x) => x % 2 == 0) |> toList  // [2, 4, 6]",
    ),
];

/// `lists` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
///
/// List surface from docs/specs/runtime/0012-Built-InFunctions.md:
/// [BUILTIN-LIST],
/// [BUILTIN-LIST-GET], [BUILTIN-LIST-APPEND], [BUILTIN-LIST-PREPEND],
/// [BUILTIN-LIST-CONCAT], [BUILTIN-LIST-REVERSE], [BUILTIN-LIST-CONTAINS],
/// [BUILTIN-LIST-FOREACH], [BUILTIN-LIST-MAP], [BUILTIN-LIST-FILTER],
/// [BUILTIN-LIST-FOLD], [BUILTIN-LIST-SORT] and
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
        "let appended = listAppend([1, 2], 3)  // [1, 2, 3]",
    ),
    builtin_doc!(
        "listPrepend",
        "Returns a new list with value at the front. O(n).",
        ["list" => "The list", "value" => "Value to prepend"],
        "let prepended = listPrepend([2, 3], 1)  // [1, 2, 3]",
    ),
    builtin_doc!(
        "listConcat",
        "Returns left ++ right. Same as left + right.",
        ["left" => "Left operand", "right" => "Right operand"],
        "let combined = listConcat([1, 2], [3, 4])  // [1, 2, 3, 4]",
    ),
    builtin_doc!(
        "listReverse",
        "Returns a new list in reverse order.",
        ["list" => "The list"],
        "let reversed = listReverse([1, 2, 3])  // [3, 2, 1]",
    ),
    builtin_doc!(
        "listLength",
        "Returns the number of elements in a list. O(1).",
        ["list" => "The list"],
        "print(listLength([1, 2, 3]))  // 3",
    ),
    builtin_doc!(
        "listGet",
        "Returns the element at the given index, or an error if the index is out of range.",
        ["list" => "The list to read from", "index" => "Zero-based element index"],
        "match listGet([10, 20], 0) {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "listContains",
        "Linear search. Strings compare by content, scalars by value, and managed handles by identity.",
        ["list" => "The list", "value" => "Value to find"],
        "print(listContains([1, 2, 3], 2))  // true",
    ),
    builtin_doc!(
        "structuralEquals",
        "Compares two values by their contents, walking both completely, so its cost grows with their size; `==` deliberately does not do this. Both operands must be one type whose every part has a decided equality — a function, a runtime handle, `any` or `Result` anywhere inside is a compile error. NaN stays unequal to itself, including inside a record, list, map or union.",
        ["a" => "The first value", "b" => "The second value, of the same type"],
        "type Route = { tab: string, depth: int }\nlet same = structuralEquals(Route { tab: \"home\", depth: 1 }, Route { tab: \"home\", depth: 1 })  // true\nlet differ = structuralEquals([1, 2], [1, 2, 3])  // false",
    ),
    builtin_doc!(
        "forEachList",
        "Applies function to every list element in index order.",
        ["list" => "The list", "function" => "Function applied per element"],
        "forEachList([1, 2, 3], print)",
    ),
    builtin_doc!(
        "mapList",
        "Returns a new list by applying a function to every element, in index order.",
        ["list" => "The list", "function" => "The transformation function"],
        "let doubled = mapList([1, 2, 3], fn(n) => satMul(n, 2))  // [2, 4, 6]",
    ),
    builtin_doc!(
        "filterList",
        "Returns a new list keeping the elements that satisfy a predicate, in index order.",
        ["list" => "The list", "predicate" => "The predicate that returns true for elements to keep"],
        "let evens = filterList([1, 2, 3, 4], fn(n) => n % 2 == 0)  // [2, 4]",
    ),
    builtin_doc!(
        "foldList",
        "Reduces a list to a single value by repeatedly applying a function.",
        ["list" => "The list to reduce", "initial" => "The initial value for the accumulator", "function" => "The reduction function that takes (accumulator, current) and returns the new accumulator"],
        "print(foldList([1, 2, 3], 0, wrapAdd))  // 6",
    ),
    builtin_doc!(
        "listSort",
        "Returns a new list sorted in ascending order by the elements' natural order: integers by value, strings by byte content. The sort is stable. Only List<int> and List<string> are supported; any other element type is a compile-time error.",
        ["list" => "The list to sort"],
        "let sorted = listSort([3, 1, 2])  // [1, 2, 3]",
    ),
    builtin_doc!(
        "listSortBy",
        "Returns a new list sorted by a comparator, for any element type. The comparator returns a negative int when its first argument comes first, a positive int when it comes second and zero to keep the two in their original order: the sort is stable.",
        ["list" => "The list to sort", "compare" => "Orders two elements: negative, zero or positive"],
        "let ordered = listSortBy([\"bb\", \"a\"], fn(x, y) => satSub(length(x), length(y)))  // [\"a\", \"bb\"]",
    ),
];

/// `maps` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
///
/// Map surface from docs/specs/runtime/0012-Built-InFunctions.md:
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
        "let updated = mapSet({\"a\": 1}, \"b\", 2)  // {\"a\": 1, \"b\": 2}",
    ),
    builtin_doc!(
        "mapGet",
        "Returns the value associated with the key, or an error if the key is absent.",
        ["map" => "The map to look up in", "key" => "The key to find"],
        "match mapGet({\"alice\": 42}, \"alice\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "mapRemove",
        "Returns a new map without key. No-op if key is absent.",
        ["map" => "The map", "key" => "Key"],
        "let remaining = mapRemove({\"a\": 1, \"b\": 2}, \"a\")  // {\"b\": 2}",
    ),
    builtin_doc!(
        "mapMerge",
        "Right-biased union. Same as left + right.",
        ["left" => "Left", "right" => "Right"],
        "let combined = mapMerge({\"a\": 1}, {\"b\": 2})  // {\"a\": 1, \"b\": 2}",
    ),
    builtin_doc!(
        "mapContains",
        "True iff key is present in map.",
        ["map" => "The map", "key" => "Key to find"],
        "print(mapContains({\"a\": 1}, \"a\"))  // true",
    ),
    builtin_doc!(
        "mapLength",
        "Returns the number of entries in a map. O(1).",
        ["map" => "The map"],
        "print(mapLength({\"a\": 1, \"b\": 2}))  // 2",
    ),
    builtin_doc!(
        "mapKeys",
        "All keys of the map as a list. Order unspecified.",
        ["map" => "The map"],
        "let keys = mapKeys({\"a\": 1})  // [\"a\"]",
    ),
    builtin_doc!(
        "mapValues",
        "All values of the map as a list. Order matches mapKeys.",
        ["map" => "The map"],
        "let values = mapValues({\"a\": 1})  // [1]",
    ),
];
