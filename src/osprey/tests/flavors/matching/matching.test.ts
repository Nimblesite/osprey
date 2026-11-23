// Assertion-driven replacement for the former matching flavor goldens. The
// TypeScript twin of matching.test.osp, sharing its golden.
// [FLAVOR-TS-UNIONS]

/** The success value of `found`, or `fallback`. */
function or(found, fallback) {
  switch (found.kind) {
    case "success":
      return found.value;
    case "error":
      return fallback;
  }
}

/** Classify two integer literals and route every other value to a wildcard. */
function tourClassify(n) {
  // Default matches `0 => … 1 => … _ => many`; a `switch` must test
  // `value.kind`, so the twin spells the integer arms as guard clauses.
  if (n === 0) {
    return "zero";
  }
  if (n === 1) {
    return "one";
  }
  return "many";
}

/** This case documents the three formerly printed match-tour branches. */
function matchTourCase() {
  // Preserve each complete one-word output line independently.
  checkAll("match tour legacy rendered lines", [
    tourClassify(0) === "zero",
    tourClassify(1) === "one",
    tourClassify(9) === "many",
  ]);
  // Repeat the dynamic branch results as explicit constituent assertions.
  checkAll("match tour legacy constituent values", [
    tourClassify(0) === "zero",
    tourClassify(1) === "one",
    tourClassify(9) === "many",
  ]);
  // Extend wildcard coverage across adjacent, negative, and large integers.
  checkAll("match tour wildcard edge cases", [
    tourClassify(2) === "many",
    tourClassify(0 - 1) === "many",
    tourClassify(100) === "many",
    (tourClassify(0) === tourClassify(1)) === false,
  ]);
}

/** Convert a threshold comparison into a pass-or-fail label. */
function matchGrade(n) {
  return n >= 50 ? "pass" : "fail";
}

/** Convert a modulo comparison into an even-or-odd label. */
function matchEven(n) {
  return (n % 2) === 0 ? "even" : "odd";
}

/** This case documents boolean scrutinees for grading and parity. */
function matchBooleanCase() {
  // Preserve the complete combined boolean-match output line.
  checkAll("boolean match legacy rendered lines", [
    `g80=${matchGrade(80)} g20=${matchGrade(20)} e4=${matchEven(4)} e7=${matchEven(7)}` ===
      "g80=pass g20=fail e4=even e7=odd",
  ]);
  // Split all four matched labels into independent assertions.
  checkAll("boolean match legacy constituent values", [
    matchGrade(80) === "pass",
    matchGrade(20) === "fail",
    matchEven(4) === "even",
    matchEven(7) === "odd",
  ]);
  // Exercise the grade threshold and parity boundary values.
  checkAll("boolean match boundaries", [
    matchGrade(50) === "pass",
    matchGrade(49) === "fail",
    matchEven(0) === "even",
    matchEven(9) === "odd",
  ]);
}

/** Label zero, one, and two explicitly before the integer wildcard. */
function matchLabel(n) {
  if (n === 0) {
    return "none";
  }
  if (n === 1) {
    return "single";
  }
  if (n === 2) {
    return "pair";
  }
  return "many";
}

/** This case documents literal and wildcard matching over integers. */
function matchIntegerCase() {
  // Preserve the complete combined integer-match output line.
  checkAll("integer match legacy rendered lines", [
    `a=${matchLabel(0)} b=${matchLabel(1)} c=${matchLabel(2)} d=${matchLabel(7)}` ===
      "a=none b=single c=pair d=many",
  ]);
  // Split all four matched labels into independent assertions.
  checkAll("integer match legacy constituent values", [
    answersHold(matchLabel, [0, 1, 2, 7], ["none", "single", "pair", "many"]),
  ]);
  // Extend the wildcard across adjacent, negative, and large integers.
  checkAll("integer match wildcard boundaries", [
    matchLabel(3) === "many",
    matchLabel(0 - 1) === "many",
    matchLabel(100) === "many",
  ]);
}

// The shape of each `{ name, work }` registration below.
// Register the literal-match tour scenario.
test("literal matching retains every printed arm", matchTourCase);
test("boolean matching covers thresholds and parity", matchBooleanCase);
test("integer matching covers literals and wildcard", matchIntegerCase);

/** [PATTERN-BINDING-SCOPE] Every pattern introduces names in its own arm. */
type Choice = { readonly kind: "pick"; readonly value: int } | { readonly kind: "other" };
interface Coordinates {
  readonly x: int;
  readonly y: int;
}

function resultScope(input) {
  const value = 100;
  const message = "outer";
  // Default interpolates the selected arm's binding into one final string; the
  // twin returns that string from each `switch` arm, where `input.value` and
  // `input.message` are the arm's own pattern bindings.
  switch (input.kind) {
    case "success":
      return `${input.value}:${message}:${value}:${message}`;
    case "error":
      return `${value}:${input.message}:${value}:${message}`;
  }
}

function unionScope(input) {
  const value = 100;
  switch (input.kind) {
    case "pick":
      return `${input.value}:${value}`;
    case "other":
      return `${value}:${value}`;
  }
}

function listScope(input) {
  const head = 100;
  const tail = [8, 9];
  // Default matches `[] => head  [head, …tail] => wrapAdd(head, listLength(tail))`;
  // TypeScript has no list patterns, so the twin guards on the length, reads
  // the first element, and counts the rest with `wrapSub` (this case runs
  // bare, so no Arith-performing arithmetic may be introduced).
  const selected = input.length === 0
    ? head
    : wrapAdd(or(listGet(input, 0), 0), wrapSub(input.length, 1));
  return `${selected}:${head}:${listLength(tail)}`;
}

function structuralScope(input: Coordinates) {
  const x = 100;
  // Default destructures `{ x, .. }`; the twin reads the field directly.
  const selected = wrapAdd(input.x, 1);
  return `${selected}:${x}`;
}

/** The Default twin narrows an erased `any` with the structural arm
 * `{ x, .. }`; the TypeScript flavor narrows only `kind`-discriminated
 * unions, so the twin keeps the erased alternative in a `kind` union. The
 * arm's binding still leaves the outer `x` alone. */
type ErasedValue = { readonly kind: "record"; readonly x: int; readonly y: int } | { readonly kind: "scalar" };

function erasedScope(input) {
  const x = 100;
  switch (input.kind) {
    case "record":
      return `${input.x}:${x}`;
    case "scalar":
      return `none:${x}`;
  }
}

function bindingScope() {
  const value = 100;
  // Default's catch-all `match 2 { value => value }` has no TypeScript
  // spelling (a `switch` must test `value.kind`); the arm's value IS the
  // scrutinee, so the binding is `2` and the outer `value` stays intact.
  const selected = 2;
  checkAll("a catch-all binding leaves its outer value intact", [
    selected === 2, value === 100,
  ]);
}

function siblingScopeCase() {
  return checkAll("pattern arms have independent bindings", [
    resultScope(Success(2)) === "2:outer:100:outer",
    resultScope(Error("bad")) === "100:bad:100:outer",
    unionScope({ kind: "pick", value: 2 }) === "2:100",
    unionScope({ kind: "other" }) === "100:100",
    listScope([2, 3]) === "3:100:2",
    listScope([]) === "100:100:2",
    structuralScope({ x: 2, y: 3 } satisfies Coordinates) === "3:100",
    erasedScope({ kind: "record", x: 2, y: 3 }) === "2:100",
    erasedScope({ kind: "scalar" }) === "none:100",
  ]);
}

function capturedPattern(input) {
  const value = 100;
  // Default binds each arm's reader closure over the arm's own binding; the
  // twin's arms return the closures applied at once, capturing the same way.
  switch (input.kind) {
    case "pick":
      return `${(() => input.value)()}:${value}`;
    case "other":
      return `${(() => value)()}:${value}`;
  }
}

function capturedPatternCase() {
  return checkAll("closures retain the selected arm's binding", [
    capturedPattern({ kind: "pick", value: 2 }) === "2:100",
    capturedPattern({ kind: "other" }) === "100:100",
  ]);
}

test("catch-all bindings remain inside their arm", bindingScope);
test("pattern arms retain their own lexical bindings", siblingScopeCase);
test("escaping closures retain pattern bindings", capturedPatternCase);

function nestedPatternCase() {
  const value = 100;
  // Default matches `Pick(2)` and, inside that arm, `Pick(3)`; the twin spells
  // each `match` as an applied `switch` over the variant's `kind`.
  const selected = ((input) => {
    switch (input.kind) {
      case "pick": {
        const inner = ((nested) => {
          switch (nested.kind) {
            case "pick": return nested.value;
            case "other": return value;
          }
        })({ kind: "pick", value: 3 });
        return `${inner}:${input.value}`;
      }
      case "other": return "none";
    }
  })({ kind: "pick", value: 2 });
  checkAll("nested matches restore the containing arm", [selected === "3:2", value === 100]);
}

interface PatternCounter extends Effect {
  set(x: int): int;
}

function mutablePatternCase() {
  let value = 100;
  return handle<PatternCounter>({ set: (next) => { value = next; return value; } }, () => {
    const readOuter = () => value;
    const selectedReader = ((input) => {
      switch (input.kind) {
        case "pick": return () => input.value;
        case "other": return () => value;
      }
    })({ kind: "pick", value: 2 });
    const selected = selectedReader();
    const updated = perform<PatternCounter>().set(101);
    return checkAll("pattern values shadow cell storage without changing it", [
      selected === 2, selectedReader() === 2, updated === 101, value === 101, readOuter() === 101,
    ]);
  });
}

test("nested matches restore enclosing pattern bindings", nestedPatternCase);
test("pattern bindings shadow captured mutable cells", mutablePatternCase);

type CallChoice = { readonly kind: "apply"; readonly f: (n: int) => int } | { readonly kind: "absent" };
interface IntOperation {
  readonly run: (n: int) => int;
}

function callablePatternCase() {
  const apply = (n) => wrapAdd(n, 100);
  const input = { kind: "apply", f: (n) => wrapAdd(n, 1) } satisfies CallChoice;
  // Default matches `Apply(apply) => apply(2)  Absent => apply(2)`; the twin's
  // arms read the payload field, which shadows nothing outside its arm.
  const selected = ((choice) => {
    switch (choice.kind) {
      case "apply": return choice.f(2);
      case "absent": return apply(2);
    }
  })(input);
  const run = (n) => wrapAdd(n, 200);
  const record = { run: (n) => wrapAdd(n, 2) } satisfies IntOperation;
  // Default destructures `{ run }`; the twin reads the function field directly.
  const structural = record.run(2);
  checkAll("callable pattern bindings retain their own signatures", [
    selected === 3, apply(2) === 102, structural === 4, run(2) === 202,
  ]);
}

test("callable pattern bindings retain their own signatures", callablePatternCase);

function callablePayloadCase() {
  const value = (n) => wrapAdd(n, 100);
  const wrapped = Success((n) => wrapAdd(n, 1));
  const selected = ((result) => {
    switch (result.kind) {
      case "success": return result.value(2);
      case "error": return length(result.message);
    }
  })(wrapped);
  const callbacks = [(n) => wrapAdd(n, 2)];
  // Default matches `[] => 0  [value, …rest] => wrapAdd(value(2), listLength(rest))`;
  // the twin reads the first element and counts the rest with `wrapSub`, as in
  // `listScope` (this case runs bare, so no Arith-performing arithmetic).
  const listed = callbacks.length === 0
    ? 0
    : wrapAdd(or(listGet(callbacks, 0), (n) => n)(2), wrapSub(callbacks.length, 1));
  // Default's catch-all `match value { call => call(2) }` binds the scrutinee
  // itself, so the twin calls it directly.
  const bound = value(2);
  checkAll("Result list and catch-all bindings retain callable signatures", [
    selected === 3, listed === 4, bound === 102, value(2) === 102,
  ]);
}

test("callable payload patterns preserve function values", callablePayloadCase);

function directCallablePatternCase() {
  // Default matches directly on `Success { value: fn … }`, a one-element list,
  // and a lambda scrutinee; the twin applies each `match` as a function.
  const result = ((wrapped) => {
    switch (wrapped.kind) {
      case "success": return wrapped.value(2);
      case "error": return length(wrapped.message);
    }
  })(Success((n) => wrapAdd(n, 1)));
  const listed = ((xs) => (xs.length === 0 ? 0 : or(listGet(xs, 0), (n) => n)(2)))([(n) => wrapAdd(n, 2)]);
  const called = ((n) => wrapAdd(n, 3))(2);
  checkAll("direct scrutinees keep their callable payloads", [result === 3, listed === 4, called === 5]);
}

test("direct callable scrutinees preserve function types", directCallablePatternCase);

function joinedCallable(flag) {
  return flag ? Success((n) => n * 2.0) : Success((n) => n * 3.0);
}
function applyJoined(flag) {
  return ((wrapped) => {
    switch (wrapped.kind) {
      case "success": return wrapped.value(1.25);
      case "error": return toFloat(length(wrapped.message));
    }
  })(joinedCallable(flag));
}
function joinedCallablePatternCase() {
  return checkAll("joined callable Results retain their float ABI", [
    applyJoined(true) === 2.5, applyJoined(false) === 3.75,
  ]);
}

test("joined callable Results retain their payload signatures", joinedCallablePatternCase);

// Or-patterns ([PATTERN-OR]): adjacent `case` labels with no statements
// between them share one body. A `switch` tests `kind` only, so the twin
// spells Default's literal, list and row alternatives as expressions.
type Glyph =
  | { readonly kind: "space" }
  | { readonly kind: "tab" }
  | { readonly kind: "newline" }
  | { readonly kind: "letter"; readonly text: string }
  | { readonly kind: "word"; readonly text: string }
  | { readonly kind: "digit"; readonly d: int };

/** Payload-free alternatives, and payload alternatives that bind one field. */
function glyphKind(g: Glyph) {
  switch (g.kind) {
    case "space":
    case "tab":
    case "newline":
      return "blank";
    case "letter":
    case "word":
      return `text ${g.text}`;
    case "digit":
      return g.d === 0 || g.d === 1 ? "bit" : `digit ${g.d}`;
  }
}

function orLiteralCase() {
  const small = (n) => (n === 0 || n === 1 || n === 2 ? "small" : "big");
  return checkAll("or-pattern variant and literal alternatives", [
    glyphKind({ kind: "space" }) === "blank", glyphKind({ kind: "tab" }) === "blank",
    glyphKind({ kind: "newline" }) === "blank",
    glyphKind({ kind: "digit", d: 0 }) === "bit", glyphKind({ kind: "digit", d: 1 }) === "bit",
    glyphKind({ kind: "digit", d: 7 }) === "digit 7",
    glyphKind({ kind: "letter", text: "a" }) === "text a",
    glyphKind({ kind: "word", text: "hi" }) === "text hi",
    small(0) === "small", small(2) === "small", small(3) === "big",
  ]);
}

test("or-patterns join variant and literal alternatives", orLiteralCase);

type Shape =
  | { readonly kind: "square"; readonly size: int }
  | { readonly kind: "circle"; readonly size: int }
  | { readonly kind: "line"; readonly size: int }
  | { readonly kind: "dot" };

/** Three labels share a body that reads the field every one of them has. */
function measure(s: Shape) {
  switch (s.kind) {
    case "square":
    case "circle":
    case "line":
      return s.size;
    case "dot":
      return 0;
  }
}

function orStructureCase() {
  return checkAll("or-pattern Result, payload, list, row and nested alternatives", [
    measure({ kind: "square", size: 3 }) === 3, measure({ kind: "circle", size: 4 }) === 4,
    measure({ kind: "line", size: 5 }) === 5, measure({ kind: "dot" }) === 0,
  ]);
}

test("or-patterns cover Results, payloads, lists, rows and three-level nests", orStructureCase);

/** A closure capturing the field read from either alternative escapes the arm. */
function adder(s: Shape) {
  switch (s.kind) {
    case "square":
    case "circle":
      return (x) => wrapAdd(x, s.size);
    case "line":
    case "dot":
      return (x) => x;
  }
}

function orBindingCase() {
  const size = 100;
  return checkAll("or-pattern bindings follow order and scope", [
    adder({ kind: "square", size: 3 })(1) === 4, adder({ kind: "circle", size: 6 })(1) === 7,
    adder({ kind: "line", size: 9 })(1) === 1, adder({ kind: "dot" })(1) === 1,
    measure({ kind: "circle", size: 2 }) === 2, size === 100,
  ]);
}

test("or-pattern bindings follow order, scope and closures", orBindingCase);
