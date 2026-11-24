// Assertion-driven move of the closure, partial application, higher-order,
// nested-call, and recursion Default-flavor examples. The TypeScript twin of
// functions.test.osp, sharing its golden. [FLAVOR-TS-FUNCTIONS]

/** The success value of `found`, or `fallback`. */
function or(found, fallback) {
  switch (found.kind) {
    case "success":
      return found.value;
    case "error":
      return fallback;
  }
}

/** Build a concrete closure that captures one integer offset. */
function closureAdder(n: int): (x: int) => int {
  return (x) => x + n;
}

/** This case proves closure capture, independence, and direct invocation. */
function closuresCase() {
  const add10 = closureAdder(10);
  const add100 = closureAdder(100);

  // Preserve the full closures golden line as an interpolated string.
  checkAll("closures legacy rendered lines", [
    `a=${add10(5)} b=${add100(5)} c=${closureAdder(3)(4)}` === "a=15 b=105 c=7",
  ]);
  // Split all three captured-call results into independent assertions.
  checkAll("closures legacy constituent values", [
    add10(5) === 15,
    add100(5) === 105,
    closureAdder(3)(4) === 7,
  ]);
  // Exercise zero, cancellation, identity, and independent captures.
  checkAll("closures capture edge cases", [
    add10(0 - 10) === 0,
    add100(0) === 100,
    closureAdder(0)(9) === 9,
    (add10(5) === add100(5)) === false,
  ]);
}

/** Return an explicitly curried addition closure for partial application. */
function partialAdd(a: int): (b: int) => int {
  return (b) => a + b;
}

/** This case documents partial and fully applied addition paths. */
function curryPartialCase() {
  const add5 = partialAdd(5);
  const add100 = partialAdd(100);

  // Preserve the complete partial-application golden line.
  checkAll("partial application legacy rendered lines", [
    `a=${add5(10)} b=${add5(0)} c=${add100(23)} full=${partialAdd(7)(8)}` ===
      "a=15 b=5 c=123 full=15",
  ]);
  // Check each of the four formerly interpolated results separately.
  checkAll("partial application legacy constituent values", [
    add5(10) === 15,
    add5(0) === 5,
    add100(23) === 123,
    partialAdd(7)(8) === 15,
  ]);
  // Extend partial application across zero, cancellation, and equivalence.
  checkAll("partial application edge cases", [
    add5(0 - 5) === 0,
    add100(0) === 100,
    partialAdd(0)(9) === 9,
    partialAdd(5)(5) === partialAdd(8)(2),
  ]);
}

/** Apply a supplied unary integer function exactly once. */
function hofApply(f: (x: int) => int): (x: int) => int {
  return (x) => f(x);
}
/** Apply a supplied unary integer function twice in sequence. */
function hofTwice(f: (x: int) => int): (x: int) => int {
  return (x) => f(f(x));
}
/** Double an integer for use as a first-class function value. */
function hofDouble(n: int) {
  return n + n;
}
/** Increment an integer for use as a first-class function value. */
function hofInc(n: int) {
  return n + 1;
}

/** This case documents function arguments, nested application, and composition. */
function higherOrderCase() {
  // Preserve the complete higher-order golden line.
  checkAll("higher-order legacy rendered lines", [
    `a=${hofApply(hofDouble)(5)} b=${hofApply(hofInc)(9)} t=${hofTwice(hofDouble)(3)} ti=${hofTwice(hofInc)(40)}` ===
      "a=10 b=10 t=12 ti=42",
  ]);
  // Check all four application results that were formerly combined.
  checkAll("higher-order legacy constituent values", [
    hofApply(hofDouble)(5) === 10,
    hofApply(hofInc)(9) === 10,
    hofTwice(hofDouble)(3) === 12,
    hofTwice(hofInc)(40) === 42,
  ]);
  // Exercise zero inputs, repeated application, and composed function values.
  checkAll("higher-order edge cases", [
    hofApply(hofDouble)(0) === 0,
    hofTwice(hofInc)(0) === 2,
    hofTwice(hofDouble)(1) === 4,
    hofApply(hofInc)(hofApply(hofDouble)(3)) === 7,
  ]);
}

/** Increment a value used in deeply nested call expressions. */
function nestedInc(x: int) {
  return x + 1;
}
/** Double a value used in deeply nested call expressions. */
function nestedDouble(x: int) {
  return x + x;
}
/** Negate a value without relying on a standalone unary helper. */
function nestedNeg(x: int) {
  return 0 - x;
}

/** This case documents evaluation through multiple nested unary calls. */
function nestedCallsCase() {
  const deep = nestedInc(nestedDouble(nestedInc(nestedDouble(3))));

  // Preserve the complete nested-call golden line.
  checkAll("nested calls legacy rendered lines", [
    `deep=${deep} mix=${nestedNeg(nestedInc(nestedDouble(5)))} two=${nestedDouble(nestedDouble(4))}` ===
      "deep=15 mix=-11 two=16",
  ]);
  // Check each deeply derived integer independently.
  checkAll("nested calls legacy constituent values", [
    deep === 15,
    nestedNeg(nestedInc(nestedDouble(5))) === 0 - 11,
    nestedDouble(nestedDouble(4)) === 16,
  ]);
  // Exercise each primitive and additional nested paths around zero.
  checkAll("nested calls edge cases", [
    nestedInc(0) === 1,
    nestedDouble(0) === 0,
    nestedNeg(0) === 0,
    nestedInc(nestedDouble(2)) === 5,
    nestedNeg(nestedInc(nestedDouble(0))) === 0 - 1,
  ]);
}

/** Sum positive integers recursively with an explicit zero base case. */
function recursiveSumTo(n: int) {
  // Default matches `0 => 0  _ => …`; a guard clause spells the same branches.
  if (n === 0) {
    return 0;
  }
  return n + recursiveSumTo(n - 1);
}

/** This case documents both the recursive branch and its terminating base. */
function recursionCase() {
  // Preserve the complete recursive-sum golden line.
  checkAll("recursion legacy rendered lines", [
    `sum5=${recursiveSumTo(5)} sum10=${recursiveSumTo(10)} sum1=${recursiveSumTo(1)}` ===
      "sum5=15 sum10=55 sum1=1",
  ]);
  // Check all three sums that were formerly interpolated together.
  checkAll("recursion legacy constituent values", [
    recursiveSumTo(5) === 15,
    recursiveSumTo(10) === 55,
    recursiveSumTo(1) === 1,
  ]);
  // Add base, short, longer, and recurrence-relation assertions.
  checkAll("recursion edge cases", [
    recursiveSumTo(0) === 0,
    recursiveSumTo(2) === 3,
    recursiveSumTo(6) === 21,
    recursiveSumTo(6) === recursiveSumTo(5) + 6,
  ]);
}

/** Identity as a GENERIC definition — no single concrete ABI, so it exists
 * only as a body its call sites specialise ([TYPE-GENERICS-FN]). */
function genericIdentity(x) {
  return x;
}

/** A file-scope binding of that definition. `gen_bind` materialises NO value
 * for it: a generic function has no closure cell, so the name resolves
 * through the call-alias table instead. That table is populated while `main`
 * is lowered — which is AFTER every function body — so a function reading
 * `idAlias` found it empty and emitted a direct call to `@idAlias`, a symbol
 * no definition produces, and the module failed to link. The module global
 * the same reader triggered could not be filled either, so publication failed
 * with `unknown name`. Implements [MODULES-FILE-SCOPE-BINDING]. */
const idAlias = genericIdentity;

/** The same hole for a still-generic file-scope LAMBDA, whose inline body is
 * per-function state that `begin_function` clears. */
const idLambda = (x) => x;

/** Read the alias from inside a function body, before `main` runs. */
function throughAlias(x) {
  return idAlias(x);
}

/** Read the generic lambda from inside a function body. */
function throughLambda(x) {
  return idLambda(x);
}

/** A function must read a runtime list from file-scope storage, not the
 * initializer's private flat-literal layout. */
const globalPair = [1, 2];
function globalHead() {
  // Default matches `[] => 0  [head, …] => head`; TypeScript has no list
  // patterns, so the twin guards on the length and reads the first element.
  if (globalPair.length === 0) {
    return 0;
  }
  return or(listGet(globalPair, 0), 0);
}

/** A generic call nested inside its OWN argument is not recursion: the
 * re-entry guard was armed before the argument was lowered, so this was
 * rejected as "`genericIdentity` is recursive but its signature is not fully
 * inferred". */
function nestedGeneric(x) {
  return genericIdentity(genericIdentity(x));
}

/** A parameter spelled like a two-argument builtin. Callability must be
 * LEXICAL: this parameter may not change `contains(a, b)` anywhere else in
 * the program ([FLAVOR-ML-CALL]). */
function shadowsBuiltin(contains) {
  return contains;
}

/** The builtin call that the parameter above must leave alone. */
function builtinAfterShadow() {
  return contains("alpha", "ph");
}

/** Apply a curried function through a parameter — the case that requires a
 * parameter to stay callable INSIDE its own scope. */
function applyTwo(f, a, b) {
  return f(a)(b);
}

/** A function handed down through a recursive generic helper stays the one
 * each call passed: two calls with different functions share no callee. */
function walkWith(n, step, acc) {
  return n <= 0 ? acc : restWith(wrapSub(n, 1), step, listAppend(acc, step(n)));
}

/** The recursive helper `walkWith` hands its function to. */
function restWith(n, step, acc) {
  return n <= 0 ? acc : restWith(wrapSub(n, 1), step, listAppend(acc, step(n)));
}

function addThree(x) {
  return wrapAdd(x, 3);
}

function timesTwo(x) {
  return wrapMul(x, 2);
}

function total(xs) {
  return foldList(xs, 0, (a, b) => wrapAdd(a, b));
}

/** This case pins file-scope generic function values and lexical callability. */
function genericValueCase() {
  const curriedSum = (a) => (b) => a + b;
  checkAll("file-scope generic function values resolve by name", [
    throughAlias(7) === 7,
    throughAlias("osp") === "osp",
    throughLambda(9) === 9,
    throughLambda("ml") === "ml",
    nestedGeneric(21) === 21,
    nestedGeneric("x") === "x",
    genericIdentity(genericIdentity(genericIdentity(5))) === 5,
    globalHead() === 1,
  ]);
  checkAll("a parameter never changes calls outside its own scope", [
    shadowsBuiltin(3) === 3,
    shadowsBuiltin("keep") === "keep",
    builtinAfterShadow(),
    contains("alpha", "ph"),
    !contains("alpha", "zz"),
    applyTwo(curriedSum, 20, 22) === 42,
    total(walkWith(3, addThree, [])) === 15,
    total(walkWith(3, timesTwo, [])) === 12,
  ]);
}

// The shape of each `{ name, work }` registration below.
interface Case {
  readonly name: string;
  readonly work: () => Unit;
}

// Register the closure capture scenario.

exactTests([
  { name: "closures retain independent captured state", work: closuresCase } satisfies Case,
  // Register the partial-application scenario.
  { name: "partial application retains every curried golden", work: curryPartialCase } satisfies Case,
  // Register the higher-order function scenario.
  { name: "higher-order functions retain every application golden", work: higherOrderCase } satisfies Case,
  // Register the nested-call scenario.
  { name: "nested calls retain evaluation structure", work: nestedCallsCase } satisfies Case,
  // Register the recursive-sum scenario.
  { name: "recursion retains base and recursive branches", work: recursionCase } satisfies Case,
]);

// Register the file-scope generic function value scenario.
test("file-scope generic function values resolve by name", exactly(genericValueCase));
