// Assertion-driven replacement for the former pipeline and string goldens.
// The TypeScript twin of text_and_pipelines.test.osp, sharing its golden.
// [FLAVOR-TS-EXPRESSIONS]

/** Increment a pipeline value by one. */
function pipeInc(x: int) {
  return x + 1;
}
/** Double a pipeline value. */
function pipeDouble(x: int) {
  return x + x;
}
/** Square a pipeline value. */
function pipeSquare(x: int) {
  return x * x;
}

/** This case documents left-to-right pipe threading and nested-call equivalence. */
function pipeChainCase() {
  // The flavor has no `|>` spelling, so each pipeline is its nested-call
  // chain; every value and the output stay identical.
  const p1 = pipeDouble(pipeInc(5));
  const p2 = pipeInc(pipeSquare(pipeDouble(3)));
  const p3 = pipeDouble(pipeInc(pipeInc(10)));

  // Preserve the complete three-pipeline golden line.
  checkAll("pipe chain legacy rendered lines", [
    `p1=${p1} p2=${p2} p3=${p3}` === "p1=12 p2=37 p3=24",
  ]);
  // Split the three final pipeline values into independent assertions.
  checkAll("pipe chain legacy constituent values", [
    p1 === 12,
    p2 === 37,
    p3 === 24,
  ]);
  // Exercise intermediate stages, zero input, and a different stage order.
  return checkAll("pipe chain composition edges", [
    pipeInc(5) === 6,
    p1 === pipeDouble(pipeInc(5)),
    pipeDouble(pipeInc(0)) === 2,
    pipeDouble(pipeSquare(3)) === 18,
  ]);
}

/** Append an exclamation mark to a string. */
function stringShout(s) {
  return s + "!";
}
/** Prefix a name with the legacy greeting. */
function stringGreet(who) {
  return "Hi " + who;
}

/** This case documents bindings, nested calls, concatenation, and interpolation. */
function stringsCase() {
  const name = "Ada";
  const hello = stringGreet(name);

  // Preserve the complete strings golden line byte-for-byte.
  checkAll("strings legacy rendered lines", [
    `plain=${hello} loud=${stringShout(hello)} both=${stringShout(stringGreet(name))} concat=${hello + name}` ===
      "plain=Hi Ada loud=Hi Ada! both=Hi Ada! concat=Hi AdaAda",
  ]);
  // Split all four dynamic string values into independent assertions.
  checkAll("strings legacy constituent values", [
    hello === "Hi Ada",
    stringShout(hello) === "Hi Ada!",
    stringShout(stringGreet(name)) === "Hi Ada!",
    hello + name === "Hi AdaAda",
  ]);
  // Exercise empty inputs, equivalent nested calls, and reversed concatenation.
  checkAll("strings composition edges", [
    stringGreet("") === "Hi ",
    stringShout("") === "!",
    stringShout(hello) === stringShout(stringGreet(name)),
    name + hello === "AdaHi Ada",
  ]);
  const rows = [1, 2, 3];
  // A lambda parameter inside an interpolation stays code, whatever its first letter.
  return checkAll("lambdas inside interpolation", [
    `${length(filterList(rows, (row) => row > 1))} kept` === "2 kept",
    `${length(filterList(rows, (name) => name > 2))} kept` === "1 kept",
    `${length(filterList(rows, (thing) => thing > 0))} kept` === "3 kept",
    `${length(filterList(rows, (each) => each > 3))} kept` === "0 kept",
    `${length(filterList(rows, (zero) => zero > 1))}\t${length(filterList(rows, (row) => row > 0))}\n` === "2\t3\n",
    `a\nb${length(filterList(rows, (row) => row > 1))}` === "a\nb2",
  ]);
}

// Register the pipeline composition scenario.
// exactTests' list of { name, work } records has no TypeScript spelling here;
// each record is spelled as the test(name, exactly(work)) call that exactTests
// itself lowers to, so the output is identical.

test("pipelines retain each stage and complete chain", exactly(pipeChainCase));
// Register the string composition scenario.
test("string calls retain interpolation and concatenation", exactly(stringsCase));
