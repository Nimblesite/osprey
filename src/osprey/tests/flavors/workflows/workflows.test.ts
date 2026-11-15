// High-density interaction suite moved from curry_tour, mixed, and
// results_state_hof. It deliberately keeps cross-feature behavior together.
// The TypeScript twin of workflows.test.osp, sharing its golden.

/** Increment an integer in direct calls and pipelines. */
function tourInc(x: int) {
  return x + 1;
}
/** Add two integers through curried application. */
function tourAdd(x: int): (y: int) => int {
  return (y) => x + y;
}
/** Square an integer in direct calls and pipelines. */
function tourSquare(n: int) {
  return n * n;
}
/** Capture a scale factor in a concrete returned closure. */
function tourMakeScaler(k: int): (v: int) => int {
  return (v) => v * k;
}

/** Name two integer literals and stringify the wildcard branch. */
function tourName(n) {
  // Default matches `0 => … 1 => … _ => toString(n)`; a `switch` must test
  // `value.kind`, so the twin spells the integer arms as guard clauses.
  if (n === 0) {
    return "zero";
  }
  if (n === 1) {
    return "one";
  }
  return toString(n);
}

/** Classify parity by matching a modulo-derived boolean. */
function tourParity(n) {
  return (n % 2) === 0 ? "even" : "odd";
}

/** This case integrates arithmetic, closures, matches, pipes, and rendering. */
function curryTourCase() {
  const base = tourAdd(40)(2);
  const scaled = tourMakeScaler(3);

  // Preserve all five complete curry-tour output lines byte-for-byte.
  checkAll("curry tour legacy rendered lines", [
    `base=${toString(base)} inc=${toString(tourInc(base))} sq=${toString(tourSquare(9))}` ===
      "base=42 inc=43 sq=81",
    `scaled7=${toString(scaled(7))} curry=${toString(tourAdd(5)(6))}` ===
      "scaled7=21 curry=11",
    `name0=${tourName(0)} name1=${tourName(1)} name9=${tourName(9)}` ===
      "name0=zero name1=one name9=9",
    `parity7=${tourParity(7)} parity8=${tourParity(8)}` ===
      "parity7=odd parity8=even",
    // Default pipes `10 |> tourInc |> tourSquare`; TypeScript has no pipe
    // operator, so the twin nests the same calls right-to-left.
    `pipe=${toString(tourSquare(tourInc(10)))}` === "pipe=121",
  ]);
  // Split all eleven dynamic values from the five legacy lines.
  checkAll("curry tour legacy constituent values", [
    structuralEquals([base, tourInc(base), tourSquare(9)], [42, 43, 81]),
    structuralEquals([scaled(7), tourAdd(5)(6), tourSquare(tourInc(10))], [21, 11, 121]),
    structuralEquals([tourName(0), tourName(1), tourName(9)], ["zero", "one", "9"]),
    structuralEquals([tourParity(7), tourParity(8)], ["odd", "even"]),
  ]);
  // Extend zero, wildcard, closure, parity, and intermediate-pipe coverage.
  checkAll("curry tour cross-feature edges", [
    structuralEquals([tourInc(0), tourSquare(0), tourMakeScaler(0)(9), tourInc(10)], [1, 0, 0, 11]),
    structuralEquals([tourName(2), tourParity(0)], ["2", "even"]),
  ]);
}

/** Capture a multiplier for the concise mixed-feature workflow. */
function mixedScale(k: int): (v: int) => int {
  return (v) => v * k;
}
/** Advance an integer by one for direct and piped calls. */
function mixedStep(x: int) {
  return x + 1;
}

/** Classify zero and positive values separately from negative values. */
function mixedSign(n: int) {
  return n >= 0 ? "pos" : "neg";
}

/** This case composes a closure, pipeline, match, binding, and interpolation. */
function mixedFeaturesCase() {
  const base = 7;
  const triple = mixedScale(3);
  // Default pipes `base |> mixedStep |> mixedStep`; the twin nests the calls.
  const piped = mixedStep(mixedStep(base));

  // Preserve the complete mixed-feature golden line.
  checkAll("mixed features legacy rendered lines", [
    `triple=${triple(base)} piped=${piped} sign=${mixedSign(base)} mix=${triple(mixedStep(base))}` ===
      "triple=21 piped=9 sign=pos mix=24",
  ]);
  // Split all four dynamic values from the combined line.
  checkAll("mixed features legacy constituent values", [
    structuralEquals([triple(base), piped, triple(mixedStep(base))], [21, 9, 24]),
    mixedSign(base) === "pos",
  ]);
  // Extend sign boundaries, closure factors, and intermediate pipeline state.
  checkAll("mixed features interaction edges", [
    structuralEquals([mixedSign(0), mixedSign(0 - 1)], ["pos", "neg"]),
    structuralEquals([mixedScale(0)(99), mixedStep(base), mixedScale(2)(5)], [0, 8, 10]),
  ]);
}

/** Unwrap a successful modulo Result and map errors to the legacy sentinel. */
function resultSafeMod(a) {
  // Default installs the handler as a statement before the block's value
  // `a % b`; `handle<Arith>(arms, () => body)` is the same handler around it.
  return (b) => handle<Arith>({ remainderByZero: (_) => -1 }, () => a % b);
}

/** Apply a concrete integer function twice. */
function resultTwice(f: (x: int) => int): (x: int) => int {
  return (x) => f(f(x));
}
/** Add ten as the first-class function passed to resultTwice. */
function resultBump(x) {
  return x + 10;
}
/** Capture one operand in a genuinely curried addition closure. */
function resultAdder(a: int): (b: int) => int {
  return (b) => a + b;
}

/** This case composes Result matching, bindings, HOFs, and partial application. */
function resultsStateHigherOrderCase() {
  const rem = resultSafeMod(17)(5);
  const total = rem;
  const addTen = resultAdder(10);

  // Preserve all three complete Result/HOF output lines.
  checkAll("Result and higher-order legacy rendered lines", [
    `rem17_5=${toString(total)} rem20_7=${toString(resultSafeMod(20)(7))}` ===
      "rem17_5=2 rem20_7=6",
    `twiceBump5=${toString(resultTwice(resultBump)(5))}` === "twiceBump5=25",
    `addTen32=${toString(addTen(32))} direct=${toString(resultAdder(7)(8))}` ===
      "addTen32=42 direct=15",
  ]);
  // Split the five dynamic values from those legacy lines.
  checkAll("Result and higher-order legacy constituent values", [
    structuralEquals([total, resultSafeMod(20)(7), resultTwice(resultBump)(5)], [2, 6, 25]),
    structuralEquals([addTen(32), resultAdder(7)(8)], [42, 15]),
  ]);
  // Exercise the error branch, zero remainder, repeated calls, and captures.
  checkAll("Result and higher-order branch edges", [
    resultSafeMod(5)(0) === 0 - 1,
    structuralEquals([resultSafeMod(20)(5), resultTwice(resultBump)(0)], [0, 20]),
    structuralEquals([resultAdder(0)(9), resultAdder(0 - 5)(5)], [9, 0]),
  ]);
}

// The shape of each `{ name, work }` registration below.
interface Case {
  readonly name: string;
  readonly work: () => Unit;
}

// Register the broad curry-tour interaction scenario.

exactTests([
  { name: "curry tour retains all closure match and pipe outputs", work: curryTourCase } satisfies Case,
  // Register the concise mixed-feature interaction scenario.
  { name: "mixed workflow composes closure match pipe and interpolation", work: mixedFeaturesCase } satisfies Case,
  // Register the Result and higher-order interaction scenario.
  { name: "Result workflow covers success error HOF and partial application", work: resultsStateHigherOrderCase } satisfies Case,
]);
