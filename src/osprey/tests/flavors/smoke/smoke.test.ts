// Portable cross-contract oracle for [TYPE-GENERICS-FN], [TYPE-LIST],
// [TYPE-MAP], [TYPE-MAP-LOOKUP], [PATTERN-RESULT-DEFAULT],
// [BUILTIN-STRING-UFCS], [BUILTIN-STRING-LENGTH],
// [BUILTIN-STRING-CONTAINS], and [BUILTIN-STRING-STARTSWITH].
// The TypeScript twin of smoke.test.osp, sharing its golden. [FLAVOR-TS-TYPES]

/** The success value of `found`, or `fallback`. */
function or(found, fallback) {
  switch (found.kind) {
    case "success":
      return found.value;
    case "error":
      return fallback;
  }
}

function atOr<T>(items: T[], index: int, fallback: T): T {
  return or(listGet(items, index), fallback);
}

function bucketAt<T>(buckets: Map<string, T[]>, key: string, index: int, fallback: T): T {
  return or(listGet(or(mapGet(buckets, key), [fallback]), index), fallback);
}

function nestedAt<T>(matrix: T[][], row: int, column: int, fallback: T): T {
  return or(listGet(or(listGet(matrix, row), [fallback]), column), fallback);
}

function chooseByList<T, U>(items: T[], present: U, empty: U): U {
  return isEmpty(items) ? empty : present;
}

/** This case retains both smoke-test output lines and probes the bound greeting. */
function helloSmokeCase() {
  const greeting = "Hello from the ML flavor";
  const words = ["Hello", "from", "the", "ML", "flavor"];
  const nestedWords = [words, ["portable", "IR", "wasm"]];
  const wordBuckets = mapSet(mapSet(new Map<string, string[]>(), "greeting", words), "targets", ["IR", "wasm"]);
  const numbers = [2, 3, 5, 8];
  const numberBuckets = mapSet(mapSet(new Map<string, int[]>(), "small", numbers), "large", [13, 21, 34]);

  // Reconstruct both complete legacy lines byte-for-byte as string values.
  checkAll("hello legacy rendered lines", [
    greeting === "Hello from the ML flavor",
    `2 + 3 = ${2 + 3}` === "2 + 3 = 5",
  ]);
  // Split the two dynamic constituents from their rendered presentation.
  checkAll("hello legacy constituent values", [
    greeting === "Hello from the ML flavor",
    2 + 3 === 5,
  ]);
  // Confirm useful boundaries of the greeting rather than only whole equality.
  checkAll("hello greeting edge cases", [
    startsWith(greeting, "Hello"),
    contains(greeting, "ML flavor"),
    (greeting === "") === false,
  ]);

  checkAll("generic list lookups preserve unrelated element types", [
    listLength(words) === 5,
    atOr(words, 0, "missing") === "Hello",
    atOr(words, 3, "missing") === "ML",
    atOr(words, 4, "missing") === "flavor",
    atOr(words, 9, "missing") === "missing",
    listLength(numbers) === 4,
    atOr(numbers, 0, 99) === 2,
    atOr(numbers, 1, 99) === 3,
    atOr(numbers, 3, 99) === 8,
    atOr(numbers, 9, 99) === 99,
    chooseByList(words, 41, 99) === 41,
    chooseByList(numbers, "present", "empty") === "present",
    chooseByList([], true, false) === false,
  ]);

  checkAll("nested generic lists keep their inner descriptors", [
    listLength(nestedWords) === 2,
    listLength(atOr(nestedWords, 0, ["missing"])) === 5,
    listLength(atOr(nestedWords, 1, ["missing"])) === 3,
    nestedAt(nestedWords, 0, 0, "missing") === "Hello",
    nestedAt(nestedWords, 0, 3, "missing") === "ML",
    nestedAt(nestedWords, 1, 0, "missing") === "portable",
    nestedAt(nestedWords, 1, 2, "missing") === "wasm",
    nestedAt(nestedWords, 9, 0, "fallback") === "fallback",
  ]);

  return checkAll("maps of generic lists compose with interpolation and Result fallback", [
    mapLength(wordBuckets) === 2,
    mapLength(numberBuckets) === 2,
    bucketAt(wordBuckets, "greeting", 0, "missing") === "Hello",
    bucketAt(wordBuckets, "greeting", 4, "missing") === "flavor",
    bucketAt(wordBuckets, "targets", 0, "missing") === "IR",
    bucketAt(wordBuckets, "targets", 1, "missing") === "wasm",
    bucketAt(wordBuckets, "missing", 0, "fallback") === "fallback",
    bucketAt(numberBuckets, "small", 0, 99) === 2,
    bucketAt(numberBuckets, "small", 3, 99) === 8,
    bucketAt(numberBuckets, "large", 2, 99) === 34,
    bucketAt(numberBuckets, "missing", 0, 99) === 99,
    `${bucketAt(wordBuckets, "targets", 0, "missing")}/${bucketAt(wordBuckets, "targets", 1, "missing")}` === "IR/wasm",
    `${bucketAt(numberBuckets, "small", 0, 99) + bucketAt(numberBuckets, "small", 1, 99)}` === "5",
    listLength(mapKeys(wordBuckets)) === 2,
    listLength(mapValues(numberBuckets)) === 2,
  ]);
}

// Run the paired flavor and interpolation smoke test.

test("top-level binding and interpolation retain both hello lines", exactly(helloSmokeCase));
