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
        "Returns the absolute value with the same numeric type as its input: t0 is int or float. Integer INT64_MIN performs Arith.overflow; the enclosing policy supplies the result. Float absolute value is total: negative zero becomes positive zero, infinities become positive infinity, and NaN remains NaN.",
        ["value" => "The int or float whose magnitude to take"],
        "let d = abs(0 - 5)  // int: 5\nlet f = abs(-2.5)   // float: 2.5",
    ),
    builtin_doc!(
        "intDiv",
        "Truncating integer division returning a plain int. A zero divisor performs Arith.remainderByZero and INT64_MIN / -1 performs Arith.overflow; a literal divisor other than 0 and -1 is total.",
        ["a" => "The dividend", "b" => "The divisor"],
        "fn half(n) = intDiv(n, 2)  // half(7) == 3",
    ),
    builtin_doc!(
        "wrapAdd",
        "Total two's-complement addition: overflow wraps and performs no Arith operation.",
        ["a" => "The first addend", "b" => "The second addend"],
        "let h = wrapAdd(9223372036854775807, 1)  // -9223372036854775808",
    ),
    builtin_doc!(
        "wrapSub",
        "Total two's-complement subtraction: overflow wraps and performs no Arith operation.",
        ["a" => "The minuend", "b" => "The subtrahend"],
        "let h = wrapSub(-9223372036854775808, 1)  // 9223372036854775807",
    ),
    builtin_doc!(
        "wrapMul",
        "Total two's-complement multiplication: overflow wraps and performs no Arith operation.",
        ["a" => "The first factor", "b" => "The second factor"],
        "let h = wrapMul(4294967296, 4294967296)  // 0",
    ),
    builtin_doc!(
        "satAdd",
        "Total saturating addition: overflow clamps to the int range and performs no Arith operation.",
        ["a" => "The first addend", "b" => "The second addend"],
        "let s = satAdd(9223372036854775807, 1)  // 9223372036854775807",
    ),
    builtin_doc!(
        "satSub",
        "Total saturating subtraction: overflow clamps to the int range and performs no Arith operation.",
        ["a" => "The minuend", "b" => "The subtrahend"],
        "let s = satSub(-9223372036854775808, 1)  // -9223372036854775808",
    ),
    builtin_doc!(
        "satMul",
        "Total saturating multiplication: overflow clamps to the int range and performs no Arith operation.",
        ["a" => "The first factor", "b" => "The second factor"],
        "let s = satMul(4294967296, -4294967296)  // -9223372036854775808",
    ),
    builtin_doc!(
        "toFloat",
        "Widens an int to a float, rounding to nearest even. Exact for magnitudes up to 2^53. Total, so it returns a bare float rather than a Result. This is the explicit element conversion GPU kernels use; there is no implicit widening at a buffer boundary.",
        ["value" => "The integer to widen"],
        "let xs = gpuIota(1000) |> gpuMap(toFloat)  // a float buffer 0.0 .. 999.0",
    ),
    builtin_doc!(
        "sqrt",
        "The square root of a float. Total over IEEE doubles: a negative argument yields NaN. Pure, so a kernel may call it or name it.",
        ["x" => "The float to take the root of"],
        "sqrt(2.0)  // 1.4142135623730951",
    ),
    builtin_doc!(
        "exp",
        "e raised to a float. Total: a large argument yields infinity, never an Arith operation. Pure, so softmax kernels name it directly.",
        ["x" => "The exponent"],
        "scores |> gpuMap(exp)",
    ),
    builtin_doc!(
        "ln",
        "The natural logarithm of a float. Total: zero yields negative infinity and a negative argument NaN. Named ln so programs keep the name log for their loggers.",
        ["x" => "The float to take the logarithm of"],
        "ln(1.0)  // 0.0",
    ),
    builtin_doc!(
        "tanh",
        "The hyperbolic tangent of a float, in (-1, 1). Total; the activation GELU is written from.",
        ["x" => "The float argument"],
        "tanh(0.0)  // 0.0",
    ),
    builtin_doc!(
        "sin",
        "The sine of a float in radians. Total over IEEE doubles; pure, so a kernel may call it or name it.",
        ["x" => "The angle in radians"],
        "sin(1.0)  // 0.8414709848078965",
    ),
    builtin_doc!(
        "cos",
        "The cosine of a float in radians. Total over IEEE doubles; pure, so a kernel may call it or name it.",
        ["x" => "The angle in radians"],
        "cos(1.0)  // 0.5403023058681398",
    ),
    builtin_doc!(
        "tan",
        "The tangent of a float in radians. Total: an argument at a pole yields the large finite neighbour, never a fault.",
        ["x" => "The angle in radians"],
        "tan(0.5)  // 0.5463024898437905",
    ),
    builtin_doc!(
        "asin",
        "The arc sine in radians. Total: an argument outside [-1, 1] yields NaN.",
        ["x" => "The value whose arc sine is taken"],
        "asin(0.5)  // 0.5235987755982988",
    ),
    builtin_doc!(
        "acos",
        "The arc cosine in radians. Total: an argument outside [-1, 1] yields NaN.",
        ["x" => "The value whose arc cosine is taken"],
        "acos(0.5)  // 1.0471975511965976",
    ),
    builtin_doc!(
        "atan",
        "The arc tangent in radians, in (-pi/2, pi/2). Total.",
        ["x" => "The value whose arc tangent is taken"],
        "atan(0.5)  // 0.46364760900080615",
    ),
    builtin_doc!(
        "atan2",
        "The arc tangent of y/x in radians, quadrant-aware: the angle of the point (x, y). Total; both zero yields NaN.",
        ["y" => "The point's y coordinate", "x" => "The point's x coordinate"],
        "atan2(1.0, -1.0)  // 2.356194490192345",
    ),
    builtin_doc!(
        "sinh",
        "The hyperbolic sine. Total: a large argument yields infinity.",
        ["x" => "The float argument"],
        "sinh(1.0)  // 1.1752011936438014",
    ),
    builtin_doc!(
        "cosh",
        "The hyperbolic cosine, >= 1.0. Total: a large argument yields infinity.",
        ["x" => "The float argument"],
        "cosh(1.0)  // 1.5430806348152437",
    ),
    builtin_doc!(
        "asinh",
        "The inverse hyperbolic sine. Total.",
        ["x" => "The float argument"],
        "asinh(1.0)  // 0.881373587019543",
    ),
    builtin_doc!(
        "acosh",
        "The inverse hyperbolic cosine. Total: an argument below 1.0 yields NaN.",
        ["x" => "The float argument"],
        "acosh(2.0)  // 1.3169578969248166",
    ),
    builtin_doc!(
        "atanh",
        "The inverse hyperbolic tangent. Total: |x| > 1 yields NaN, and exactly 1 yields infinity.",
        ["x" => "The float argument"],
        "atanh(0.5)  // 0.5493061443340549",
    ),
    builtin_doc!(
        "log2",
        "The base-2 logarithm. Total: zero yields negative infinity, a negative argument NaN.",
        ["x" => "The float to take the logarithm of"],
        "log2(8.0)  // 3.0",
    ),
    builtin_doc!(
        "log10",
        "The base-10 logarithm. Total: zero yields negative infinity, a negative argument NaN.",
        ["x" => "The float to take the logarithm of"],
        "log10(1000.0)  // 3.0",
    ),
    builtin_doc!(
        "log1p",
        "ln(1 + x) computed without the cancellation log1p exists for: exact for tiny x. Total: x = -1 yields negative infinity, x < -1 NaN.",
        ["x" => "The value added to 1 before the logarithm"],
        "log1p(0.1)  // 0.09531017980432487",
    ),
    builtin_doc!(
        "expm1",
        "e^x - 1 computed without the cancellation expm1 exists for: exact for tiny x. Total.",
        ["x" => "The exponent"],
        "expm1(0.1)  // 0.10517091807564763",
    ),
    builtin_doc!(
        "erf",
        "The error function, the Gaussian CDF up to scaling. Total; erf(+inf) is 1.0 and erf(-inf) is -1.0.",
        ["x" => "The float argument"],
        "erf(1.0)  // 0.8427007929497148",
    ),
    builtin_doc!(
        "erfc",
        "1 - erf(x), computed without cancellation so the tail keeps its relative precision. Total.",
        ["x" => "The float argument"],
        "erfc(1.0)  // 0.15729920705028516",
    ),
    builtin_doc!(
        "floor",
        "The greatest integer <= x, as a float. Total: floor of an infinity or NaN is itself.",
        ["x" => "The float to round down"],
        "floor(-2.7)  // -3.0",
    ),
    builtin_doc!(
        "ceil",
        "The least integer >= x, as a float. Total: ceil of an infinity or NaN is itself.",
        ["x" => "The float to round up"],
        "ceil(2.3)  // 3.0",
    ),
    builtin_doc!(
        "trunc",
        "The integer part of x toward zero, as a float. Total.",
        ["x" => "The float to truncate"],
        "trunc(-2.7)  // -2.0",
    ),
    builtin_doc!(
        "round",
        "The nearest integer to x, as a float, with halfway cases rounded to even — Python's round and numpy's rounding, not C's half-away-from-zero. Total.",
        ["x" => "The float to round"],
        "round(2.5)  // 2.0\nround(3.5)  // 4.0",
    ),
    builtin_doc!(
        "pow",
        "x raised to y, C pow semantics: pow(0.0, 0.0) is 1.0, a negative base needs an integer exponent, and an overflow yields infinity — never a fault.",
        ["x" => "The base", "y" => "The exponent"],
        "pow(2.0, 10.0)  // 1024.0",
    ),
    builtin_doc!(
        "hypot",
        "sqrt(x*x + y*y) without intermediate overflow or underflow. Total.",
        ["x" => "The first leg", "y" => "The second leg"],
        "hypot(3.0, 4.0)  // 5.0",
    ),
    builtin_doc!(
        "fmod",
        "The C fmod: the remainder of x/y with x's sign, truncating toward zero. Total: a zero divisor yields NaN, unlike the % operator's Arith policy.",
        ["x" => "The dividend", "y" => "The divisor"],
        "fmod(-7.5, 2.0)  // -1.5",
    ),
    builtin_doc!(
        "copysign",
        "x's magnitude with y's sign. Total; a NaN keeps its payload and takes y's sign bit.",
        ["x" => "The magnitude", "y" => "The sign donor"],
        "copysign(3.0, -1.0)  // -3.0",
    ),
    builtin_doc!(
        "float32",
        "Rounds a float to the nearest binary32 (ties to even) and widens back: the value a float32 pipeline holds. Total and pure, so a kernel may call it or name it.",
        ["x" => "The double to narrow"],
        "float32(0.1)  // 0.10000000149011612",
    ),
    builtin_doc!(
        "floatBits",
        "The binary64 word of x as an int: a bitcast, preserving NaN payloads, signed zeros and infinities.",
        ["x" => "The float to view"],
        "floatBits(1.0)  // 4607182418800017408",
    ),
    builtin_doc!(
        "floatFromBits",
        "The float the binary64 word `bits` holds. The exact inverse of floatBits.",
        ["bits" => "The binary64 word"],
        "floatFromBits(4607182418800017408)  // 1.0",
    ),
    builtin_doc!(
        "float32Bits",
        "The binary32 pattern of the nearest float32 to x (the low 32 bits of the float32 word), as an int.",
        ["x" => "The double to narrow and view"],
        "float32Bits(0.1)  // 1036831949",
    ),
    builtin_doc!(
        "float32FromBits",
        "The float the binary32 pattern `bits` holds, widened exactly. The inverse of float32Bits on float32 values.",
        ["bits" => "The binary32 pattern"],
        "float32FromBits(1036831949)  // 0.10000000149011612",
    ),
    builtin_doc!(
        "floatToInt",
        "Checked truncation toward zero: Success(n) when x is finite and inside the int range, Error for NaN, infinities and out-of-range magnitudes.",
        ["x" => "The float to truncate"],
        "floatToInt(-2.9)  // Success(-2)\nfloatToInt(0.0 / 0.0)  // Error",
    ),
    builtin_doc!(
        "bitAnd",
        "Bitwise and of two ints. Total; performs no Arith operation.",
        ["a" => "The first word", "b" => "The second word"],
        "bitAnd(12, 10)  // 8",
    ),
    builtin_doc!(
        "bitOr",
        "Bitwise or of two ints. Total; performs no Arith operation.",
        ["a" => "The first word", "b" => "The second word"],
        "bitOr(12, 10)  // 14",
    ),
    builtin_doc!(
        "bitXor",
        "Bitwise exclusive or of two ints. Total; performs no Arith operation.",
        ["a" => "The first word", "b" => "The second word"],
        "bitXor(12, 10)  // 6",
    ),
    builtin_doc!(
        "bitNot",
        "One's complement of an int. Total; performs no Arith operation.",
        ["a" => "The word to invert"],
        "bitNot(12)  // -13",
    ),
    builtin_doc!(
        "shiftLeft",
        "a << n, total: a count outside 0..63 yields 0, the limit of repeated shifting. Performs no Arith operation.",
        ["a" => "The word to shift", "n" => "The shift count"],
        "shiftLeft(1, 40)  // 1099511627776",
    ),
    builtin_doc!(
        "shiftRight",
        "Arithmetic right shift a >> n, total: a count outside 0..63 yields 0 for a >= 0 and -1 for a < 0. Performs no Arith operation.",
        ["a" => "The word to shift", "n" => "The shift count"],
        "shiftRight(-16, 2)  // -4",
    ),
    builtin_doc!(
        "shiftRightLogical",
        "Logical right shift of a's bits, total: a count outside 0..63 yields 0. Performs no Arith operation.",
        ["a" => "The word to shift", "n" => "The shift count"],
        "shiftRightLogical(-16, 2)  // 4611686018427387900",
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
        "range(1, 5) |> fold(0, add)  // sum: 0+1+2+3+4 = 10",
    ),
    builtin_doc!(
        "toList",
        "Runs an iterator pipeline once and collects every element it yields into a list, in order.",
        ["iterator" => "The iterator to collect"],
        "range(1, 7) |> filter(fn(x) => x % 2 == 0) |> toList  // [2, 4, 6]",
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
        "structuralEquals",
        "Compares two values by their contents, walking both completely, so its cost grows with their size; `==` deliberately does not do this. Both operands must be one type whose every part has a decided equality — a function, a runtime handle, `any` or `Result` anywhere inside is a compile error. NaN stays unequal to itself, including inside a record, list, map or union.",
        ["a" => "The first value", "b" => "The second value, of the same type"],
        "type Route = { tab: string, depth: int }\nlet same = structuralEquals(Route { tab: \"home\", depth: 1 }, Route { tab: \"home\", depth: 1 })  // true\nlet differ = structuralEquals([1, 2], [1, 2, 3])  // false",
    ),
    builtin_doc!(
        "forEachList",
        "Applies function to every list element in index order.",
        ["list" => "The list", "function" => "Function applied per element"],
        "forEachList(xs, print)",
    ),
    builtin_doc!(
        "mapList",
        "Returns a new list by applying a function to every element, in index order.",
        ["list" => "The list", "function" => "The transformation function"],
        "mapList([1, 2, 3], fn(n) => n * 2)  // [2, 4, 6]",
    ),
    builtin_doc!(
        "filterList",
        "Returns a new list keeping the elements that satisfy a predicate, in index order.",
        ["list" => "The list", "predicate" => "The predicate that returns true for elements to keep"],
        "filterList([1, 2, 3, 4], fn(n) => n % 2 == 0)  // [2, 4]",
    ),
    builtin_doc!(
        "foldList",
        "Reduces a list to a single value by repeatedly applying a function.",
        ["list" => "The list to reduce", "initial" => "The initial value for the accumulator", "function" => "The reduction function that takes (accumulator, current) and returns the new accumulator"],
        "foldList([1, 2, 3], 0, fn(acc, n) => acc + n)  // 6",
    ),
    builtin_doc!(
        "listSort",
        "Returns a new list sorted in ascending order by the elements' natural order: integers by value, strings by byte content. The sort is stable. Only List<int> and List<string> are supported; any other element type is a compile-time error.",
        ["list" => "The list to sort"],
        "listSort([3, 1, 2])  // [1, 2, 3]",
    ),
    builtin_doc!(
        "listSortBy",
        "Returns a new list sorted by a comparator, for any element type. The comparator returns a negative int when its first argument comes first, a positive int when it comes second and zero to keep the two in their original order: the sort is stable.",
        ["list" => "The list to sort", "compare" => "Orders two elements: negative, zero or positive"],
        "listSortBy([\"bb\", \"a\"], fn(x, y) => length(x) - length(y))  // [\"a\", \"bb\"]",
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
/// GPU surface from docs/specs/runtime/0034-GPUComputation.md: [GPU-BUFFER-FROM-LIST],
/// [GPU-BUFFER-TO-LIST], [GPU-BUFFER-LENGTH], [GPU-MAP], [GPU-FOLD],
/// [GPU-ZIPWITH], [GPU-IOTA], [GPU-GET], [GPU-SCAN], [GPU-FILTER],
/// [GPU-DEVICE].
pub(crate) static GPU: &[BuiltinDoc] = &[
    builtin_doc!(
        "toGpu",
        "Copies a list of scalars (int, float, or bool) into a dense GpuBuffer. Also consumes an iterator pipeline, replaying its map/filter stages into the buffer with no list in between.",
        ["source" => "The scalar list or iterator to copy into a buffer"],
        "let buf = toGpu([1, 2, 3, 4])",
    ),
    builtin_doc!(
        "fromGpu",
        "Materializes a GpuBuffer back into a host list, keeping the element type: a float buffer becomes a List<float>.",
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
        "Applies a pure kernel to every buffer element independently, one thread per element when OSPREY_GPU selects Metal or OpenCL. The compiler rejects a kernel that performs any effect a handler must discharge at runtime; an effect a static handler has already erased is gone before the check.",
        ["buffer" => "The source buffer", "kernel" => "The pure per-element function"],
        "toGpu([1, 2, 3]) |> gpuMap(fn(x) => satMul(x, x))",
    ),
    builtin_doc!(
        "gpuFold",
        "Reduces a buffer to one scalar with a pure combine function, applied left to right on every backend. A device runs the fold on one thread, so fold a large buffer by rows and then fold the rows.",
        ["buffer" => "The buffer to reduce", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "toGpu([1, 2, 3]) |> gpuFold(0, fn(a, x) => wrapAdd(a, x))",
    ),
    builtin_doc!(
        "gpuZipWith",
        "Combines two buffers elementwise with a pure binary kernel. The result takes the shorter operand's length.",
        ["a" => "The left buffer", "b" => "The right buffer", "kernel" => "The pure (a, b) element function"],
        "gpuZipWith(xs, ys, fn(x, y) => satMul(x, y))  // elementwise product",
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
        "Inclusive prefix scan: element i is combine folded left to right through element i. It has no device form yet: when OSPREY_GPU selects a device, the run stops and names it.",
        ["buffer" => "The buffer to scan", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "toGpu([1, 2, 3]) |> gpuScan(0, fn(a, x) => wrapAdd(a, x))  // 1, 3, 6",
    ),
    builtin_doc!(
        "gpuFilter",
        "Stream compaction: keeps the elements a pure predicate accepts, preserving order. It has no device form yet: when OSPREY_GPU selects a device, the run stops and names it.",
        ["buffer" => "The source buffer", "predicate" => "The pure element predicate"],
        "toGpu([1, 2, 3, 4]) |> gpuFilter(fn(x) => x % 2 == 0)",
    ),
    builtin_doc!(
        "gpuMatMul",
        "The dense row-major product of a rows x inner float buffer and an inner x cols one, as a rows x cols buffer. Every element accumulates in k order from 0.0 in double; a length that disagrees with the stated shape is an Error naming the operand.",
        ["left" => "The rows x inner buffer", "right" => "The inner x cols buffer", "rows" => "The left operand's row count", "inner" => "The shared dimension", "cols" => "The right operand's column count"],
        "gpuMatMul(x, w, tokens, 768, 2304) ?: empty",
    ),
    builtin_doc!(
        "gpuLoad",
        "Reads count little-endian IEEE-754 binary32 values starting at a byte offset of a file into a GpuBuffer<float>, widening each exactly. A missing file or a short read is an Error carrying the operating system's reason.",
        ["path" => "The file to read", "offset" => "The byte offset the first value starts at", "count" => "How many values to read"],
        "gpuLoad(\"gpt2_124M.bin\", 1024, 768) ?: toGpu([0.0])",
    ),
    builtin_doc!(
        "gpuLoadTensor",
        "Reads one named tensor out of a safetensors file into a GpuBuffer<float>, widening F64, F32, F16 or BF16 storage exactly. An unknown name, another dtype or a truncated file is an Error saying which.",
        ["path" => "The safetensors file", "name" => "The tensor's name in the header"],
        "gpuLoadTensor(\"model.safetensors\", \"wte.weight\") ?: toGpu([0.0])",
    ),
    builtin_doc!(
        "gpuDevice",
        "Returns the name of the backend GPU kernels and products run on: \"host\", or the adapter OSPREY_GPU selected, such as \"metal:0 (Apple M4 Max)\" or \"opencl:0 (Apple M4 Max)\".",
        [],
        "print(gpuDevice())  // host",
    ),
];
